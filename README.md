# hyprwindowshade-gui

A Qt Quick front end for [HyprWindowShade](https://github.com/ManofJELLO/HyprWindowShade),
the Hyprland plugin that applies GLSL fragment shaders to individual windows and layers.

It does two things:

* **Writes the plugin's section of `hyprland.lua` for you** — window rules and their shader
  tags, layer shaders, open and close animations, keybinds and startup calls — into a block
  between two marker comments. The rest of your config is never touched.
* **Edits the shaders themselves** — the `const` values inside a `.glsl`, its `// @duration`
  and its `// @overlay`. The plugin reloads a shader when its mtime changes, so a slider here
  is a live change on screen.

Gruvbox dark and light are built in; any other palette is a small TOML file.

---

## Contents

- [What it looks like](#what-it-looks-like)
- [Requirements](#requirements) · [Build and install](#build-and-install)
- [How it writes your config](#how-it-writes-your-config)
- [Shader parameters](#shader-parameters)
- [Themes](#themes)
- [Importing rules you already wrote](#importing-rules-you-already-wrote)
- [Settings](#settings)
- [Command line](#command-line)
- [How it is put together](#how-it-is-put-together)
- [Things worth knowing](#things-worth-knowing)

---

## What it looks like

![Rules](docs/rules.png)

Six pages down the left:

| Page | What it does |
|---|---|
| **Rules** | Window rules. Match by class or title, then fill any of the plugin's twenty shader tags. |
| **Shaders** | Everything in your shader folder: what each one declares, and sliders for its tunable constants. |
| **Layers** | `layershader`, `layeropenanim`, `layercloseanim` by namespace, with live namespaces offered. |
| **Keybinds** | Toggle binds and session-start calls. |
| **Preview** | Exactly the Lua that will be written, and the importer. |
| **Settings** | Paths, how the block is written, theme, backups. |

Nothing is written until you press **Save**.

![Shaders](docs/shaders.png)

*The Shaders page: what a shader declares, its timing, and sliders for the constants inside it.*

---

## Requirements

* **Rust 1.82+** and a C++ compiler — the UI is Qt Quick, reached through
  [CXX-Qt](https://github.com/KDAB/cxx-qt).
* **Qt 6.4 or newer**, with Qt Quick and Qt Quick Controls. On Arch that is
  `qt6-base` and `qt6-declarative`.
* **HyprWindowShade** itself, and a Lua Hyprland config (`~/.config/hypr/hyprland.lua`).
  `hyprland.conf` is not supported here — the plugin's own README explains why `.conf` is on
  its way out.

Hyprland does not have to be running. Without it the app still edits your config and your
shaders; it just cannot offer you the list of open window classes or live layer namespaces,
and the "Try it" buttons are disabled.

## Build and install

```sh
git clone https://github.com/ManofJELLO/hyprwindowshade-gui
cd hyprwindowshade-gui
cargo build --release
```

If `cargo` cannot find Qt, point it at qmake:

```sh
QMAKE=/usr/bin/qmake6 cargo build --release
```

Then either run `./target/release/hyprwindowshade-gui`, or install it with the desktop entry:

```sh
sudo make install          # /usr/local by default
make install PREFIX=~/.local
```

An Arch `PKGBUILD` is in `packaging/`.

---

## How it writes your config

Everything the app generates lives between two marker lines:

```lua
-- >>> HyprWindowShade (managed by hyprwindowshade-gui) >>>
...
-- <<< HyprWindowShade (managed by hyprwindowshade-gui) <<<
```

Anything above or below is preserved byte for byte. Before every write the file is copied to
`~/.config/hyprwindowshade-gui/backups/` with a timestamp, and the write itself is atomic —
a crash mid-save cannot leave you with half a config, which on Hyprland means a session that
will not start.

One line inside the block is a comment carrying the whole configuration as JSON. That is what
the app reads back on the next run, so it never has to parse Lua to know what it wrote. If you
delete the markers, the app forgets everything and leaves your hand-written Lua alone.

**Removing it.** *Preview → Remove managed block* deletes the block and nothing else.

### What the generated Lua looks like

```lua
local hws_shaders = "/home/you/.config/hypr/shaders"

hl.window_rule({
    name  = "chrome-reading-base",
    match = { class = "google-chrome" },
    tag   = "+shader:" .. hws_shaders .. "/reading_mode.glsl",
})

hl.on("hyprland.start", function()
    local hws = hl.plugin.HyprWindowShade
    if not hws then
        hl.notification.create({ text = "[HyprWindowShade] plugin not loaded", … })
        return
    end
    hws.layershader("rofi", hws_shaders .. "/blur.glsl")
end)

hl.bind("SUPER + W", function()
    local hws = hl.plugin.HyprWindowShade
    if not hws then … end
    hws.togglewindowshader(hws_shaders .. "/pixelate.glsl")
end)
```

Three deliberate choices in there:

* **One `hl.window_rule` per tag.** Hyprland's Lua rule takes one `tag`, and the plugin stacks
  tags that match the same window. A rule with four tags becomes four calls sharing a match.
* **The plugin table is looked up inside the closure.** It loads at session start, long after
  the config is parsed, so looking it up at parse time would give you `nil`. The generated
  binds also *say so* with a notification rather than failing silently.
* **Paths go through a local.** One place to change if you move your shader folder. Turn it
  off in Settings if you would rather see full paths.

### Loading the plugin

Settings can add the loader to the same start handler — `hyprpm reload -n`, or
`hyprctl plugin load …`. Both run at session start rather than at parse time, which is what
keeps a corrupt `.so` from taking your desktop with it. The plugin's README has the full
explanation; it is worth reading once.

### If your plugin loads slowly

Startup calls (layer shaders, class shaders) run inside the `hyprland.start` handler, right
after the loader. If the plugin has not finished loading by then they are skipped and you get
a notification. Settings has a **startup delay**: set it above zero and those calls are
re-issued instead through a shell-delayed `hyprctl dispatch`, which runs once the plugin is
certainly up. Leave it at zero unless you actually see the problem.

---

## Shader parameters

The plugin populates a fixed set of uniforms and offers no channel for custom ones, so a
shader's tunable numbers are `const` declarations in its source. Editing one here rewrites
that line in the file — indentation, trailing comments and line endings intact — and the
plugin picks it up on the next frame.

The app finds two kinds:

**Annotated**, which is what you want for anything you will come back to:

```glsl
// @param 0.0 1.0 0.01 "Dim amount"
const float DIM = 0.6;

// @param STRENGTH 0 4 "Warmth"          // the named form, for a const further down
const float STRENGTH = 1.5;

// @color "Paper tint"
const vec3 PAPER = vec3(0.98, 0.94, 0.86);

// @bool "Invert"
const bool INVERT = false;
```

`// @param [name] <min> <max> [step] ["Label"]`. Without a name it attaches to the next `const`.

**Inferred**, for shaders you did not write: any `const float/int/bool/vec2/vec3/vec4` with a
plain literal gets a slider with a guessed range, marked *guessed range* in the UI. A `vec3`
whose name contains `color`, `tint` or `rgb` and whose components are all in 0–1 becomes a
colour picker.

A `const` whose value is an expression — `const float A = 1.0 / 3.0;` — is left alone, on
purpose: the app will not rewrite something it cannot read back.

Two file-level directives are editable too:

* **`// @duration`** — how long a one-shot animation runs. The plugin caps it at 5 seconds and
  falls back to 0.3s when it is absent, warning once per edit if the shader uses `progress`.
  The app shows the same warning before you hit it.
* **`// @overlay`** — composite with Hyprland's own close animation instead of replacing it.
  Right for a tint or a wipe, wrong for a dissolve that drives its own alpha.

Shader files are backed up before every edit, same as the config.

---

## Themes

Gruvbox Dark and Gruvbox Light ship with the app. Anything else is a TOML file in
`~/.config/hyprwindowshade-gui/themes/`; the file stem is the theme's name in the picker.
*Settings → Write an example theme* drops a fully commented one there.

```toml
name = "Example"
base = "gruvbox-dark"     # what to start from; anything you omit comes from here
dark = true
opacity = 0.96            # 0.3 – 1.0; Hyprland composites the rest

[colors]
bg         = "#282828"    # window background
bg_alt     = "#1d2021"    # sidebar and header
surface    = "#32302f"    # cards, rows, inputs
surface_hi = "#3c3836"    # hover
border     = "#504945"
fg         = "#ebdbb2"
fg_dim     = "#d5c4a1"
muted      = "#928374"
accent     = "#83a598"    # focus rings, sliders, active tab
accent_alt = "#d3869b"
ok         = "#b8bb26"
warn       = "#fabd2f"
error      = "#fb4934"
selection  = "#3c3836"
on_accent  = "#1d2021"    # text drawn on the accent colour
```

Every key is optional. `#rgb`, `#rrggbb` and `#aarrggbb` all work. A file with a broken colour
is reported by name and role rather than silently ignored, and the other themes still load.

---

## Importing rules you already wrote

*Preview → Look for rules* reads the parts of your config **outside** the managed block and
shows what it recognises: `hl.window_rule` calls carrying a shader tag, `layershader` and the
layer animations, and plugin calls inside `hl.bind`.

It understands string literals and concatenations with `local`s defined in the same file —
the `local shaders = "/home/you/…"` pattern from the plugin's README. Anything it cannot read
with certainty is listed as skipped rather than guessed at.

Importing adds those entries to the app; **it does not delete your originals**. Look them over,
save, check the result, then remove the hand-written lines yourself. Until you do, both copies
are in the file and the tags will fight — see the plugin's notes on `_default` for what that
looks like.

---

## Settings

| Setting | Notes |
|---|---|
| Hyprland config | Which file the block goes in. Default `~/.config/hypr/hyprland.lua`. |
| Shader folder | Scanned for `.glsl`, `.frag`, `.fs`, one level of subfolders deep. Default `~/.config/hypr/shaders`. |
| Backups to keep | Per file. Default 10. |
| Shader paths | Whether to use the `hws_shaders` local or write full paths. |
| Load the plugin | Nothing, `hyprpm reload -n`, or `hyprctl plugin load <path>`. |
| Startup delay | See above. Zero unless you need it. |
| Reload Hyprland after saving | Runs `hyprctl reload` so new rules apply without logging out. |
| Reload shaders after editing one | Off by default — the plugin already reloads on mtime change. |

App settings live in `~/.config/hyprwindowshade-gui/settings.toml`. A malformed file is
reported and ignored, never silently overwritten.

---

## Command line

```sh
hyprwindowshade-gui                 # the app
hyprwindowshade-gui --print-state   # the whole state document as JSON
hyprwindowshade-gui --print-block   # the Lua that would be written
```

The two `--print-*` flags do not start Qt, which makes them useful on a machine where the GUI
will not come up, and in a script.

`HWS_SHOT_DIR=<dir>` renders each page to a PNG there and quits — how the screenshots in this
README were made, and how the layout is checked without a compositor.

---

## How it is put together

Two crates:

* **`hws-core`** — everything that decides anything. The config model, the Lua emitter, the
  managed-block reader and writer, the GLSL parser and patcher, the importer, themes, and the
  `hyprctl` calls. No Qt, no UI, and covered by unit tests: `cargo test -p hws-core`.
* **`hws-gui`** — the Qt layer. One `Backend` singleton with a JSON state property, a `run`
  method and a `query` method. The QML reads state and sends commands; it holds no
  configuration logic of its own.

That split is the reason the interesting parts can be tested without a display or a
compositor, and the reason a UI bug cannot corrupt a config.

```
crates/hws-core/src/
    model.rs      the configuration, and the twenty tag slots
    emit.rs       model  -> Lua
    block.rs      the marker block, and the state blob inside it
    import.rs     hand-written Lua -> model, conservatively
    shader/       scan, parse (@param, @duration, @overlay, uniforms), patch
    theme.rs      Gruvbox, and the TOML loader
    hyprctl.rs    clients, layers, dispatch
    session.rs    the facade: one state document, one command entry point
crates/hws-gui/
    src/bridge.rs the CXX-Qt bridge, and nothing else
    qml/          Theme and App singletons, components, six pages
```

---

## Things worth knowing

**Motion uniforms are zero on a layer.** A shader that scales its effect by `velocity` or
`peak_velocity` renders a layer surface untouched — no error, no warning, nothing happens. The
Layers page flags this when you pick such a shader, because it is the single most confusing
thing about layer shaders.

**Fullscreen drops everything by default.** A fullscreen window gets no shaders at all unless
you set *While fullscreen* or *Keep stack fullscreen*. That is the plugin's behaviour, not a
limitation here.

**`_default` is documented for eight tags.** The plugin's tag table says any shader tag accepts
the `_default` fallback suffix; its fallback section names eight specifically. The app lets you
set it anywhere but warns on the ones outside that list, so a surprise is a warning rather than
a mystery.

**A duration override does nothing on move and resize.** Hyprland's own `windowsMove` animation
is the clock there. The app hides the duration field for those tags.

**`hyprctl dispatch` lies about failing.** The plugin's functions return nothing, so
`hl.dispatch` rejects the call with a non-zero exit *after* it has already worked. The app
ignores that specific complaint and surfaces anything else.

---

## Licence

MIT. See [LICENSE](LICENSE).
