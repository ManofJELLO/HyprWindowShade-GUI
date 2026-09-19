//! Putting the user's own desktop behind the preview window.
//!
//! A flat colour is honest but uninformative: whether a dissolve really reaches
//! zero alpha, or a tint lifts blacks, only shows against something with light
//! and dark in it. The most representative something is the desktop the shader
//! is actually going to run on.
//!
//! So the preview photographs it. Not the wallpaper *file* — asking the
//! wallpaper daemon which image it is showing works for some of them and not
//! others, and falls apart entirely for the ones that do not show an image at
//! all: a video wallpaper, a shuffling directory, a shader. A screenshot works
//! whatever is behind it, because by then it is just pixels.
//!
//! Photographing the screen itself would catch every window on it, and there is
//! no way to ask for the wallpaper layer alone. So the photograph is taken
//! somewhere nothing has ever been opened: a headless output added to the
//! user's own session for a moment, which arrives with an empty workspace and
//! the wallpaper already drawn on it, and is taken away again immediately.
//! Nothing appears on their screen and nothing of theirs moves — a new output
//! has no windows to shuffle, and removing one that never had any leaves the
//! rest alone.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::hyprctl;
use crate::paths;

/// Tools that can show an image in the preview's compositor.
///
/// `fill` crops to cover, which is the cropping-to-aspect the pane wants: the
/// screenshot is a whole monitor and the pane is a small rectangle of a
/// different shape.
///
/// hyprpaper is absent on purpose. Against a headless output it answers
/// `Monitor HEADLESS-1 has no target: no wp will be created` and paints the
/// output white — measured with the wildcard monitor, with the output named
/// explicitly, with and without an instance signature in its environment, and
/// with another encoder's PNG, while swaybg drew the same file on the same
/// instance without complaint.
const TOOLS: &[(&str, &[&str])] = &[("swaybg", &["-i", "{}", "-m", "fill"]), ("wbg", &["{}"])];

/// Held for the whole borrow of a scratch output.
///
/// The output that has just appeared is identified by diffing the monitor list,
/// so two captures overlapping would each see the other's output and one of
/// them would be left behind — which is exactly what running the live tests in
/// parallel did. Two copies of the app could still collide; one copy cannot.
static BORROW: Mutex<()> = Mutex::new(());

/// Photograph the user's wallpaper into `dest`, without their windows on it.
///
/// Runs against the session the app is in, not the preview's compositor — this
/// is the desktop worth looking at. The output it makes is removed again even
/// when the capture fails, because a stray monitor in someone's session is a
/// far worse thing to leave behind than a missing backdrop.
pub fn capture_desktop(dest: &Path) -> Result<()> {
    // A poisoned lock means another capture panicked mid-borrow; the output it
    // was holding is already lost, and refusing to ever capture again would
    // not bring it back.
    let _borrow = BORROW.lock().unwrap_or_else(|e| e.into_inner());

    let before = names()?;
    hyprctl::run_args(&["output", "create", "headless"])?;

    // Give the new output time to arrive and the wallpaper time to be drawn on
    // it. A wallpaper daemon told to cover every output will follow; one
    // pinned to a named monitor will not, and then this comes back blank —
    // which is why the caller treats a bad photograph as no photograph.
    std::thread::sleep(Duration::from_millis(900));

    names()
        .map(|after| after.into_iter().find(|n| !before.contains(n)))
        .and_then(|new| new.ok_or_else(|| Error::other("the scratch output never arrived")))
        .and_then(|name| {
            let result = grim(&name, dest);
            let _ = hyprctl::run_args(&["output", "remove", &name]);
            result
        })
}

fn grim(output: &str, dest: &Path) -> Result<()> {
    let out = Command::new("grim")
        .args(["-o", output, "-l", "1"])
        .arg(dest)
        .output()
        .map_err(|e| Error::other(format!("grim: {e}")))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::other(if err.is_empty() {
            "grim could not photograph the desktop".to_string()
        } else {
            err
        }));
    }
    Ok(())
}

fn names() -> Result<Vec<String>> {
    Ok(hyprctl::monitors()?.into_iter().map(|m| m.name).collect())
}

/// The program and arguments that show `image`, or `None` when no tool is
/// installed.
pub fn show_command(image: &Path) -> Option<(PathBuf, Vec<String>)> {
    let path = image.to_string_lossy().into_owned();
    TOOLS.iter().find_map(|(name, args)| {
        paths::which(name)
            .map(|program| (program, args.iter().map(|a| a.replace("{}", &path)).collect()))
    })
}

/// Where the photograph of the desktop is kept.
pub fn image_path(dir: &Path) -> PathBuf {
    dir.join("desktop.png")
}

/// Whether a capture looks like it caught nothing.
///
/// A wallpaper daemon pinned to a named monitor will not follow onto the
/// scratch output, and what comes back is then a single flat colour. Decoding
/// the image to find out would mean carrying a PNG decoder for one check, so
/// this goes by size instead: a flat colour compresses to almost nothing,
/// while any real picture does not come close to this floor. Being wrong in
/// the cautious direction costs the backdrop, not the preview.
fn looks_blank(image: &Path, pixels: u64) -> bool {
    let bytes = std::fs::metadata(image).map(|m| m.len()).unwrap_or(0);
    bytes < pixels / 100
}

/// Take the photograph, if there is anything that could show it afterwards.
///
/// Called **before** the preview's compositor is started, and that ordering is
/// not incidental: a nested compositor puts a window on the user's screen for
/// the moment before it is made headless, and photographing the desktop after
/// that catches the preview's own scaffolding in the picture.
///
/// Best-effort: without it the preview still runs, on the flat colour of the
/// pane, and only the backdrop is lost.
pub fn capture_if_showable(dir: &Path) -> Option<()> {
    let image = image_path(dir);
    // Nothing to show it with is reason enough not to take the photograph.
    show_command(&image)?;
    let _ = std::fs::remove_file(&image);

    let pixels = hyprctl::monitors()
        .ok()?
        .iter()
        .map(|m| u64::from(m.width) * u64::from(m.height))
        .max()
        .unwrap_or(0);

    capture_desktop(&image).ok()?;
    if looks_blank(&image, pixels) {
        let _ = std::fs::remove_file(&image);
        return None;
    }
    Some(())
}

/// How to show a photograph already taken.
pub fn prepare(dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    let image = image_path(dir);
    image.is_file().then(|| show_command(&image))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_told_which_file_to_show() {
        for (name, args) in TOOLS {
            let rendered: Vec<String> =
                args.iter().map(|a| a.replace("{}", "/tmp/desktop.png")).collect();
            assert!(
                rendered.iter().any(|a| a == "/tmp/desktop.png"),
                "{name} is never told what to show"
            );
        }
    }

    #[test]
    fn the_screenshot_is_cropped_to_cover_rather_than_squashed() {
        // A whole monitor shown in a small pane of another shape has to be
        // cropped; stretching it would misrepresent every proportion in it.
        let (_, args) = TOOLS.iter().find(|(n, _)| *n == "swaybg").unwrap();
        assert!(args.contains(&"fill"));
    }

    #[test]
    fn a_flat_capture_is_treated_as_no_capture() {
        let d = tempfile::tempdir().unwrap();
        let blank = d.path().join("blank.png");
        // Two and a half million pixels would never compress to this.
        std::fs::write(&blank, vec![0u8; 4_000]).unwrap();
        assert!(looks_blank(&blank, 2_560 * 1_080));

        let photo = d.path().join("photo.png");
        std::fs::write(&photo, vec![0u8; 900_000]).unwrap();
        assert!(!looks_blank(&photo, 2_560 * 1_080));
    }

    #[test]
    fn a_capture_that_never_happened_is_blank() {
        assert!(looks_blank(Path::new("/nonexistent/none.png"), 1_000_000));
    }

    #[test]
    fn hyprpaper_is_not_offered() {
        // It does not bind to a headless output; see the note on TOOLS.
        assert!(!TOOLS.iter().any(|(n, _)| *n == "hyprpaper"));
    }
}
