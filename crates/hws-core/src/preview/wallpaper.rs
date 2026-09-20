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
//!
//! Except the mouse pointer, which Hyprland throws across the screen when any
//! output goes away. That one is put back; see [`Borrowed`].

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

/// A headless output borrowed from the user's session, given back on drop.
///
/// A guard rather than a cleanup at the end of the happy path, because every
/// way out of the capture has to return it: an error reading the monitor list,
/// a `grim` that fails, a panic. A stray monitor left on someone's desktop is
/// far worse than a missing backdrop, and the first version of this leaked one
/// whenever the output could not be identified.
struct Borrowed {
    name: String,
    /// The mode it came up in, which is what was photographed.
    size: (u32, u32),
    /// Where the pointer was before any of this, so it can be put back.
    cursor: Option<(i32, i32)>,
}

impl Drop for Borrowed {
    fn drop(&mut self) {
        let _ = hyprctl::run_args(&["output", "remove", &self.name]);
        if let Some(was) = self.cursor {
            restore_cursor(was);
        }
    }
}

/// Undo the jump the compositor makes when an output is taken away.
///
/// Removing a monitor warps the pointer to the middle of one of the others —
/// `CMonitor::onDisconnect` does it with `force`, so `cursor:no_warps` does
/// not stop it and there is nothing to configure around it. A preview is not
/// allowed to move someone's mouse, so the position is noted before the
/// output is borrowed and restored once it has been given back.
///
/// Only the compositor's own warp is undone. If the pointer is anywhere other
/// than the exact middle of a monitor, the person moved it themselves during
/// the second this took, and moving it back would be the same rudeness in the
/// other direction.
fn restore_cursor(was: (i32, i32)) {
    // The warp lands as the output goes away, which is a moment after
    // `output remove` has answered.
    std::thread::sleep(Duration::from_millis(150));

    let Some(now) = cursor_pos() else { return };
    if now == was || !is_a_monitor_middle(now) {
        return;
    }
    warp_to(was);
}

/// Where the pointer is.
fn cursor_pos() -> Option<(i32, i32)> {
    parse_pos(&hyprctl::run_args(&["cursorpos"]).ok()?)
}

/// Read what `hyprctl cursorpos` prints: `881, 973`.
fn parse_pos(out: &str) -> Option<(i32, i32)> {
    let (x, y) = out.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Whether a position is exactly where a removal would have thrown the mouse.
fn is_a_monitor_middle(pos: (i32, i32)) -> bool {
    hyprctl::monitors().is_ok_and(|monitors| is_middle_of(&monitors, pos))
}

/// The same arithmetic Hyprland does — the monitor's origin plus half its
/// size — rather than anything cleverer, so that a layout this does not
/// predict correctly simply leaves the pointer alone.
fn is_middle_of(monitors: &[hyprctl::Monitor], (x, y): (i32, i32)) -> bool {
    monitors.iter().any(|m| {
        let (mx, my) = (m.x + (m.width / 2) as i32, m.y + (m.height / 2) as i32);
        (mx - x).abs() <= 1 && (my - y).abs() <= 1
    })
}

/// Put the pointer at a position, whichever config the session is written in.
///
/// `hyprctl dispatch` is evaluated as Lua when the user's own config is Lua,
/// and taken as a plain dispatcher line when it is not. Nothing from outside
/// says which, and the wrong spelling fails without doing anything, so both
/// are offered and the first one that is accepted wins.
fn warp_to((x, y): (i32, i32)) {
    if hyprctl::run_args(&["dispatch", &format!("movecursor {x} {y}")]).is_ok() {
        return;
    }
    let lua = format!("hl.dsp.cursor.move({{x = {x}, y = {y}}})");
    let _ = hyprctl::run_args(&["dispatch", &lua]);
}

/// Add a headless output and wait for it to arrive.
fn borrow_output() -> Result<Borrowed> {
    // Two captures overlapping would each see the other's output in the diff
    // below, so only one may be in flight; see `BORROW`.
    let before: Vec<String> = names()?;
    // Before the output exists, because giving it back is what moves the
    // pointer; see `Borrowed`.
    let cursor = cursor_pos();
    hyprctl::run_args(&["output", "create", "headless"])?;

    // Give the output time to arrive and the wallpaper time to be drawn on it.
    // A wallpaper daemon told to cover every output will follow; one pinned to
    // a named monitor will not, and then the photograph comes back flat —
    // which is what `looks_blank` is for.
    std::thread::sleep(Duration::from_millis(900));

    let found = hyprctl::monitors()?.into_iter().find(|m| !before.contains(&m.name));
    match found {
        Some(m) => Ok(Borrowed { name: m.name, size: (m.width, m.height), cursor }),
        // Nothing to give back, so no guard is built — but something did
        // probably arrive late, and the next capture's diff would then adopt
        // it. Remove by the name it would have had.
        None => {
            let _ = hyprctl::run_args(&["output", "remove", "HEADLESS-1"]);
            Err(Error::other("the scratch output never arrived"))
        }
    }
}

/// Photograph the user's wallpaper into `dest`, without their windows on it.
///
/// Runs against the session the app is in, not the preview's compositor — this
/// is the desktop worth looking at. Returns the size of what was photographed.
pub fn capture_desktop(dest: &Path) -> Result<(u32, u32)> {
    let _borrow_lock = BORROW.lock().unwrap_or_else(|e| e.into_inner());
    let output = borrow_output()?;
    grim(&output.name, dest)?;
    Ok(output.size)
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

    // The size comes back from the output that was actually photographed. The
    // user's own monitors are the wrong yardstick: the scratch output comes up
    // in whatever mode the headless backend chooses, and on a large desktop
    // that would set the floor far above what the capture could ever weigh.
    let (width, height) = capture_desktop(&image).ok()?;
    if looks_blank(&image, u64::from(width) * u64::from(height)) {
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

    fn monitor(name: &str, x: i32, y: i32, width: u32, height: u32) -> hyprctl::Monitor {
        hyprctl::Monitor { name: name.into(), focused: false, x, y, width, height }
    }

    #[test]
    fn the_pointer_is_read_back_the_way_hyprctl_prints_it() {
        assert_eq!(parse_pos("881, 973\n"), Some((881, 973)));
        assert_eq!(parse_pos("0, 0"), Some((0, 0)));
        assert_eq!(parse_pos("no such thing"), None);
    }

    #[test]
    fn only_the_compositors_own_warp_is_undone() {
        // Where a removal throws the pointer on a two-monitor desktop.
        let monitors =
            [monitor("DP-3", 0, 0, 2560, 1080), monitor("HDMI-A-1", 2560, 0, 1920, 1080)];
        assert!(is_middle_of(&monitors, (1280, 540)));
        assert!(is_middle_of(&monitors, (3520, 540)));
        // Anywhere else is someone using their mouse, and is left alone.
        assert!(!is_middle_of(&monitors, (1280, 600)));
        assert!(!is_middle_of(&monitors, (337, 911)));
    }

    #[test]
    fn hyprpaper_is_not_offered() {
        // It does not bind to a headless output; see the note on TOOLS.
        assert!(!TOOLS.iter().any(|(n, _)| *n == "hyprpaper"));
    }
}
