use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Per-account unread counts, keyed by account id. Managed app state.
pub type UnreadMap = Mutex<HashMap<String, u32>>;
/// Window label of the last-focused account window (e.g. `wa-default`). Managed app state.
pub type ActiveAccount = Mutex<String>;
/// Serialize account mutations now that window-opening commands run off the UI thread.
pub static MUTATION: Mutex<()> = Mutex::new(());

/// A single WhatsApp account.
///
/// `store_uuid` is `Some` only for non-default accounts (used on macOS >= 14 as the
/// `WKWebsiteDataStore` identifier). It is persisted so the identifier is stable
/// across launches. The `default` account keeps `None` so it uses the default
/// webview store (preserving the pre-multi-account login).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub order: u32,
    #[serde(default)]
    pub store_uuid: Option<[u8; 16]>,
}

/// The persisted accounts file. `next_seq` is a monotonic counter so a removed-then
/// re-added account never reuses a stale id (and thus never collides with a stale
/// profile directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountsFile {
    pub accounts: Vec<Account>,
    pub next_seq: u32,
    /// Preferred startup account; independent of the historical `default` profile id.
    pub startup_account: Option<String>,
}

impl Default for AccountsFile {
    fn default() -> Self {
        Self {
            accounts: vec![Account {
                id: "default".to_string(),
                name: "WhatsApp".to_string(),
                order: 0,
                store_uuid: None,
            }],
            next_seq: 1,
            startup_account: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure mutations (no I/O) — unit-tested.
// ---------------------------------------------------------------------------

/// Append a new account. Allocates `acct-{next_seq}`, bumps `next_seq`, places it
/// after the current highest `order`, and gives it a fresh persisted store UUID.
pub fn add(f: &mut AccountsFile, name: &str) -> Account {
    let id = format!("acct-{}", f.next_seq);
    f.next_seq += 1;
    let order = f
        .accounts
        .iter()
        .map(|a| a.order)
        .max()
        .map_or(0, |m| m + 1);
    let acct = Account {
        id,
        name: name.to_string(),
        order,
        store_uuid: Some(gen_store_uuid()),
    };
    f.accounts.push(acct.clone());
    acct
}

/// Remove an account by id. Refuses to remove the last remaining account, and
/// errors if the id is unknown. Returns the removed account on success.
pub fn remove(f: &mut AccountsFile, id: &str) -> Result<Account, String> {
    if f.accounts.len() <= 1 {
        return Err("cannot remove the last account".into());
    }
    let Some(pos) = f.accounts.iter().position(|a| a.id == id) else {
        return Err(format!("unknown account: {id}"));
    };
    let removed = f.accounts.remove(pos);
    if f.startup_account.as_deref() == Some(id) {
        f.startup_account = None;
    }
    Ok(removed)
}

/// Older files and a removed preference fall back to the first account in UI order.
pub fn startup_account(f: &AccountsFile) -> Option<&Account> {
    f.startup_account
        .as_deref()
        .and_then(|id| f.accounts.iter().find(|a| a.id == id))
        .or_else(|| f.accounts.iter().min_by_key(|a| a.order))
}

pub fn set_startup_account(f: &mut AccountsFile, id: &str) -> Result<(), String> {
    if !f.accounts.iter().any(|a| a.id == id) {
        return Err(format!("unknown account: {id}"));
    }
    f.startup_account = Some(id.to_owned());
    Ok(())
}

/// Rename an account by id. Errors if the id is unknown.
pub fn rename(f: &mut AccountsFile, id: &str, name: &str) -> Result<(), String> {
    let Some(acct) = f.accounts.iter_mut().find(|a| a.id == id) else {
        return Err(format!("unknown account: {id}"));
    };
    acct.name = name.to_string();
    Ok(())
}

/// Sum of unread across all accounts.
pub fn aggregate_unread(map: &HashMap<String, u32>) -> u32 {
    map.values().copied().sum()
}

// ---------------------------------------------------------------------------
// Label / path helpers.
// ---------------------------------------------------------------------------

/// Window label for an account (e.g. `wa-default`).
pub fn window_label(id: &str) -> String {
    format!("wa-{id}")
}

/// Extract the account id from a window label, or `None` if not an account label.
pub fn id_from_label(label: &str) -> Option<&str> {
    label.strip_prefix("wa-")
}

/// The on-disk profile directory for an account (Linux/Windows data_directory).
pub fn profile_dir(app: &AppHandle, id: &str) -> tauri::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join("profiles").join(id))
}

/// Delete an account's profile directory. No-op (and no error) when it does not
/// exist. On macOS the per-identifier WKWebsiteDataStore is left to the system.
#[cfg(not(target_os = "macos"))]
pub fn delete_profile(app: &AppHandle, id: &str) {
    if let Ok(dir) = profile_dir(app, id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(target_os = "macos")]
pub fn delete_profile(app: &AppHandle, id: &str) {
    // macOS uses data_store_identifier; the system owns the store. No-op.
    let _ = (app, id);
}

// ---------------------------------------------------------------------------
// Persistence (mirrors settings.rs).
// ---------------------------------------------------------------------------

fn accounts_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    let dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("accounts.json"))
}

pub fn load(app: &AppHandle) -> AccountsFile {
    accounts_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, f: &AccountsFile) -> tauri::Result<()> {
    let path = accounts_path(app)?;
    let json = serde_json::to_string_pretty(f).expect("serialize accounts");
    std::fs::write(path, json)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Store UUID generation (macOS data_store_identifier).
// ---------------------------------------------------------------------------

/// Generate a non-nil RFC-4122 v4-shaped 16-byte identifier.
///
/// macOS's `WKWebsiteDataStore.dataStoreForIdentifier:` raises
/// `NSInvalidArgumentException` on the all-zeros UUID and wry does not guard it,
/// so this must never return `[0; 16]`. We use the current `SystemTime` nanos
/// XORed with a process-local monotonic counter — NOT `DefaultHasher`, which is
/// both nil-prone and hash-unstable across Rust versions.
pub fn gen_store_uuid() -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    let hi = nanos.rotate_left(17) ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let lo = counter
        .rotate_left(31)
        .wrapping_add(nanos.wrapping_mul(0xD6E8_FEB8_6659_FD93));

    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&hi.to_le_bytes());
    b[8..].copy_from_slice(&lo.to_le_bytes());

    // RFC-4122 v4 bits.
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;

    // The version/variant bits above already guarantee non-nil (b[6] >= 0x40),
    // but guard explicitly in case this is ever refactored.
    if b == [0u8; 16] {
        b[0] = 1;
    }
    debug_assert!(b != [0u8; 16]);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_preference_survives_roundtrip_without_changing_profile_identity() {
        let mut f = AccountsFile::default();
        let second = add(&mut f, "Work");
        set_startup_account(&mut f, &second.id).unwrap();
        let restored: AccountsFile =
            serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(startup_account(&restored), Some(&second));
        assert_eq!(restored.accounts[0].id, "default");
        assert_eq!(restored.accounts[0].store_uuid, None);
    }

    #[test]
    fn missing_or_stale_startup_preference_uses_ui_order() {
        let mut f = AccountsFile::default();
        add(&mut f, "Work");
        f.accounts.reverse();
        assert_eq!(startup_account(&f).unwrap().id, "default");
        f.startup_account = Some("removed-id".into());
        assert_eq!(startup_account(&f).unwrap().id, "default");
    }

    #[test]
    fn removing_preferred_account_selects_remaining_account() {
        let mut f = AccountsFile::default();
        let second = add(&mut f, "Work");
        set_startup_account(&mut f, &second.id).unwrap();
        remove(&mut f, &second.id).unwrap();
        assert_eq!(startup_account(&f).unwrap().id, "default");
        assert_eq!(f.startup_account, None);
    }

    #[test]
    fn startup_selection_rejects_unknown_id_without_mutating_state() {
        let mut f = AccountsFile::default();
        let before = f.clone();
        assert!(set_startup_account(&mut f, "missing").is_err());
        assert_eq!(f, before);
    }

    #[test]
    fn no_startup_account_when_list_is_empty() {
        let mut f = AccountsFile::default();
        f.accounts.clear();
        assert!(startup_account(&f).is_none());
    }

    #[test]
    fn default_file_has_single_default_account() {
        let f = AccountsFile::default();
        assert_eq!(f.accounts.len(), 1);
        let a = &f.accounts[0];
        assert_eq!(a.id, "default");
        assert_eq!(a.name, "WhatsApp");
        assert_eq!(a.order, 0);
        assert_eq!(a.store_uuid, None);
        assert_eq!(f.next_seq, 1);
    }

    #[test]
    fn add_increments_seq_and_order() {
        let mut f = AccountsFile::default();
        let a = add(&mut f, "Work");
        assert_eq!(a.id, "acct-1");
        assert_eq!(a.name, "Work");
        assert_eq!(a.order, 1);
        assert_eq!(f.next_seq, 2);

        let b = add(&mut f, "Personal");
        assert_eq!(b.id, "acct-2");
        assert_eq!(b.order, 2);
        assert_eq!(f.next_seq, 3);
        assert_eq!(f.accounts.len(), 3);
    }

    #[test]
    fn added_account_has_non_nil_store_uuid() {
        let mut f = AccountsFile::default();
        let a = add(&mut f, "Work");
        let uuid = a.store_uuid.expect("non-default account has a store_uuid");
        assert_ne!(uuid, [0u8; 16]);
    }

    #[test]
    fn gen_store_uuid_is_non_nil_and_v4_shaped() {
        let b = gen_store_uuid();
        assert_ne!(b, [0u8; 16]);
        assert_eq!(b[6] & 0xF0, 0x40, "version nibble must be 4");
        assert_eq!(b[8] & 0xC0, 0x80, "variant bits must be 10xxxxxx");
    }

    #[test]
    fn gen_store_uuid_differs_across_calls() {
        let a = gen_store_uuid();
        let b = gen_store_uuid();
        assert_ne!(a, b);
    }

    #[test]
    fn remove_works_and_preserves_others() {
        let mut f = AccountsFile::default();
        let a = add(&mut f, "Work");
        let _b = add(&mut f, "Personal");
        let removed = remove(&mut f, &a.id).unwrap();
        assert_eq!(removed.id, a.id);
        assert_eq!(f.accounts.len(), 2);
        assert!(f.accounts.iter().any(|x| x.id == "default"));
        assert!(f.accounts.iter().any(|x| x.id == "acct-2"));
        assert!(!f.accounts.iter().any(|x| x.id == "acct-1"));
    }

    #[test]
    fn cannot_remove_last_account() {
        let mut f = AccountsFile::default();
        let err = remove(&mut f, "default").unwrap_err();
        assert!(err.contains("last account"));
        assert_eq!(f.accounts.len(), 1);
    }

    #[test]
    fn remove_unknown_id_is_err() {
        let mut f = AccountsFile::default();
        add(&mut f, "Work");
        let err = remove(&mut f, "nope").unwrap_err();
        assert!(err.contains("unknown account"));
        assert_eq!(f.accounts.len(), 2);
    }

    #[test]
    fn rename_updates_name() {
        let mut f = AccountsFile::default();
        rename(&mut f, "default", "Primary").unwrap();
        assert_eq!(f.accounts[0].name, "Primary");
    }

    #[test]
    fn rename_unknown_id_is_err() {
        let mut f = AccountsFile::default();
        let err = rename(&mut f, "nope", "x").unwrap_err();
        assert!(err.contains("unknown account"));
    }

    #[test]
    fn json_roundtrip() {
        let mut f = AccountsFile::default();
        add(&mut f, "Work");
        add(&mut f, "Personal");
        let json = serde_json::to_string(&f).unwrap();
        let back: AccountsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn partial_json_fills_defaults() {
        // Account without store_uuid (serde default = None) inside a custom file.
        let f: AccountsFile = serde_json::from_str(
            r#"{"accounts":[{"id":"default","name":"WhatsApp","order":0}],"next_seq":5}"#,
        )
        .unwrap();
        assert_eq!(f.accounts.len(), 1);
        assert_eq!(f.accounts[0].store_uuid, None);
        assert_eq!(f.next_seq, 5);
    }

    #[test]
    fn empty_json_gives_default() {
        let f: AccountsFile = serde_json::from_str("{}").unwrap();
        assert_eq!(f, AccountsFile::default());
    }

    #[test]
    fn default_account_store_uuid_is_none_after_roundtrip() {
        let f = AccountsFile::default();
        let json = serde_json::to_string(&f).unwrap();
        let back: AccountsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.accounts[0].store_uuid, None);
    }

    #[test]
    fn window_label_format() {
        assert_eq!(window_label("default"), "wa-default");
        assert_eq!(window_label("acct-2"), "wa-acct-2");
    }

    #[test]
    fn id_from_label_round_trips() {
        assert_eq!(id_from_label("wa-default"), Some("default"));
        assert_eq!(id_from_label("wa-acct-2"), Some("acct-2"));
        assert_eq!(id_from_label("settings"), None);
        for id in ["default", "acct-7"] {
            assert_eq!(id_from_label(&window_label(id)), Some(id));
        }
    }

    #[test]
    fn aggregate_unread_sums_all() {
        let mut m = HashMap::new();
        m.insert("default".to_string(), 3);
        m.insert("acct-1".to_string(), 4);
        m.insert("acct-2".to_string(), 0);
        assert_eq!(aggregate_unread(&m), 7);
    }

    #[test]
    fn aggregate_unread_zero_when_all_clear() {
        let mut m = HashMap::new();
        m.insert("default".to_string(), 0);
        m.insert("acct-1".to_string(), 0);
        assert_eq!(aggregate_unread(&m), 0);
        assert_eq!(aggregate_unread(&HashMap::new()), 0);
    }
}
