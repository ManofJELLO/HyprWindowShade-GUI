import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprwindowshade.gui

// The shader folder: what is in it, and the numbers inside each file.
//
// The plugin has no channel for custom uniforms, so a tunable value is a `const`
// in the source. Changing one here rewrites that line in the file — and because
// the plugin reloads a shader when its mtime changes, the change is on screen
// immediately.
Item {
    id: page

    property string selectedPath: ""
    property string filter: ""
    property bool showSource: false

    readonly property var selected: App.shaderByPath(page.selectedPath)

    readonly property var visibleShaders: {
        var needle = page.filter.toLowerCase()
        if (needle === "")
            return App.shaders
        var out = []
        for (var i = 0; i < App.shaders.length; ++i) {
            var s = App.shaders[i]
            if (s.name.toLowerCase().indexOf(needle) >= 0
                    || s.path.toLowerCase().indexOf(needle) >= 0
                    || (s.description || "").toLowerCase().indexOf(needle) >= 0)
                out.push(s)
        }
        return out
    }

    function ensureSelection() {
        if (App.shaders.length === 0) {
            page.selectedPath = ""
            return
        }
        if (App.shaderByPath(page.selectedPath) === null)
            page.selectedPath = App.shaders[0].path
    }

    Connections {
        target: App
        function onShadersChanged() {
            page.ensureSelection()
        }
    }

    Component.onCompleted: page.ensureSelection()

    RowLayout {
        anchors.fill: parent
        spacing: Theme.pad

        // ------------------------------------------------------------------
        // The list
        // ------------------------------------------------------------------
        ColumnLayout {
            // Pinned rather than merely preferred: a RowLayout hands leftover
            // space to whichever item will take it, and a list of rules does not
            // get better by being 900px wide.
            Layout.preferredWidth: Theme.listWidth
            Layout.minimumWidth: Theme.listWidth
            Layout.maximumWidth: Theme.listWidth
            Layout.fillHeight: true
            spacing: Theme.gap

            LineEdit {
                Layout.fillWidth: true
                placeholderText: "Search shaders…"
                onTextChanged: page.filter = text
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: Theme.surface
                radius: Theme.radius
                border.width: 1
                border.color: Theme.border

                EmptyState {
                    anchors.centerIn: parent
                    width: parent.width - Theme.pad * 2
                    visible: page.visibleShaders.length === 0
                    title: App.shaders.length === 0 ? "No shaders found" : "Nothing matches"
                    body: App.shaders.length === 0
                          ? "Put .glsl files in " + App.shaderDirDisplay
                            + ", or point the shader folder somewhere else in Settings."
                          : "Try a different search."
                }

                ListView {
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    model: page.visibleShaders

                    delegate: Rectangle {
                        id: shaderRow

                        required property var modelData

                        width: ListView.view.width
                        height: 58
                        color: shaderRow.modelData.path === page.selectedPath
                               ? Theme.selection
                               : (shaderHover.hovered ? Theme.surfaceHi : "transparent")

                        HoverHandler {
                            id: shaderHover
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: page.selectedPath = shaderRow.modelData.path
                        }

                        Rectangle {
                            width: 3
                            height: parent.height
                            color: Theme.accent
                            visible: shaderRow.modelData.path === page.selectedPath
                        }

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: Theme.gap
                            anchors.leftMargin: Theme.gap + 4
                            spacing: 2

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.gapSmall

                                Text {
                                    Layout.fillWidth: true
                                    text: shaderRow.modelData.name
                                    color: Theme.fg
                                    font.pixelSize: Theme.fontSize
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }

                                Badge {
                                    visible: shaderRow.modelData.inUse
                                    text: "in use"
                                    tint: Theme.ok
                                }
                                Badge {
                                    visible: shaderRow.modelData.notes.length > 0
                                    text: "!"
                                    tint: Theme.warn
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.gapSmall

                                Text {
                                    Layout.fillWidth: true
                                    text: App.fileName(shaderRow.modelData.path)
                                    color: Theme.muted
                                    font.pixelSize: Theme.fontSizeSmall
                                    font.family: Theme.monoFamily
                                    elide: Text.ElideMiddle
                                }

                                Text {
                                    visible: shaderRow.modelData.params.length > 0
                                    text: shaderRow.modelData.params.length + " ●"
                                    color: Theme.accent
                                    font.pixelSize: Theme.fontSizeSmall
                                }
                            }
                        }
                    }

                    ScrollBar.vertical: ScrollBar {}
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.gapSmall

                PillButton {
                    text: "Rescan"
                    Layout.fillWidth: true
                    onClicked: App.run("shader.rescan", {})
                }
                PillButton {
                    text: "Reload in plugin"
                    Layout.fillWidth: true
                    enabled: App.hyprlandRunning
                    tooltip: App.hyprlandRunning
                             ? "Drop the plugin's compiled shader cache"
                             : "Hyprland is not running"
                    onClicked: App.run("shader.reload", {})
                }
            }
        }

        // ------------------------------------------------------------------
        // The detail
        // ------------------------------------------------------------------
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            EmptyState {
                anchors.centerIn: parent
                width: Math.min(420, parent.width - Theme.pad * 2)
                visible: page.selected === null
                title: "No shader selected"
                body: "Pick one on the left to see what it does and what you can tune."
            }

            ScrollView {
                id: detailScroll
                anchors.fill: parent
                visible: page.selected !== null
                clip: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: detailScroll.availableWidth
                    spacing: Theme.pad

                    // --- header ---
                    Card {
                        Layout.fillWidth: true
                        title: page.selected ? page.selected.name : ""
                        subtitle: page.selected ? page.selected.display : ""

                        Text {
                            width: parent.width
                            visible: page.selected && page.selected.description
                            text: page.selected && page.selected.description
                                  ? page.selected.description : ""
                            color: Theme.fgDim
                            font.pixelSize: Theme.fontSize
                            wrapMode: Text.WordWrap
                        }

                        Flow {
                            width: parent.width
                            spacing: Theme.gapSmall

                            Badge {
                                visible: page.selected && page.selected.isAnimation
                                text: "one-shot animation"
                                tint: Theme.accentAlt
                            }
                            Badge {
                                visible: page.selected && page.selected.isMotionDriven
                                text: "motion driven"
                                tint: Theme.accent
                            }
                            Badge {
                                visible: page.selected && page.selected.inUse
                                text: "used by a rule"
                                tint: Theme.ok
                            }
                            Badge {
                                visible: page.selected
                                         && page.selected.uniforms.indexOf("time") >= 0
                                text: "redraws continuously"
                                tint: Theme.warn
                            }
                        }
                    }

                    // --- notes ---
                    Card {
                        Layout.fillWidth: true
                        visible: page.selected && page.selected.notes.length > 0
                        title: "Worth knowing"

                        Repeater {
                            model: page.selected ? page.selected.notes : []

                            Text {
                                required property string modelData
                                width: parent.width
                                text: "•  " + modelData
                                color: Theme.warn
                                font.pixelSize: Theme.fontSize
                                wrapMode: Text.WordWrap
                            }
                        }
                    }

                    // --- animation timing ---
                    Card {
                        Layout.fillWidth: true
                        visible: page.selected !== null
                        title: "Timing"
                        subtitle: page.selected && page.selected.isAnimation
                                  ? "This shader drives itself from progress, so how long it runs " +
                                    "is part of the effect. A rule can still override it."
                                  : "This shader does not use progress, so timing only applies if " +
                                    "you use it as an open or close animation."

                        FieldRow {
                            width: parent.width
                            label: "Declares a duration"
                            hint: "// @duration in the file"

                            Toggle {
                                checked: page.selected && page.selected.duration !== null
                                         && page.selected.duration !== undefined
                                onToggled: {
                                    App.run("shader.setDuration", {
                                        path: page.selectedPath,
                                        seconds: checked ? 0.4 : null
                                    })
                                }
                            }
                        }

                        LabeledSlider {
                            width: parent.width
                            visible: page.selected && page.selected.duration !== null
                                     && page.selected.duration !== undefined
                            label: "Duration (seconds) — the plugin caps this at 5"
                            from: 0.05
                            to: 5.0
                            stepSize: 0.05
                            value: page.selected && page.selected.duration ? page.selected.duration : 0.4
                            onSettled: function (v) {
                                App.run("shader.setDuration", { path: page.selectedPath, seconds: v })
                            }
                        }

                        FieldRow {
                            width: parent.width
                            label: "Composite on close"
                            hint: "// @overlay — keep Hyprland's own fade underneath instead of " +
                                  "replacing it. Right for a tint or wipe; wrong for a dissolve that " +
                                  "drives its own alpha."

                            Toggle {
                                checked: page.selected ? page.selected.overlay : false
                                onToggled: {
                                    App.run("shader.setOverlay", {
                                        path: page.selectedPath,
                                        overlay: checked
                                    })
                                }
                            }
                        }
                    }

                    // --- parameters ---
                    Card {
                        Layout.fillWidth: true
                        visible: page.selected !== null
                        title: "Parameters"
                        subtitle: page.selected && page.selected.params.length > 0
                                  ? "These are const declarations in the file. Changing one rewrites " +
                                    "that line; the plugin picks it up on the next frame."
                                  : ""

                        Text {
                            width: parent.width
                            visible: page.selected && page.selected.params.length === 0
                            wrapMode: Text.WordWrap
                            color: Theme.muted
                            font.pixelSize: Theme.fontSize
                            text: "Nothing tunable found. Add a const with a plain number — " +
                                  "const float DIM = 0.6; — and it shows up here. A " +
                                  "// @param 0 1 0.01 \"Dim amount\" comment above it gives it a " +
                                  "proper range and label."
                        }

                        Repeater {
                            model: page.selected ? page.selected.params : []

                            ParamEditor {
                                required property var modelData
                                width: parent.width
                                param: modelData
                                shaderPath: page.selectedPath
                            }
                        }
                    }

                    // --- uniforms ---
                    Card {
                        Layout.fillWidth: true
                        visible: page.selected && page.selected.uniforms.length > 0
                        title: "Uniforms it declares"
                        subtitle: "The plugin fills these in every frame."

                        Flow {
                            width: parent.width
                            spacing: Theme.gapSmall

                            Repeater {
                                model: page.selected ? page.selected.uniforms : []

                                Badge {
                                    required property string modelData
                                    text: modelData
                                    tint: Theme.accent
                                }
                            }
                        }

                        Text {
                            width: parent.width
                            visible: page.selected && page.selected.isMotionDriven
                            wrapMode: Text.WordWrap
                            color: Theme.warn
                            font.pixelSize: Theme.fontSizeSmall
                            text: "Motion uniforms read zero on a layer surface, so pointing this " +
                                  "shader at a layer namespace does nothing at all — silently. " +
                                  "Drive it from progress in a layer animation instead."
                        }
                    }

                    // --- source ---
                    Card {
                        Layout.fillWidth: true
                        visible: page.selected !== null
                        title: "Source"

                        PillButton {
                            text: page.showSource ? "Hide source" : "Show source"
                            onClicked: page.showSource = !page.showSource
                        }

                        Rectangle {
                            width: parent.width
                            height: Math.min(420, sourceText.implicitHeight + Theme.gap * 2)
                            visible: page.showSource
                            color: Theme.bgAlt
                            radius: Theme.radiusSmall
                            border.width: 1
                            border.color: Theme.border

                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: Theme.gap
                                clip: true

                                Text {
                                    id: sourceText
                                    text: page.showSource && page.selectedPath !== ""
                                          ? App.query("shaderSource", { path: page.selectedPath })
                                          : ""
                                    color: Theme.fgDim
                                    font.family: Theme.monoFamily
                                    font.pixelSize: Theme.fontSizeSmall
                                    textFormat: Text.PlainText
                                }
                            }
                        }
                    }

                    Item {
                        Layout.preferredHeight: Theme.pad
                    }
                }
            }
        }
    }
}
