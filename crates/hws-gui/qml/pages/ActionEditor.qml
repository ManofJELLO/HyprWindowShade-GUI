import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import dev.hyprwindowshade.gui

// Edits one plugin call — the thing a keybind runs, or a startup action does.
Item {
    id: editor

    property var action: null

    signal edited(var action)

    readonly property var kinds: [
        { id: "toggle_window_shader", label: "Toggle on the focused window",
          wants: "shader" },
        { id: "toggle_class_shader",  label: "Toggle on every window of a class",
          wants: "class+shader" },
        { id: "class_shader",         label: "Force on every window of a class",
          wants: "class+shader?" },
        { id: "toggle_layer_shader",  label: "Toggle on a layer namespace",
          wants: "namespace+shader" },
        { id: "layer_shader",         label: "Force on a layer namespace",
          wants: "namespace+shader?" },
        { id: "reload_shaders",       label: "Reload all shaders", wants: "" }
    ]

    readonly property string kindId: editor.action && editor.action.action
                                     ? editor.action.action : "reload_shaders"
    readonly property var kind: {
        for (var i = 0; i < editor.kinds.length; ++i)
            if (editor.kinds[i].id === editor.kindId)
                return editor.kinds[i]
        return editor.kinds[editor.kinds.length - 1]
    }

    readonly property bool wantsClass: editor.kind.wants.indexOf("class") >= 0
    readonly property bool wantsNamespace: editor.kind.wants.indexOf("namespace") >= 0
    readonly property bool wantsShader: editor.kind.wants.indexOf("shader") >= 0
    readonly property bool shaderOptional: editor.kind.wants.indexOf("shader?") >= 0

    implicitHeight: column.implicitHeight

    function currentShaderPath() {
        if (editor.action && editor.action.shader && editor.action.shader.path)
            return editor.action.shader.path
        return ""
    }

    function emitWith(overrides) {
        var next = { action: editor.kindId }
        if (editor.wantsClass)
            next["class"] = editor.action && editor.action["class"] ? editor.action["class"] : ""
        if (editor.wantsNamespace)
            next.namespace = editor.action && editor.action.namespace
                             ? editor.action.namespace : ""
        if (editor.wantsShader) {
            var p = editor.currentShaderPath()
            next.shader = p === "" ? null : { path: p }
        }
        for (var key in overrides)
            next[key] = overrides[key]
        // A "toggle" with no shader is meaningless, so it is never emitted.
        if (editor.wantsShader && !editor.shaderOptional && !next.shader)
            return
        editor.edited(next)
    }

    Column {
        id: column
        width: parent.width
        spacing: Theme.gapSmall

        Dropdown {
            width: parent.width
            model: editor.kinds
            textRole: "label"
            currentIndex: {
                for (var i = 0; i < editor.kinds.length; ++i)
                    if (editor.kinds[i].id === editor.kindId)
                        return i
                return 0
            }
            onActivated: function (index) {
                var next = { action: editor.kinds[index].id }
                var wants = editor.kinds[index].wants
                if (wants.indexOf("class") >= 0)
                    next["class"] = editor.action && editor.action["class"]
                                    ? editor.action["class"] : ""
                if (wants.indexOf("namespace") >= 0)
                    next.namespace = editor.action && editor.action.namespace
                                     ? editor.action.namespace : ""
                if (wants.indexOf("shader") >= 0) {
                    var p = editor.currentShaderPath()
                    next.shader = p === "" ? null : { path: p }
                }
                editor.edited(next)
            }
        }

        RowLayout {
            width: parent.width
            visible: editor.wantsClass
            spacing: Theme.gapSmall

            LineEdit {
                Layout.fillWidth: true
                mono: true
                placeholderText: "window class"
                text: editor.action && editor.action["class"] ? editor.action["class"] : ""
                onCommit: function (v) {
                    editor.emitWith({ "class": v.trim() })
                }
            }

            Dropdown {
                Layout.preferredWidth: 150
                visible: App.liveClasses.length > 0
                model: ["open windows…"].concat(App.liveClasses)
                currentIndex: 0
                onActivated: function (index) {
                    if (index === 0)
                        return
                    editor.emitWith({ "class": App.liveClasses[index - 1] })
                    currentIndex = 0
                }
            }
        }

        RowLayout {
            width: parent.width
            visible: editor.wantsNamespace
            spacing: Theme.gapSmall

            LineEdit {
                Layout.fillWidth: true
                mono: true
                placeholderText: "layer namespace"
                text: editor.action && editor.action.namespace ? editor.action.namespace : ""
                onCommit: function (v) {
                    editor.emitWith({ namespace: v.trim() })
                }
            }

            Dropdown {
                Layout.preferredWidth: 150
                visible: App.liveNamespaces.length > 0
                model: ["live layers…"].concat(App.liveNamespaces)
                currentIndex: 0
                onActivated: function (index) {
                    if (index === 0)
                        return
                    editor.emitWith({ namespace: App.liveNamespaces[index - 1] })
                    currentIndex = 0
                }
            }
        }

        ShaderPicker {
            width: parent.width
            visible: editor.wantsShader
            path: editor.currentShaderPath()
            placeholder: editor.shaderOptional ? "clear the shader" : "pick a shader"
            onPicked: function (path, duration, isDefault) {
                editor.emitWith({ shader: path === "" ? null : { path: path } })
            }
        }
    }
}
