//! Best-effort import of hand-written config.
//!
//! Someone arriving with an existing `hyprland.lua` should not have to retype
//! their rules. This reads the parts of the file *outside* the managed block
//! and turns what it recognises into model objects.
//!
//! It is deliberately conservative: it understands string literals and
//! concatenations of literals with `local` variables defined in the same file,
//! and gives up on anything else rather than guessing. Callers are expected to
//! show the result for review before merging it.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::block;
use crate::model::{
    Action, Bind, LayerEntry, Match, ShaderRef, StartupAction, Tag, TagSlot, TagValue, WindowRule,
};

/// What an import found, plus what it could not make sense of.
#[derive(Debug, Clone, Default)]
pub struct Imported {
    /// Window rules, one per `hl.window_rule` call that carried a shader tag.
    pub rules: Vec<WindowRule>,
    /// Layer namespaces mentioned by `layershader` and the layer animations.
    pub layers: Vec<LayerEntry>,
    /// Keybinds calling into the plugin.
    pub binds: Vec<Bind>,
    /// Plugin calls that were not inside a bind.
    pub startup: Vec<StartupAction>,
    /// Lines that looked relevant but could not be read.
    pub skipped: Vec<String>,
}

impl Imported {
    /// True when nothing at all was found.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
            && self.layers.is_empty()
            && self.binds.is_empty()
            && self.startup.is_empty()
    }

    /// A one-line summary for the UI.
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "nothing to import".into();
        }
        let mut parts = Vec::new();
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        if !self.rules.is_empty() {
            parts.push(plural(self.rules.len(), "window rule", "window rules"));
        }
        if !self.layers.is_empty() {
            parts.push(plural(self.layers.len(), "layer", "layers"));
        }
        if !self.binds.is_empty() {
            parts.push(plural(self.binds.len(), "keybind", "keybinds"));
        }
        if !self.startup.is_empty() {
            parts.push(plural(self.startup.len(), "startup call", "startup calls"));
        }
        parts.join(", ")
    }
}

/// Read a whole `hyprland.lua`, ignoring anything inside the managed block.
pub fn from_document(document: &str) -> Imported {
    let outside = match block::split(document) {
        Ok(Some(s)) => format!("{}\n{}", s.before, s.after),
        _ => document.to_string(),
    };
    scan(&outside)
}

/// Import from a source that is already free of the managed block.
pub fn scan(source: &str) -> Imported {
    let locals = collect_locals(source);
    let mut out = Imported::default();
    let mut next = 1usize;
    let mut id = |prefix: &str| {
        let s = format!("{prefix}{next}");
        next += 1;
        s
    };

    // --- window rules ---
    for (body, _) in find_calls(source, "hl.window_rule") {
        match rule_from_table(&body, &locals, &mut id) {
            Ok(Some(rule)) => out.rules.push(rule),
            Ok(None) => {}
            Err(why) => out.skipped.push(why),
        }
    }

    // --- plugin calls ---
    for call in find_plugin_calls(source) {
        let args: Vec<String> = match call
            .args
            .iter()
            .map(|a| eval(a, &locals).ok_or_else(|| a.clone()))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(v) => v,
            Err(bad) => {
                out.skipped.push(format!("{}(…): could not read `{bad}`", call.func));
                continue;
            }
        };

        // Layer shader and layer animations become layer entries; everything
        // else becomes a bind or a startup call.
        let layer_slot = match call.func.as_str() {
            "layershader" => Some(0),
            "layeropenanim" => Some(1),
            "layercloseanim" => Some(2),
            _ => None,
        };

        if let (Some(slot), false) = (layer_slot, call.bind_key.is_some()) {
            if args.len() != 2 {
                out.skipped.push(format!("{}: expected two arguments", call.func));
                continue;
            }
            let ns = args[0].clone();
            let value = &args[1];
            if value == "clear" || value == "none" {
                continue;
            }
            let idx = match out.layers.iter().position(|l| l.namespace == ns) {
                Some(i) => i,
                None => {
                    out.layers.push(LayerEntry::new(id("layer"), ns.clone()));
                    out.layers.len() - 1
                }
            };
            let entry = &mut out.layers[idx];
            let sref = Some(ShaderRef::parse(value));
            match slot {
                0 => entry.shader = sref,
                1 => entry.open_anim = sref,
                _ => entry.close_anim = sref,
            }
            continue;
        }

        let Some(action) = action_from(&call.func, &args) else {
            out.skipped.push(format!("{}: unrecognised arguments", call.func));
            continue;
        };

        match call.bind_key {
            Some(key) => out.binds.push(Bind { id: id("bind"), enabled: true, key, action }),
            None => out.startup.push(StartupAction { id: id("start"), enabled: true, action }),
        }
    }

    out
}

fn action_from(func: &str, args: &[String]) -> Option<Action> {
    let opt = |s: &String| -> Option<ShaderRef> {
        if s == "clear" || s == "none" {
            None
        } else {
            Some(ShaderRef::parse(s))
        }
    };
    match (func, args.len()) {
        ("reloadshaders", 0) => Some(Action::ReloadShaders),
        ("togglewindowshader", 1) => {
            Some(Action::ToggleWindowShader { shader: ShaderRef::parse(&args[0]) })
        }
        ("classshader", 2) => {
            Some(Action::ClassShader { class: args[0].clone(), shader: opt(&args[1]) })
        }
        ("toggleclassshader", 2) => Some(Action::ToggleClassShader {
            class: args[0].clone(),
            shader: ShaderRef::parse(&args[1]),
        }),
        ("layershader", 2) => {
            Some(Action::LayerShader { namespace: args[0].clone(), shader: opt(&args[1]) })
        }
        ("togglelayershader", 2) => Some(Action::ToggleLayerShader {
            namespace: args[0].clone(),
            shader: ShaderRef::parse(&args[1]),
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// hl.window_rule tables
// ---------------------------------------------------------------------------

fn rule_from_table(
    body: &str,
    locals: &HashMap<String, String>,
    id: &mut impl FnMut(&str) -> String,
) -> Result<Option<WindowRule>, String> {
    let Some(tag_expr) = field(body, "tag") else {
        // A rule with no tag is not ours.
        return Ok(None);
    };
    let Some(tag_text) = eval(&tag_expr, locals) else {
        return Err(format!("window_rule: could not read tag `{}`", tag_expr.trim()));
    };
    if !tag_text.trim_start().starts_with('+') {
        return Ok(None);
    }

    let Some((key, value)) = tag_text.trim_start_matches('+').split_once(':') else {
        return Err(format!("window_rule: `{tag_text}` is not a tag"));
    };
    let Some((slot, is_default)) = TagSlot::parse_key(key) else {
        // A non-shader tag on a window rule; not this app's business.
        return Ok(None);
    };

    let value = match slot.info().kind {
        crate::model::TagKind::Flag => TagValue::Flag { on: value.trim() != "0" },
        crate::model::TagKind::Path => TagValue::Path(ShaderRef::parse(value.trim())),
    };

    let mut rule = WindowRule::new(id("rule"));
    if let Some(n) = field(body, "name").and_then(|e| eval(&e, locals)) {
        rule.name = n;
    }
    if let Some(m) = field(body, "match") {
        rule.match_ = match_from_table(&m, locals);
    }
    rule.tags.push(Tag { slot, is_default, value });
    Ok(Some(rule))
}

fn match_from_table(expr: &str, locals: &HashMap<String, String>) -> Match {
    let body = expr.trim();
    let body = body.strip_prefix('{').unwrap_or(body);
    let body = body.strip_suffix('}').unwrap_or(body);
    Match {
        class: field(body, "class").and_then(|e| eval(&e, locals)),
        title: field(body, "title").and_then(|e| eval(&e, locals)),
        initial_class: field(body, "initialClass").and_then(|e| eval(&e, locals)),
        initial_title: field(body, "initialTitle").and_then(|e| eval(&e, locals)),
    }
}

/// Pull `key = <expr>` out of a Lua table body, respecting nesting and strings.
fn field(body: &str, key: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?m)(^|[,{{\s]){}\s*=\s*", regex::escape(key))).ok()?;
    let m = re.find(body)?;
    let start = m.end();
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    let mut in_str: Option<u8> = None;

    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_str = Some(c),
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    Some(body[start..i].trim().to_string())
}

// ---------------------------------------------------------------------------
// Expression evaluation
// ---------------------------------------------------------------------------

/// Evaluate a Lua expression made of string literals and `local` variables
/// joined by `..`. Returns `None` for anything more adventurous.
fn eval(expr: &str, locals: &HashMap<String, String>) -> Option<String> {
    let mut out = String::new();
    for part in split_concat(expr) {
        let p = part.trim();
        if p.is_empty() {
            return None;
        }
        if let Some(s) = lua_string(p) {
            out.push_str(&s);
        } else if let Some(v) = locals.get(p) {
            out.push_str(v);
        } else {
            return None;
        }
    }
    Some(out)
}

/// Split on `..` that is not inside a string.
fn split_concat(expr: &str) -> Vec<String> {
    let bytes = expr.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut in_str: Option<u8> = None;

    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            in_str = Some(c);
            i += 1;
            continue;
        }
        if c == b'.' && i + 1 < bytes.len() && bytes[i + 1] == b'.' {
            parts.push(expr[start..i].to_string());
            i += 2;
            start = i;
            continue;
        }
        i += 1;
    }
    parts.push(expr[start..].to_string());
    parts
}

/// Unquote a Lua string literal.
fn lua_string(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    if bytes.len() < 2 || *bytes.last()? != quote {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => return None,
        }
    }
    Some(out)
}

fn collect_locals(source: &str) -> HashMap<String, String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(r#"(?m)^\s*local\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*("[^"\n]*"|'[^'\n]*')\s*$"#)
            .expect("locals regex")
    });
    re.captures_iter(source)
        .filter_map(|c| {
            let name = c.get(1)?.as_str().to_string();
            let value = lua_string(c.get(2)?.as_str())?;
            Some((name, value))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Finding calls
// ---------------------------------------------------------------------------

/// Find `name({ ... })` calls and return each table body with its byte offset.
fn find_calls(source: &str, name: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let rest = &source[from..];
        let Some(open_rel) = rest.find('{') else { break };
        // Only an opening paren and whitespace may sit between.
        if rest[..open_rel].chars().any(|c| !c.is_whitespace() && c != '(') {
            continue;
        }
        let open = from + open_rel;
        if let Some(close) = match_brace(source, open) {
            out.push((source[open + 1..close].to_string(), at));
            from = close;
        }
    }
    out
}

/// Given the index of `{`, find its matching `}`.
fn match_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_str = Some(c),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[derive(Debug)]
struct PluginCall {
    func: String,
    args: Vec<String>,
    bind_key: Option<String>,
}

const PLUGIN_FUNCS: &[&str] = &[
    "togglewindowshader",
    "toggleclassshader",
    "classshader",
    "togglelayershader",
    "layershader",
    "layeropenanim",
    "layercloseanim",
    "reloadshaders",
];

fn find_plugin_calls(source: &str) -> Vec<PluginCall> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(&format!(r"\.({})\s*\(", PLUGIN_FUNCS.join("|"))).expect("plugin call regex")
    });
    static BIND: OnceLock<Regex> = OnceLock::new();
    let bind_re = BIND.get_or_init(|| {
        Regex::new(r#"hl\.bind\s*\(\s*("[^"\n]*"|'[^'\n]*')"#).expect("bind regex")
    });

    let binds: Vec<(usize, String)> = bind_re
        .captures_iter(source)
        .filter_map(|c| {
            let m = c.get(0)?;
            let key = lua_string(c.get(1)?.as_str())?;
            Some((m.start(), key))
        })
        .collect();

    let mut out = Vec::new();
    for caps in re.captures_iter(source) {
        let whole = caps.get(0).expect("group 0");
        let func = caps[1].to_string();
        let open = whole.end() - 1;
        let Some(close) = match_paren(source, open) else { continue };
        let args = split_args(&source[open + 1..close]);

        // The nearest preceding hl.bind wins, provided no `end)` closes it
        // before the call.
        let bind_key = binds
            .iter()
            .rev()
            .find(|(at, _)| *at < whole.start())
            .filter(|(at, _)| !source[*at..whole.start()].contains("end)"))
            .map(|(_, k)| k.clone());

        out.push(PluginCall { func, args, bind_key });
    }
    out
}

fn match_paren(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_str = Some(c),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn split_args(inner: &str) -> Vec<String> {
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let bytes = inner.as_bytes();
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;

    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => in_str = Some(c),
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    args.push(inner[start..].trim().to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plain_rule() {
        let src = r#"
hl.window_rule({
    name  = "kitty-dim",
    match = { class = "kitty" },
    tag   = "+shader:/home/u/.config/hypr/shaders/dim.glsl",
})
"#;
        let got = scan(src);
        assert_eq!(got.rules.len(), 1);
        let r = &got.rules[0];
        assert_eq!(r.name, "kitty-dim");
        assert_eq!(r.match_.class.as_deref(), Some("kitty"));
        assert_eq!(r.tags[0].slot, TagSlot::Shader);
        match &r.tags[0].value {
            TagValue::Path(p) => assert!(p.path.ends_with("dim.glsl")),
            _ => panic!("expected a path"),
        }
    }

    #[test]
    fn resolves_a_local_used_in_the_path() {
        let src = r#"
local shaders = "/home/u/.config/hypr/shaders"
hl.window_rule({
    match = { class = "kitty" },
    tag   = "+shader_close:" .. shaders .. "/smoke.glsl@1.0",
})
"#;
        let got = scan(src);
        assert_eq!(got.rules.len(), 1);
        match &got.rules[0].tags[0].value {
            TagValue::Path(p) => {
                assert_eq!(p.path, "/home/u/.config/hypr/shaders/smoke.glsl");
                assert_eq!(p.duration, Some(1.0));
            }
            _ => panic!("expected a path"),
        }
        assert_eq!(got.rules[0].tags[0].slot, TagSlot::Close);
    }

    #[test]
    fn reads_the_default_suffix() {
        let src = r#"hl.window_rule({ match = { class = ".*" }, tag = "+shader_close_default:/p/x.glsl" })"#;
        let got = scan(src);
        assert!(got.rules[0].tags[0].is_default);
    }

    #[test]
    fn a_flag_tag_is_read_as_a_flag() {
        let src =
            r#"hl.window_rule({ match = { class = "mpv" }, tag = "+shader_fullscreen_stack:1" })"#;
        let got = scan(src);
        assert_eq!(got.rules[0].tags[0].value, TagValue::Flag { on: true });
    }

    #[test]
    fn unrelated_window_rules_are_ignored() {
        let src = r#"hl.window_rule({ match = { class = "kitty" }, opacity = 0.9 })"#;
        assert!(scan(src).is_empty());
    }

    #[test]
    fn an_unreadable_tag_is_skipped_not_guessed() {
        let src = r#"hl.window_rule({ match = { class = "k" }, tag = "+shader:" .. pick_one() })"#;
        let got = scan(src);
        assert!(got.rules.is_empty());
        assert_eq!(got.skipped.len(), 1);
    }

    #[test]
    fn layer_calls_become_layer_entries() {
        let src = r#"
hl.on("hyprland.start", function()
    hl.plugin.HyprWindowShade.layershader("rofi", "/p/blur.glsl")
    hl.plugin.HyprWindowShade.layeropenanim("rofi", "/p/open.glsl@0.2")
end)
"#;
        let got = scan(src);
        assert_eq!(got.layers.len(), 1);
        let l = &got.layers[0];
        assert_eq!(l.namespace, "rofi");
        assert_eq!(l.shader.as_ref().unwrap().path, "/p/blur.glsl");
        assert_eq!(l.open_anim.as_ref().unwrap().duration, Some(0.2));
    }

    #[test]
    fn a_bind_keeps_its_key() {
        let src = r#"
hl.bind("SUPER + W", function()
    hl.plugin.HyprWindowShade.togglewindowshader("/p/pixelate.glsl")
end)
"#;
        let got = scan(src);
        assert_eq!(got.binds.len(), 1);
        assert_eq!(got.binds[0].key, "SUPER + W");
        assert!(matches!(got.binds[0].action, Action::ToggleWindowShader { .. }));
    }

    #[test]
    fn a_call_after_a_closed_bind_is_not_attributed_to_it() {
        let src = r#"
hl.bind("SUPER + W", function()
    hl.plugin.HyprWindowShade.reloadshaders()
end)
hl.plugin.HyprWindowShade.classshader("kitty", "/p/x.glsl")
"#;
        let got = scan(src);
        assert_eq!(got.binds.len(), 1);
        assert_eq!(got.startup.len(), 1);
        assert!(matches!(got.startup[0].action, Action::ClassShader { .. }));
    }

    #[test]
    fn clear_removes_rather_than_imports() {
        let src = r#"hl.plugin.HyprWindowShade.layershader("mpvpaper", "clear")"#;
        assert!(scan(src).is_empty());
    }

    #[test]
    fn the_managed_block_is_not_re_imported() {
        let inner = r#"hl.window_rule({ match = { class = "kitty" }, tag = "+shader:/p/a.glsl" })"#;
        let doc = format!(
            "hl.window_rule({{ match = {{ class = \"mpv\" }}, tag = \"+shader:/p/b.glsl\" }})\n\
             {}\n{inner}\n{}\n",
            block::BEGIN,
            block::END
        );
        let got = from_document(&doc);
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].match_.class.as_deref(), Some("mpv"));
    }

    #[test]
    fn concat_splitting_ignores_dots_inside_strings() {
        let parts = split_concat(r#""a..b" .. x"#);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), r#""a..b""#);
    }

    #[test]
    fn summary_reads_naturally() {
        let mut i = Imported::default();
        i.rules.push(WindowRule::new("r1"));
        assert_eq!(i.summary(), "1 window rule");
        i.rules.push(WindowRule::new("r2"));
        assert_eq!(i.summary(), "2 window rules");
    }
}
