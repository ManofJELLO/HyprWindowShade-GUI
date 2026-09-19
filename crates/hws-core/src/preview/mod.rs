//! Previewing a shader by running it, in a Hyprland of its own.
//!
//! The alternative was to reimplement the plugin inside the GUI — translate the
//! GLSL to Qt's dialect, bake it, and invent values for the twenty-seven
//! uniforms the plugin fills in. That is an approximation, and an approximation
//! that disagrees with the real thing is worse than no preview at all. So the
//! preview *is* the real thing: a second Hyprland, the plugin loaded into it,
//! and one window with the shader tagged onto it.
//!
//! Three things make it unobtrusive:
//!
//! * **It is headless.** A nested instance starts with one output — a window on
//!   the user's screen. Creating a headless output and then removing that one
//!   leaves the instance running with nowhere to draw but the framebuffer we
//!   capture. Nothing appears on their desktop, and no rule has to be pushed
//!   into their session to hide it.
//! * **It never touches their files.** The shader it renders is a copy in a
//!   runtime directory, written from whatever is staged in the app — which is
//!   how a preview can show an edit that has not been saved.
//! * **It looks like their desktop.** Rounding, gaps, borders, opacity, blur
//!   and their own animation curves are read from the running compositor with
//!   `hyprctl`, so the window in the pane is shaped like the windows around it.

pub mod backdrop;
pub mod config;
pub mod look;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::hyprctl;
use crate::model::TagSlot;
use crate::paths;
use crate::preview::config::PreviewConfig;
use crate::preview::look::Look;

/// The headless output the preview draws on.
const OUTPUT: &str = "HEADLESS-1";

/// How long to wait for the nested instance to register itself.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// What to preview, and how.
#[derive(Debug, Clone)]
pub struct Request {
    /// The shader file being edited, for naming the copy.
    pub original: PathBuf,
    /// Its source, with staged edits already applied.
    pub source: String,
    /// True when the shader drives itself from `progress`.
    pub is_animation: bool,
    /// The plugin `.so` to load.
    pub plugin_so: PathBuf,
    /// Pane size in pixels.
    pub size: (u32, u32),
    /// Seconds the demo window stays open before closing again.
    pub hold_secs: f32,
    /// Whether the app is on a dark theme, so the backdrop matches.
    pub dark: bool,
}

impl Request {
    /// The tag slot this shader should be previewed through.
    pub fn slot(&self) -> TagSlot {
        config::slot_for(self.is_animation, false)
    }
}

/// A running preview, or the absence of one.
#[derive(Default)]
pub struct Preview {
    running: Option<Running>,
}

struct Running {
    compositor: Child,
    backdrop: Option<Child>,
    signature: String,
    socket: String,
    shader: PathBuf,
    dir: PathBuf,
    stop: Arc<AtomicBool>,
    demo: Option<JoinHandle<()>>,
    frame: PathBuf,
}

/// What the UI needs to know about the preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether an instance is up.
    pub running: bool,
    /// The shader being previewed, if any.
    pub shader: Option<String>,
}

impl Preview {
    /// Nothing running.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a preview instance is up.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// What the UI should show about it.
    pub fn status(&self) -> Status {
        match &self.running {
            Some(r) => {
                Status { running: true, shader: Some(r.shader.to_string_lossy().into_owned()) }
            }
            None => Status::default(),
        }
    }

    /// Start a preview, replacing any that is already up.
    ///
    /// Takes a few seconds: a compositor has to come up and report itself.
    /// Callers with a UI thread should run this off it.
    pub fn start(&mut self, request: &Request) -> Result<()> {
        self.stop();

        if !hyprctl::is_running() {
            return Err(Error::other(
                "the preview runs a second Hyprland inside this session, so Hyprland has to be \
                 running first",
            ));
        }
        if !request.plugin_so.exists() {
            return Err(Error::other(format!(
                "{} is not there — install the plugin from the Plugin page first",
                paths::contract(&request.plugin_so)
            )));
        }

        let dir = config::dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;

        let shader = config::shader_path(&request.original);
        paths::write_atomic(&shader, &request.source)?;

        let mut look = Look::from_host();
        look.prune_curves();

        let cfg = PreviewConfig {
            shader: shader.to_string_lossy().into_owned(),
            slot: request.slot(),
            demo_class: ".*".into(),
            plugin_so: request.plugin_so.to_string_lossy().into_owned(),
            background: if request.dark { 0x121216 } else { 0x303038 },
            size: (request.size.0.max(160), request.size.1.max(120)),
            app_pid: std::process::id(),
            look,
        };

        let config_path = dir.join("preview.lua");
        paths::write_atomic(&config_path, &config::render(&cfg))?;

        let before = hyprctl::instances().unwrap_or_default();
        let compositor = spawn_compositor(&config_path, &dir)?;
        let (signature, socket) = match wait_for_instance(&before) {
            Ok(found) => found,
            Err(e) => {
                let mut compositor = compositor;
                let _ = compositor.kill();
                return Err(e);
            }
        };

        let stop = Arc::new(AtomicBool::new(false));
        let mut running = Running {
            compositor,
            backdrop: None,
            signature,
            socket,
            shader,
            dir: dir.clone(),
            stop: Arc::clone(&stop),
            demo: None,
            frame: dir.join("frame.png"),
        };

        if let Err(e) = go_headless(&running.signature) {
            running.shut_down();
            return Err(e);
        }

        // Only now: the gradient is a layer surface, and before this there was
        // an output about to be taken away for it to bind to.
        running.backdrop = backdrop::prepare(&dir, request.dark)
            .and_then(|(program, args)| spawn_on_socket(&running.socket, program, args));

        running.demo = Some(spawn_demo_loop(running.socket.clone(), request.hold_secs, stop));
        self.running = Some(running);
        Ok(())
    }

    /// Rewrite the shader the preview is rendering.
    ///
    /// This is the whole live-update mechanism: the plugin reloads a shader when
    /// its mtime changes, so writing the file is all it takes for the next frame
    /// to show the new values. No restart, and nothing to tell the compositor.
    pub fn set_source(&mut self, source: &str) -> Result<()> {
        let Some(running) = &self.running else {
            return Ok(());
        };
        paths::write_atomic(&running.shader, source)
    }

    /// Grab the current frame, returning the file it was written to.
    pub fn capture(&mut self) -> Result<PathBuf> {
        let Some(running) = &self.running else {
            return Err(Error::other("no preview is running"));
        };

        let out = Command::new("grim")
            .args(["-o", OUTPUT, "-l", "0"])
            .arg(&running.frame)
            .env("WAYLAND_DISPLAY", &running.socket)
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .output()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Error::other(
                    "grim is not installed, and the preview needs it to read frames out of the \
                     nested compositor",
                ),
                _ => Error::other(e.to_string()),
            })?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(Error::other(if err.is_empty() {
                "grim could not capture the preview".into()
            } else {
                err
            }));
        }
        Ok(running.frame.clone())
    }

    /// Stop the preview and clean up after it.
    pub fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.shut_down();
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // A preview that outlives the app would be a compositor nobody can see
        // and nobody can stop.
        self.stop();
    }
}

impl Running {
    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.demo.take() {
            let _ = handle.join();
        }
        if let Some(mut backdrop) = self.backdrop.take() {
            let _ = backdrop.kill();
            let _ = backdrop.wait();
        }
        let _ = self.compositor.kill();
        let _ = self.compositor.wait();
        // The shader copy and the generated config are worth keeping until the
        // next preview overwrites them: if one fails to start, they are the
        // evidence of why.
        let _ = &self.dir;
        let _ = &self.signature;
    }
}

/// Start the nested compositor.
fn spawn_compositor(config: &Path, dir: &Path) -> Result<Child> {
    let log = std::fs::File::create(dir.join("hyprland.log")).ok();

    Command::new("Hyprland")
        .arg("-c")
        .arg(config)
        // Without this the child reads the host's signature and talks to the
        // wrong instance the moment anything in it shells out to hyprctl.
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("HYPRLAND_CMD")
        .stdin(Stdio::null())
        .stdout(log.as_ref().and_then(|f| f.try_clone().ok()).map_or(Stdio::null(), Stdio::from))
        .stderr(log.map_or(Stdio::null(), Stdio::from))
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::other("Hyprland is not on PATH, so there is nothing to run the preview in")
            }
            _ => Error::other(format!("could not start the preview compositor: {e}")),
        })
}

/// Wait for an instance that was not there before, and return it.
fn wait_for_instance(before: &[hyprctl::Instance]) -> Result<(String, String)> {
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(now) = hyprctl::instances() {
            if let Some(new) = now.iter().find(|i| !before.iter().any(|b| b.instance == i.instance))
            {
                return Ok((new.instance.clone(), new.wl_socket.clone()));
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(Error::other(
        "the preview compositor did not come up — see preview/hyprland.log in the runtime \
         directory",
    ))
}

/// Give the instance a headless output and take away the one with a window.
///
/// Both outputs are sized by the monitor rule in the generated config, because
/// a `hyprctl keyword monitor` aimed at the headless output after the fact is
/// accepted and then ignored — which leaves the preview rendering at the
/// default 1920x1080 with the window still laid out for something else.
fn go_headless(signature: &str) -> Result<()> {
    hyprctl::on_instance(signature, &["output", "create", "headless"])?;
    std::thread::sleep(Duration::from_millis(300));
    hyprctl::on_instance(signature, &["output", "remove", "WAYLAND-1"])?;
    std::thread::sleep(Duration::from_millis(400));
    Ok(())
}

/// Open a window, hold it, close it, repeat.
///
/// The loop is the point, not decoration: an open or close shader exists only
/// during the transition, so a preview that shows a window already open shows
/// nothing at all of it. A steady shader is served just as well.
fn spawn_demo_loop(socket: String, hold_secs: f32, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    let hold = Duration::from_secs_f32(hold_secs.clamp(1.0, 60.0));

    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Some(mut client) = spawn_demo_client(&socket) else {
                // No terminal to demo with. Sleeping rather than spinning keeps
                // this from becoming a fork bomb on a bare system.
                sleep_unless_stopped(Duration::from_secs(5), &stop);
                continue;
            };

            sleep_unless_stopped(hold, &stop);

            // SIGTERM rather than SIGKILL: a terminal closes its window on the
            // way out, which is what makes the close animation — and any close
            // shader — actually run.
            let _ = client.kill();
            let _ = client.wait();

            // Long enough for the close animation to finish before the next
            // window opens over the top of it.
            sleep_unless_stopped(Duration::from_millis(900), &stop);
        }
    })
}

fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    let step = Duration::from_millis(100);
    let mut left = total;
    while left > Duration::ZERO && !stop.load(Ordering::SeqCst) {
        let this = step.min(left);
        std::thread::sleep(this);
        left -= this;
    }
}

/// Open the demo window on the nested compositor.
///
/// The client is pointed straight at the nested socket rather than dispatched
/// through `hyprctl`, which under a Lua config evaluates its argument as Lua
/// and so cannot simply be handed a command line.
fn spawn_demo_client(socket: &str) -> Option<Child> {
    let (program, args) = demo_terminal()?;
    spawn_on_socket(socket, program, args)
}

/// Start a client on the preview's compositor rather than the user's own.
///
/// `DISPLAY` goes too, or a toolkit that prefers X11 connects to the session's
/// Xwayland and the window opens on the user's actual screen.
fn spawn_on_socket(socket: &str, program: PathBuf, args: Vec<String>) -> Option<Child> {
    Command::new(program)
        .args(args)
        .env("WAYLAND_DISPLAY", socket)
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
        .env_remove("DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// A terminal to use as the demo window, with the arguments that fill it.
fn demo_terminal() -> Option<(PathBuf, Vec<String>)> {
    const KNOWN: &[(&str, &[&str])] = &[
        ("kitty", &["-o", "font_size=12", "-o", "cursor_blink_interval=0", "-e"]),
        ("foot", &["-e"]),
        ("alacritty", &["-e"]),
        ("ghostty", &["-e"]),
        ("wezterm", &["start", "--"]),
        ("xterm", &["-e"]),
    ];

    let body = "/bin/sh".to_string();
    let script = "-c".to_string();
    // Something with text and a little colour, so a shader that touches
    // contrast or saturation has something to act on.
    let content = "printf '\\n  HyprWindowShade\\n  preview\\n\\n'; \
                   printf '  \\033[31m##\\033[32m##\\033[33m##\\033[34m##\\033[35m##\\033[36m##\\033[0m\\n\\n'; \
                   printf '  the quick brown fox jumps\\n  over the lazy dog\\n'; \
                   exec sleep 3600"
        .to_string();

    KNOWN.iter().find_map(|(name, prefix)| {
        paths::which(name).map(|path| {
            let mut args: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            args.push(body.clone());
            args.push(script.clone());
            args.push(content.clone());
            (path, args)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            original: PathBuf::from("/home/me/.config/hypr/shaders/dim.glsl"),
            source: "const float DIM = 0.5;\n".into(),
            is_animation: false,
            plugin_so: PathBuf::from("/nowhere/HyprWindowShade.so"),
            size: (640, 400),
            hold_secs: 10.0,
            dark: true,
        }
    }

    #[test]
    fn a_steady_shader_previews_on_the_plain_tag() {
        assert_eq!(request().slot(), TagSlot::Shader);
    }

    #[test]
    fn an_animation_previews_on_the_open_tag() {
        let mut r = request();
        r.is_animation = true;
        assert_eq!(r.slot(), TagSlot::Open);
    }

    #[test]
    fn a_missing_plugin_is_refused_before_anything_is_started() {
        let mut p = Preview::new();
        let err = p.start(&request()).unwrap_err().to_string();
        // Either complaint is correct, and which one depends on whether the
        // machine running the tests has a Hyprland session at all.
        assert!(
            err.contains("HyprWindowShade.so") || err.contains("Hyprland has to be running"),
            "{err}"
        );
        assert!(!p.is_running());
    }

    #[test]
    fn setting_the_source_without_a_preview_is_a_no_op() {
        let mut p = Preview::new();
        assert!(p.set_source("const float DIM = 1.0;\n").is_ok());
    }

    #[test]
    fn capturing_without_a_preview_says_so() {
        let mut p = Preview::new();
        assert!(p.capture().is_err());
    }

    #[test]
    fn an_idle_preview_reports_nothing_running() {
        assert_eq!(Preview::new().status(), Status { running: false, shader: None });
    }
}
