---
name: run-the-gui
description: Launch hyprwindowshade-gui and see a change working — render every page to a PNG, or read Qt's own QML warnings — without a compositor and without taking over the screen. Use when asked to run, start, or screenshot the app, or to confirm a QML change works in the real app.
---

# Running hyprwindowshade-gui

The app is a Qt Quick front end. It does **not** need a compositor to run, and
it should not borrow the user's screen: they work on this machine while Claude
runs, so every check below is offscreen. See the user memory
`warn-before-taking-screen-control` — ask first if you truly need a window.

The binary is `target/debug/hyprwindowshade-gui` (not `hws-gui` — that is the
crate name, and guessing it wastes a run).

## Seeing a page

`make shots` renders every page to `shots/<n>-<label>.png` and quits. It is a
development path built into `Main.qml` (`HWS_SHOT_DIR`), so it needs no input
and no display:

```bash
make shots && ls shots/
```

Then **look at the PNG** with the Read tool. A page that renders is the proof;
a build that compiles is not. The pages are `0-rules`, `1-shaders`, `2-layers`,
`3-keybinds`, `4-preview`, `5-plugin`, `6-settings` — so a change to
`PluginPage.qml` is `shots/5-plugin.png`.

To put the shots somewhere that is not the repo, run the binary directly with
`HWS_SHOT_DIR` set to a scratchpad path.

## Reading Qt's QML warnings

Warnings about the QML itself — shadowed properties, broken bindings, missing
types — come from Qt's own logging categories, which are **off by default and
do not reach the terminal** on this machine:

```bash
timeout 20 env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
  QT_LOGGING_RULES='qt.qml.*=true' \
  ./target/debug/hyprwindowshade-gui > /path/to/scratchpad/qml.log 2>&1
grep -n 'propertyCache\|Unable to assign\|is not a type' /path/to/scratchpad/qml.log
```

Two traps, both of which look exactly like "the warning is fixed":

- **`QT_FORCE_STDERR_LOGGING=1` is required.** Without it the app prints
  nothing at all. (For a bare `qml6` harness the variable is
  `QT_ASSUME_STDERR_HAS_CONSOLE=1` instead.)
- **Redirect to a file; do not pipe.** Under `timeout`, `| grep` and `| head`
  came back empty while the same run redirected to a file held 4,000 lines.

`qt.qml.*=true` is verbose (~5k lines, mostly `qt.qml.import`). Grep it; do not
read it.

This is the runtime half. The static half is `make check`, which runs `qmllint`
over every QML file — reach for that first, since it needs no run and catches
what a render cannot show. The two do not overlap: `qmllint` never instantiates
anything, so shadowed properties still only show up in the log above.

## Proving a warning is gone

An absent warning is only evidence if the check can still produce one. Put the
old code back, rebuild, confirm the warning returns, then restore:

```bash
# reintroduce the old form, rebuild (~22s), run as above, grep -> warning present
git checkout crates/hws-gui/qml/pages/ThatPage.qml
cargo build -p hws-gui   # rebuild so the binary matches the committed source
```

QML lives in the binary via `build.rs`, so **every QML edit needs a rebuild**
before it can show up in a run. Editing the copy under
`target/cxxqt/qml_modules/` changes nothing — it is generated.

## What this app does not need

No Xvfb, no Playwright, no `grim`, no clicking. Pointer-driven testing does not
work here anyway: `hyprctl dispatch movecursor` silently does nothing on this
machine, and there is no `wtype` or `ydotool`. To capture a state that normally
needs input, force it from QML behind a flag, render it with `make shots`, and
revert.
