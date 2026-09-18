//! Reading, describing and editing the `.glsl` files the plugin uses.

pub mod meta;
pub mod patch;
pub mod scan;

pub use meta::{Param, ParamKind, ParamValue, ShaderInfo, KNOWN_UNIFORMS, MOTION_UNIFORMS};
pub use scan::{missing, read, scan_dir};

use std::path::Path;

use crate::error::Result;

/// Apply a parameter change to a shader on disk: read, patch, back up, write.
///
/// Returns the re-parsed shader so the caller can refresh its view.
pub fn set_param_on_disk(
    path: &Path,
    name: &str,
    value: &ParamValue,
    backups_to_keep: usize,
) -> Result<ShaderInfo> {
    let src = crate::paths::read_to_string(path)?;
    let patched = patch::set_param(&src, name, value)?;
    if patched != src {
        patch::save(path, &patched, backups_to_keep)?;
    }
    Ok(read(path))
}

/// Set or clear a shader's `// @duration`, on disk.
pub fn set_duration_on_disk(
    path: &Path,
    seconds: Option<f32>,
    backups_to_keep: usize,
) -> Result<ShaderInfo> {
    let src = crate::paths::read_to_string(path)?;
    let patched = patch::set_duration(&src, seconds);
    if patched != src {
        patch::save(path, &patched, backups_to_keep)?;
    }
    Ok(read(path))
}

/// Set or clear a shader's `// @overlay`, on disk.
pub fn set_overlay_on_disk(
    path: &Path,
    overlay: bool,
    backups_to_keep: usize,
) -> Result<ShaderInfo> {
    let src = crate::paths::read_to_string(path)?;
    let patched = patch::set_overlay(&src, overlay);
    if patched != src {
        patch::save(path, &patched, backups_to_keep)?;
    }
    Ok(read(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_on_disk_round_trips_through_the_parser() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("dim.glsl");
        std::fs::write(&p, "// @param 0 1 0.01 \"Dim\"\nconst float DIM = 0.6;\nvoid main() {}\n")
            .unwrap();

        let info = set_param_on_disk(&p, "DIM", &ParamValue::Scalar(0.25), 3).unwrap();
        let dim = info.params.iter().find(|x| x.name == "DIM").unwrap();
        assert_eq!(dim.value, ParamValue::Scalar(0.25));
        assert!(dim.annotated);

        let info = set_duration_on_disk(&p, Some(0.8), 3).unwrap();
        assert_eq!(info.duration, Some(0.8));

        let info = set_overlay_on_disk(&p, true, 3).unwrap();
        assert!(info.overlay);
    }
}
