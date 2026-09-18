pragma Singleton

import QtQuick

// This file is part of the dev.hyprwindowshade.gui module, so the module's own
// types (Backend) are available without importing it. Importing it here would
// make the two singletons depend on each other through the module.

// Colours and metrics for the whole app, derived from the theme the engine
// resolved (a built-in Gruvbox, or a TOML file in the theme directory).
QtObject {
    id: theme

    // Main.qml keeps this in step with the backend. The singleton does not reach
    // for Backend itself: a singleton that imports its own module makes the
    // module's singletons depend on each other, which QML refuses to load.
    property string rawState: "{}"

    readonly property var _state: {
        try {
            return JSON.parse(theme.rawState)
        } catch (e) {
            return ({})
        }
    }
    readonly property var _theme: _state.theme || ({})
    readonly property var _c: _theme.colors || ({})

    readonly property string name: _theme.name || "Gruvbox Dark"
    readonly property bool dark: _theme.dark === undefined ? true : _theme.dark
    readonly property real windowOpacity: _theme.opacity === undefined ? 1.0 : _theme.opacity

    readonly property color bg:        _c.bg         || "#282828"
    readonly property color bgAlt:     _c.bg_alt     || "#1d2021"
    readonly property color surface:   _c.surface    || "#32302f"
    readonly property color surfaceHi: _c.surface_hi || "#3c3836"
    readonly property color border:    _c.border     || "#504945"
    readonly property color fg:        _c.fg         || "#ebdbb2"
    readonly property color fgDim:     _c.fg_dim     || "#d5c4a1"
    readonly property color muted:     _c.muted      || "#928374"
    readonly property color accent:    _c.accent     || "#83a598"
    readonly property color accentAlt: _c.accent_alt || "#d3869b"
    readonly property color ok:        _c.ok         || "#b8bb26"
    readonly property color warn:      _c.warn       || "#fabd2f"
    readonly property color error:     _c.error      || "#fb4934"
    readonly property color selection: _c.selection  || "#3c3836"
    readonly property color onAccent:  _c.on_accent  || "#1d2021"

    // Metrics. One place, so spacing stays consistent across pages.
    readonly property int pad: 16
    readonly property int gap: 10
    readonly property int gapSmall: 6
    readonly property int radius: 8
    readonly property int radiusSmall: 5
    readonly property int rowHeight: 34
    readonly property int sidebarWidth: 188
    readonly property int listWidth: 300

    readonly property int fontSize: 13
    readonly property int fontSizeSmall: 11
    readonly property int fontSizeLarge: 16
    readonly property int fontSizeTitle: 19

    readonly property string monoFamily: "monospace"

    // A translucent wash of a colour, for badges and hover states.
    function wash(c, amount) {
        return Qt.rgba(c.r, c.g, c.b, amount === undefined ? 0.18 : amount)
    }
}
