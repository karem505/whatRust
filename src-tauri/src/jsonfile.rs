//! Crash-safe JSON persistence shared by `settings`, `accounts` and `applock`.
//!
//! A plain `std::fs::write` truncates the file first, so a crash or power loss
//! mid-write leaves it empty or half-written. `write_atomic` writes a temp file,
//! flushes it, keeps the last good copy as `<name>.bak`, then renames over the
//! target. `read` falls back to that `.bak` when the main file does not parse.

use serde::{de::DeserializeOwned, Serialize};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Outcome of reading a JSON file.
#[derive(Debug, PartialEq)]
pub enum Loaded<T> {
    /// No file (first run).
    Missing,
    Ok(T),
    /// The main file was unreadable; the value came from `<name>.bak`.
    Recovered(T),
    /// The file exists but neither it nor its backup parses.
    Corrupt,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    path.with_file_name(name)
}

pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".bak")
}

fn parse<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let bak = backup_path(path);
    // A missing main file is deliberate (first run, or a factory reset), so the
    // backup must not resurrect it. The rename in `write_atomic` leaves no
    // window in which the main file is absent.
    if !path.exists() {
        return Loaded::Missing;
    }
    if let Some(v) = parse(path) {
        return Loaded::Ok(v);
    }
    match parse(&bak) {
        Some(v) => Loaded::Recovered(v),
        None => Loaded::Corrupt,
    }
}

/// Files written next to `path` that a factory reset must remove as well.
pub fn sidecar_paths(path: &Path) -> Vec<PathBuf> {
    vec![
        backup_path(path),
        sibling(path, ".corrupt"),
        sibling(path, ".tmp"),
    ]
}

/// Atomically replace `path` with `contents`, keeping the previous valid
/// contents as `<name>.bak`.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = sibling(path, ".tmp");
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        // Only a file that still parses may replace the backup, so a corrupt
        // main file never overwrites the last good copy.
        if let Ok(old) = std::fs::read(path) {
            if serde_json::from_slice::<serde::de::IgnoredAny>(&old).is_ok() {
                let _ = std::fs::write(backup_path(path), old);
            }
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    write_atomic(path, &json)
}

/// Keep a copy of an unreadable file next to it (once), so a later save of
/// defaults cannot destroy the only evidence of what was there.
pub fn preserve_corrupt(path: &Path) {
    let dest = sibling(path, ".corrupt");
    if !dest.exists() {
        let _ = std::fs::copy(path, dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct V {
        n: u32,
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whatrust-jsonfile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("f.json")
    }

    #[test]
    fn missing_file_is_missing() {
        assert_eq!(read::<V>(&scratch("missing")), Loaded::Missing);
    }

    #[test]
    fn save_then_read_roundtrips_and_leaves_no_tmp() {
        let p = scratch("roundtrip");
        save(&p, &V { n: 1 }).unwrap();
        assert_eq!(read::<V>(&p), Loaded::Ok(V { n: 1 }));
        assert!(!sibling(&p, ".tmp").exists());
    }

    #[test]
    fn second_save_keeps_previous_as_backup() {
        let p = scratch("bak");
        save(&p, &V { n: 1 }).unwrap();
        save(&p, &V { n: 2 }).unwrap();
        assert_eq!(read::<V>(&p), Loaded::Ok(V { n: 2 }));
        assert_eq!(parse::<V>(&backup_path(&p)), Some(V { n: 1 }));
    }

    #[test]
    fn truncated_file_recovers_from_backup() {
        let p = scratch("recover");
        save(&p, &V { n: 1 }).unwrap();
        save(&p, &V { n: 2 }).unwrap();
        std::fs::write(&p, b"{\"n\": ").unwrap();
        assert_eq!(read::<V>(&p), Loaded::Recovered(V { n: 1 }));
    }

    #[test]
    fn corrupt_main_does_not_overwrite_good_backup() {
        let p = scratch("keepbak");
        save(&p, &V { n: 1 }).unwrap();
        save(&p, &V { n: 2 }).unwrap();
        std::fs::write(&p, b"").unwrap();
        save(&p, &V { n: 3 }).unwrap();
        assert_eq!(read::<V>(&p), Loaded::Ok(V { n: 3 }));
        assert_eq!(parse::<V>(&backup_path(&p)), Some(V { n: 1 }));
    }

    #[test]
    fn corrupt_without_backup_is_corrupt() {
        let p = scratch("corrupt");
        std::fs::write(&p, b"not json").unwrap();
        assert_eq!(read::<V>(&p), Loaded::Corrupt);
    }

    #[test]
    fn deleted_main_is_not_resurrected_from_backup() {
        let p = scratch("nomain");
        save(&p, &V { n: 1 }).unwrap();
        save(&p, &V { n: 2 }).unwrap();
        std::fs::remove_file(&p).unwrap();
        assert_eq!(read::<V>(&p), Loaded::Missing);
    }

    #[test]
    fn sidecar_paths_lists_bak_and_corrupt() {
        let p = Path::new("/x/accounts.json");
        let s = sidecar_paths(p);
        assert!(s.contains(&PathBuf::from("/x/accounts.json.bak")));
        assert!(s.contains(&PathBuf::from("/x/accounts.json.corrupt")));
    }

    #[test]
    fn preserve_corrupt_copies_once() {
        let p = scratch("preserve");
        std::fs::write(&p, b"first").unwrap();
        preserve_corrupt(&p);
        std::fs::write(&p, b"second").unwrap();
        preserve_corrupt(&p);
        assert_eq!(std::fs::read(sibling(&p, ".corrupt")).unwrap(), b"first");
    }
}
