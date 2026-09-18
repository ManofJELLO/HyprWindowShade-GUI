import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprwindowshade.gui

// One tunable constant from a shader: a slider, a switch, a colour, or a small
// group of sliders, depending on what the type and the annotations say.
Item {
    id: editor

    property var param: null
    property string shaderPath: ""

    readonly property var kind: param && param.kind ? param.kind : ({})
    readonly property string kindName: kind.kind || "scalar"
    readonly property bool annotated: param ? param.annotated === true : false

    implicitHeight: column.implicitHeight

    function send(value) {
        if (!editor.param || editor.shaderPath === "")
            return
        App.run("shader.setParam", {
            path: editor.shaderPath,
            name: editor.param.name,
            value: value
        })
    }

    function components() {
        if (!editor.param)
            return []
        var v = editor.param.value
        return Array.isArray(v) ? v.slice() : [v]
    }

    function withComponent(index, value) {
        var out = editor.components()
        out[index] = value
        return out
    }

    Column {
        id: column
        width: parent.width
        spacing: 4

        // Header: name, GLSL type, and whether the range was declared or guessed.
        RowLayout {
            width: parent.width
            spacing: Theme.gapSmall

            Text {
                text: editor.param ? editor.param.label : ""
                color: Theme.fg
                font.pixelSize: Theme.fontSize
                font.weight: Font.DemiBold
            }

            Text {
                text: editor.param ? editor.param.name : ""
                color: Theme.muted
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSizeSmall
            }

            Item {
                Layout.fillWidth: true
            }

            Badge {
                text: editor.param ? editor.param.glsl_type : ""
                tint: Theme.muted
            }

            Badge {
                visible: !editor.annotated && editor.kindName !== "bool"
                text: "guessed range"
                tint: Theme.warn

                HoverHandler {
                    id: guessHover
                }
                ToolTip.visible: guessHover.hovered
                ToolTip.text: "No // @param annotation, so the range was inferred from the " +
                              "current value. Add one to set a real range and label."
                ToolTip.delay: 400
            }
        }

        // --- bool ---
        Toggle {
            visible: editor.kindName === "bool"
            checked: editor.param ? editor.param.value !== 0 : false
            onToggled: editor.send(checked)
        }

        // --- scalar ---
        LabeledSlider {
            width: parent.width
            visible: editor.kindName === "scalar"
            label: ""
            from: editor.kind.min !== undefined ? editor.kind.min : 0
            to: editor.kind.max !== undefined ? editor.kind.max : 1
            stepSize: editor.kind.step !== undefined ? editor.kind.step : 0.01
            integer: editor.kind.integer === true
            value: editor.param && !Array.isArray(editor.param.value) ? editor.param.value : 0
            onSettled: function (v) {
                editor.send(editor.kind.integer === true ? Math.round(v) : v)
            }
        }

        // --- colour ---
        Row {
            width: parent.width
            visible: editor.kindName === "color"
            spacing: Theme.gap

            Rectangle {
                id: swatch
                width: 56
                height: 56
                radius: Theme.radiusSmall
                border.width: 1
                border.color: Theme.border
                color: {
                    var c = editor.components()
                    return Qt.rgba(c[0] || 0, c[1] || 0, c[2] || 0,
                                   editor.kind.alpha === true ? (c[3] === undefined ? 1 : c[3]) : 1)
                }
            }

            Column {
                width: parent.width - swatch.width - Theme.gap
                spacing: 2

                Repeater {
                    model: editor.kind.alpha === true
                           ? ["red", "green", "blue", "alpha"]
                           : ["red", "green", "blue"]

                    LabeledSlider {
                        required property string modelData
                        required property int index

                        width: parent.width
                        label: modelData
                        from: 0
                        to: 1
                        stepSize: 0.01
                        value: {
                            var c = editor.components()
                            return c[index] === undefined ? 0 : c[index]
                        }
                        onSettled: function (v) {
                            editor.send(editor.withComponent(index, v))
                        }
                    }
                }
            }
        }

        // --- vector ---
        Column {
            width: parent.width
            visible: editor.kindName === "vector"
            spacing: 2

            Repeater {
                model: editor.kind.len !== undefined ? editor.kind.len : 0

                LabeledSlider {
                    required property int index

                    width: parent.width
                    label: ["x", "y", "z", "w"][index] || ("[" + index + "]")
                    from: editor.kind.min !== undefined ? editor.kind.min : 0
                    to: editor.kind.max !== undefined ? editor.kind.max : 1
                    stepSize: editor.kind.step !== undefined ? editor.kind.step : 0.01
                    value: {
                        var c = editor.components()
                        return c[index] === undefined ? 0 : c[index]
                    }
                    onSettled: function (v) {
                        editor.send(editor.withComponent(index, v))
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: 1
            color: Theme.border
            opacity: 0.5
        }
    }
}
