//! Whether the gate is on. `commit-review disable` parks it: the hook then
//! lets commits through untouched, until `enable`. A marker file next to
//! the login, in ~/.config/commit-review.

use std::path::PathBuf;

fn marker() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config").join("commit-review").join("disabled"))
}

/// On unless the marker exists; on as well when HOME is unknown, since a
/// gate that fails open is worthless.
pub fn enabled() -> bool {
    !marker().is_ok_and(|p| p.exists())
}

pub fn set(enabled: bool) -> Result<(), String> {
    let path = marker()?;
    if enabled {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    } else {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, "commit-review disable\n").map_err(|e| format!("{}: {e}", path.display()))
    }
}
