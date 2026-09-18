//! Asking the running compositor what is on screen.
//!
//! Everything here is best-effort: the app has to work with Hyprland not
//! running, so a failure is a message, never a crash.

use std::collections::BTreeSet;
use std::process::Command;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::Action;

/// True when a Hyprland instance appears to be running for this user.
pub fn is_running() -> bool {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

fn run(args: &[&str]) -> Result<String> {
    let out = Command::new("hyprctl").args(args).output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            Error::Hyprctl("hyprctl is not on PATH — is Hyprland installed?".into())
        }
        _ => Error::Hyprctl(e.to_string()),
    })?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::Hyprctl(if err.is_empty() {
            format!("`hyprctl {}` failed", args.join(" "))
        } else {
            err
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawClient {
    #[serde(default)]
    class: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "initialClass")]
    initial_class: String,
}

/// An open window, reduced to what the rule editor needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Window class.
    pub class: String,
    /// Window title.
    pub title: String,
    /// Initial class, which is what a rule usually wants to match.
    pub initial_class: String,
}

/// List open windows.
pub fn clients() -> Result<Vec<Client>> {
    let json = run(&["-j", "clients"])?;
    let raw: Vec<RawClient> =
        serde_json::from_str(&json).map_err(|e| Error::Hyprctl(format!("clients: {e}")))?;
    Ok(raw
        .into_iter()
        .map(|c| Client { class: c.class, title: c.title, initial_class: c.initial_class })
        .collect())
}

/// Distinct window classes currently on screen, sorted.
pub fn classes() -> Result<Vec<String>> {
    let set: BTreeSet<String> = clients()?
        .into_iter()
        .flat_map(|c| [c.class, c.initial_class])
        .filter(|s| !s.is_empty())
        .collect();
    Ok(set.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Layers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawMonitorLayers {
    #[serde(default)]
    levels: std::collections::HashMap<String, Vec<RawLayer>>,
}

#[derive(Debug, Deserialize)]
struct RawLayer {
    #[serde(default)]
    namespace: String,
}

/// Distinct layer namespaces currently mapped, sorted.
///
/// These are what `layershader` and the layer animations take.
pub fn layer_namespaces() -> Result<Vec<String>> {
    let json = run(&["-j", "layers"])?;
    let raw: std::collections::HashMap<String, RawMonitorLayers> =
        serde_json::from_str(&json).map_err(|e| Error::Hyprctl(format!("layers: {e}")))?;

    let set: BTreeSet<String> = raw
        .into_values()
        .flat_map(|m| m.levels.into_values())
        .flatten()
        .map(|l| l.namespace)
        .filter(|s| !s.is_empty())
        .collect();
    Ok(set.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Run a plugin action against the live session.
///
/// `hyprctl dispatch` evaluates its argument as Lua. The plugin's functions
/// return nothing, so `hl.dispatch` rejects the call with a non-zero exit
/// *after* the shader has already been applied — which is why the exit status
/// is deliberately not treated as failure here.
pub fn dispatch(action: &Action) -> Result<()> {
    let expr = action.to_lua_call("hl.plugin.HyprWindowShade");
    let out =
        Command::new("hyprctl").args(["dispatch", &expr]).output().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                Error::Hyprctl("hyprctl is not on PATH — is Hyprland installed?".into())
            }
            _ => Error::Hyprctl(e.to_string()),
        })?;

    // Only a message that clearly is not the known false alarm is surfaced.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let trimmed = stderr.trim();
    if !trimmed.is_empty() && !trimmed.contains("expected a dispatcher") {
        return Err(Error::Hyprctl(trimmed.to_string()));
    }
    Ok(())
}

/// Ask the plugin to drop its compiled shader cache.
pub fn reload_shaders() -> Result<()> {
    dispatch(&Action::ReloadShaders)
}

/// Ask Hyprland to re-read its config.
pub fn reload_config() -> Result<()> {
    run(&["reload"]).map(|_| ())
}

/// Whether the plugin reports itself as loaded.
pub fn plugin_loaded() -> Result<bool> {
    let json = run(&["-j", "plugin", "list"])?;
    Ok(json.contains("HyprWindowShade"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ShaderRef;

    #[test]
    fn clients_json_is_reduced_to_what_rules_need() {
        let json = r#"[{"class":"kitty","title":"zsh","initialClass":"kitty"},
                       {"class":"google-chrome","title":"News","initialClass":"chrome"}]"#;
        let raw: Vec<RawClient> = serde_json::from_str(json).unwrap();
        assert_eq!(raw.len(), 2);
        assert_eq!(raw[1].initial_class, "chrome");
    }

    #[test]
    fn missing_fields_do_not_break_parsing() {
        let raw: Vec<RawClient> = serde_json::from_str(r#"[{"class":"kitty"}]"#).unwrap();
        assert_eq!(raw[0].title, "");
    }

    #[test]
    fn layer_json_is_flattened_across_monitors_and_levels() {
        let json = r#"{
            "DP-1": {"levels": {"0": [{"namespace":"mpvpaper"}], "2": [{"namespace":"waybar"}]}},
            "HDMI-A-1": {"levels": {"2": [{"namespace":"waybar"}, {"namespace":"rofi"}]}}
        }"#;
        let raw: std::collections::HashMap<String, RawMonitorLayers> =
            serde_json::from_str(json).unwrap();
        let set: BTreeSet<String> = raw
            .into_values()
            .flat_map(|m| m.levels.into_values())
            .flatten()
            .map(|l| l.namespace)
            .filter(|s| !s.is_empty())
            .collect();
        let got: Vec<String> = set.into_iter().collect();
        assert_eq!(got, vec!["mpvpaper", "rofi", "waybar"]);
    }

    #[test]
    fn dispatch_builds_a_lua_expression() {
        let a = Action::ToggleLayerShader {
            namespace: "rofi".into(),
            shader: ShaderRef::new("/p/blur.glsl"),
        };
        assert_eq!(
            a.to_lua_call("hl.plugin.HyprWindowShade"),
            r#"hl.plugin.HyprWindowShade.togglelayershader("rofi", "/p/blur.glsl")"#
        );
    }
}
