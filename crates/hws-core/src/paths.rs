//! Where things live, and how to write to them without losing anything.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// `$XDG_CONFIG_HOME` or `~/.config`.
pub fn config_home() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config"))
}

/// `~/.config/hypr`.
pub fn hypr_dir() -> PathBuf {
    config_home().join("hypr")
}

/// `~/.config/hypr/hyprland.lua`.
pub fn default_lua_config() -> PathBuf {
    hypr_dir().join("hyprland.lua")
}

/// `~/.config/hypr/shaders`.
pub fn default_shader_dir() -> PathBuf {
    hypr_dir().join("shaders")
}

/// `~/.config/hyprwindowshade-gui`, where this app keeps its own settings.
pub fn app_config_dir() -> PathBuf {
    config_home().join("hyprwindowshade-gui")
}

/// Where backups of edited files go.
pub fn backup_dir() -> PathBuf {
    app_config_dir().join("backups")
}

/// Expand a leading `~` and return an absolute path.
pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(rest);
    }
    PathBuf::from(path)
}

/// Replace a `$HOME` prefix with `~` for display.
pub fn contract(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

/// Copy `path` into the backup directory under a timestamped name, and prune
/// old copies of the same file beyond `keep`.
///
/// Returns the backup path, or `None` if the source does not exist yet.
pub fn backup(path: &Path, keep: usize) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let dir = backup_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;

    let stem =
        path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut dest = dir.join(format!("{stem}.{stamp}.bak"));
    // Two saves inside one second must not clobber each other.
    let mut n = 1;
    while dest.exists() {
        dest = dir.join(format!("{stem}.{stamp}-{n}.bak"));
        n += 1;
    }
    std::fs::copy(path, &dest).map_err(|e| Error::io(&dest, e))?;

    prune_backups(&dir, &stem, keep)?;
    Ok(Some(dest))
}

fn prune_backups(dir: &Path, stem: &str, keep: usize) -> Result<()> {
    let prefix = format!("{stem}.");
    let mut mine: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Error::io(dir, e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(&prefix) && n.ends_with(".bak")
                })
                .unwrap_or(false)
        })
        .collect();
    if mine.len() <= keep {
        return Ok(());
    }
    // Names are timestamped, so lexical order is chronological.
    mine.sort();
    let excess = mine.len() - keep;
    for p in mine.into_iter().take(excess) {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

/// Write `contents` to `path` atomically: write a sibling temp file, fsync it,
/// then rename over the target.
///
/// A half-written `hyprland.lua` is a broken session, so this never truncates
/// the real file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = path.with_extension(format!(
        "{}.hws-tmp",
        path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    ));

    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        f.write_all(contents.as_bytes()).map_err(|e| Error::io(&tmp, e))?;
        f.sync_all().map_err(|e| Error::io(&tmp, e))?;
    }

    // Keep the original file mode if there was one.
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(path, e)
    })?;
    Ok(())
}

/// Read a file to a string, with the path in the error.
pub fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_and_contract_are_inverse_for_home_paths() {
        let Some(home) = dirs::home_dir() else { return };
        let p = expand("~/x/y");
        assert_eq!(p, home.join("x/y"));
        assert_eq!(contract(&p), "~/x/y");
    }

    #[test]
    fn expand_leaves_absolute_paths_alone() {
        assert_eq!(expand("/etc/hosts"), PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn atomic_write_replaces_contents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.lua");
        write_atomic(&p, "one").unwrap();
        write_atomic(&p, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        // No temp files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("hws-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
