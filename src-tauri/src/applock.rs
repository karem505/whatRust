use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Persisted app-lock configuration. Stored in its own `app-lock.json` (NOT in
/// settings.json) so the password hash never rides along with the general settings
/// blob and is never serialized to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppLockConfig {
    pub enabled: bool,
    /// Argon2id PHC string (salt embedded). `None` when the lock is disabled.
    pub password_phc: Option<String>,
    pub biometric_enabled: bool,
    pub lock_on_launch: bool,
    pub lock_on_hide: bool,
    /// Idle auto-lock threshold in seconds. 0 == off.
    pub idle_secs: u32,
}

impl Default for AppLockConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            password_phc: None,
            biometric_enabled: false,
            lock_on_launch: true,
            lock_on_hide: false,
            idle_secs: 0,
        }
    }
}

impl AppLockConfig {
    /// The lock is only truly active when it is enabled AND a password exists. A
    /// bare `enabled` with no hash can never lock the user out.
    pub fn is_active(&self) -> bool {
        self.enabled && self.password_phc.is_some()
    }
    pub fn should_lock_on_launch(&self) -> bool {
        self.is_active() && self.lock_on_launch
    }
    pub fn should_lock_on_hide(&self) -> bool {
        self.is_active() && self.lock_on_hide
    }
    pub fn should_lock_when_idle(&self, seconds: u64) -> bool {
        self.is_active() && self.idle_secs > 0 && seconds >= u64::from(self.idle_secs)
    }
}

fn config_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("app-lock.json"))
}

/// Stored in place of the hash when `app-lock.json` exists but cannot be read.
/// It never parses as a PHC string, so `verify_password` always fails.
const CORRUPT_PHC: &str = "!corrupt";

/// Fail closed: an unreadable lock file (with no usable backup) keeps the lock
/// ON with a hash nobody can match, instead of silently turning it off. The
/// forgot-password reset is then the way out.
fn corrupt_config() -> AppLockConfig {
    AppLockConfig {
        enabled: true,
        password_phc: Some(CORRUPT_PHC.to_string()),
        ..AppLockConfig::default()
    }
}

pub fn load(app: &AppHandle) -> AppLockConfig {
    use crate::jsonfile::Loaded;
    let Ok(path) = config_path(app) else {
        return AppLockConfig::default();
    };
    match crate::jsonfile::read::<AppLockConfig>(&path) {
        Loaded::Ok(c) | Loaded::Recovered(c) => c,
        Loaded::Missing => AppLockConfig::default(),
        Loaded::Corrupt => corrupt_config(),
    }
}

pub fn save(app: &AppHandle, c: &AppLockConfig) -> tauri::Result<()> {
    let path = config_path(app)?;
    crate::jsonfile::save(&path, c)?;
    Ok(())
}

/// Hash a passcode into an Argon2id PHC string. `Argon2::default()` is Argon2id,
/// version 0x13, with OWASP-baseline parameters. The random salt is embedded in the
/// returned PHC string, so nothing else needs storing.
pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verify a passcode against a stored PHC string. Returns false on any parse/verify
/// error (never panics, never leaks which step failed).
pub fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// Paths a factory reset removes, given whatRust's two app dirs. Pure so it can be
/// unit-tested. `app_data_dir` is whatRust-exclusive (`<base>/com.karem.whatrust`) and
/// holds BOTH the default account's webview store (directly under it, on Linux/Windows)
/// and the per-account `profiles/` dirs — removing it logs every account out.
fn reset_targets(config_dir: &Path, data_dir: &Path) -> Vec<PathBuf> {
    let accounts = config_dir.join("accounts.json");
    let lock = config_dir.join("app-lock.json");
    // The backups must go too, or a later read could bring the old lock back.
    let mut v = crate::jsonfile::sidecar_paths(&accounts);
    v.extend(crate::jsonfile::sidecar_paths(&lock));
    v.extend([accounts, lock, data_dir.to_path_buf()]);
    v
}

/// Request cleanup on the next launch, before any profile is opened.
/// The sibling marker survives even when config_dir == data_dir.
fn reset_marker(config_dir: &Path) -> PathBuf {
    let mut marker = config_dir.as_os_str().to_os_string();
    marker.push(".reset-pending");
    PathBuf::from(marker)
}

pub fn request_reset(app: &AppHandle) -> Result<(), String> {
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let marker = reset_marker(&config);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(marker, b"reset-sessions-v1").map_err(|e| e.to_string())
}

fn complete_reset_paths(config: &Path, data: &Path) -> std::io::Result<bool> {
    let marker = reset_marker(config);
    if !marker.exists() {
        return Ok(false);
    }
    // Remove profiles first. Keep the marker and refuse startup on any failure.
    // Never treat a partially removed (or locked) WebView store as logged out.
    if data.exists() {
        std::fs::remove_dir_all(data)?;
    }
    for p in reset_targets(config, data)
        .into_iter()
        .filter(|p| p.as_path() != data)
    {
        match std::fs::remove_file(p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    std::fs::remove_file(marker)?;
    Ok(true)
}

pub fn complete_pending_reset(app: &AppHandle) -> Result<(), String> {
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    complete_reset_paths(&config,&data).map(|_|()).map_err(|_|"Não foi possível concluir a redefinição. Feche outras instâncias do whatRust e tente novamente.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = AppLockConfig::default();
        assert!(!c.enabled);
        assert!(c.password_phc.is_none());
        assert!(!c.biometric_enabled);
        assert!(c.lock_on_launch);
        assert!(!c.lock_on_hide); // user decision: hide-to-tray lock ships OFF
        assert_eq!(c.idle_secs, 0);
    }

    #[test]
    fn is_active_requires_enabled_and_hash() {
        let mut c = AppLockConfig::default();
        assert!(!c.is_active());
        c.enabled = true;
        assert!(!c.is_active(), "enabled without a hash must not be active");
        c.password_phc = Some("x".into());
        assert!(c.is_active());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let c: AppLockConfig = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert!(c.enabled);
        assert!(c.lock_on_launch);
        assert!(!c.lock_on_hide);
        assert_eq!(c.idle_secs, 0);
    }

    #[test]
    fn empty_json_is_all_defaults() {
        let c: AppLockConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, AppLockConfig::default());
    }

    #[test]
    fn corrupt_config_is_active_and_unmatchable() {
        let c = corrupt_config();
        assert!(c.is_active());
        assert!(!c.biometric_enabled);
        assert!(!verify_password("", CORRUPT_PHC));
        assert!(!verify_password("!corrupt", CORRUPT_PHC));
    }

    #[test]
    fn roundtrip() {
        let c = AppLockConfig {
            enabled: true,
            idle_secs: 300,
            ..Default::default()
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: AppLockConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn hash_then_verify_roundtrips() {
        let phc = hash_password("correct horse").unwrap();
        assert!(phc.starts_with("$argon2id$"), "must be Argon2id: {phc}");
        assert!(verify_password("correct horse", &phc));
    }

    #[test]
    fn wrong_password_fails() {
        let phc = hash_password("secret").unwrap();
        assert!(!verify_password("nope", &phc));
    }

    #[test]
    fn verify_rejects_garbage_phc() {
        assert!(!verify_password("anything", "not-a-phc-string"));
    }

    #[test]
    fn reset_targets_cover_config_files_and_the_data_root() {
        use std::path::Path;
        let cfg = Path::new("/x/config");
        let data = Path::new("/x/data");
        let t = super::reset_targets(cfg, data);
        assert!(t.contains(&cfg.join("accounts.json")));
        assert!(t.contains(&cfg.join("app-lock.json")));
        assert!(t.contains(&cfg.join("app-lock.json.bak")));
        assert!(t.contains(&cfg.join("accounts.json.bak")));
        // The data root must be a target — this is what logs the DEFAULT account out.
        assert!(t.contains(&data.to_path_buf()));
    }
    #[test]
    fn deferred_reset_clears_profiles_before_removing_marker() {
        let dir = std::env::temp_dir().join(format!("whatrust-reset-{}", accounts_test_id()));
        let config = dir.join("config");
        let data = dir.join("data");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(data.join("profiles")).unwrap();
        std::fs::write(config.join("app-lock.json"), b"hash fixture").unwrap();
        std::fs::write(config.join("accounts.json"), b"accounts fixture").unwrap();
        std::fs::write(reset_marker(&config), b"reset-sessions-v1").unwrap();
        assert!(complete_reset_paths(&config, &data).unwrap());
        assert!(!data.exists());
        assert!(!config.join("app-lock.json").exists());
        assert!(!reset_marker(&config).exists());
        assert!(!complete_reset_paths(&config, &data).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_reset_keeps_marker_and_password() {
        let dir = std::env::temp_dir().join(format!("whatrust-reset-{}", accounts_test_id()));
        let config = dir.join("config");
        let data = dir.join("data");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(&data, b"a file cannot be removed as a directory").unwrap();
        std::fs::write(config.join("app-lock.json"), b"hash fixture").unwrap();
        std::fs::write(reset_marker(&config), b"reset-sessions-v1").unwrap();
        assert!(complete_reset_paths(&config, &data).is_err());
        assert!(reset_marker(&config).exists());
        assert!(config.join("app-lock.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn accounts_test_id() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
    #[test]
    fn automatic_lock_triggers_require_an_enabled_password() {
        let mut c = AppLockConfig {
            lock_on_hide: true,
            idle_secs: 60,
            ..Default::default()
        };
        assert!(!c.should_lock_on_launch());
        assert!(!c.should_lock_on_hide());
        assert!(!c.should_lock_when_idle(500));
        c.enabled = true;
        c.password_phc = Some("hash fixture".into());
        assert!(c.should_lock_on_launch());
        assert!(c.should_lock_on_hide());
        assert!(!c.should_lock_when_idle(59));
        assert!(c.should_lock_when_idle(60));
        c.lock_on_launch = false;
        c.lock_on_hide = false;
        c.idle_secs = 0;
        assert!(!c.should_lock_on_launch());
        assert!(!c.should_lock_on_hide());
        assert!(!c.should_lock_when_idle(500));
    }
    #[test]
    fn reset_marker_survives_shared_config_and_data_directory() {
        let config = std::env::temp_dir().join(format!("whatrust-reset-{}", accounts_test_id()));
        std::fs::create_dir_all(config.join("profiles")).unwrap();
        std::fs::write(config.join("app-lock.json"), b"hash fixture").unwrap();
        let marker = reset_marker(&config);
        assert!(!marker.starts_with(&config));
        assert_eq!(
            reset_marker(Path::new("com.karem.whatrust")),
            Path::new("com.karem.whatrust.reset-pending")
        );
        std::fs::write(&marker, b"reset-sessions-v1").unwrap();
        assert!(complete_reset_paths(&config, &config).unwrap());
        assert!(!config.exists());
        assert!(!marker.exists());
    }
}
