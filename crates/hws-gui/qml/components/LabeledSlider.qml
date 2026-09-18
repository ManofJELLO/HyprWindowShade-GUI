import QtQuick
import QtQuick.Controls.Basic
import dev.hyprwindowshade.gui

// A slider with a label and an editable number beside it.
//
// `moved` fires while dragging (for a live preview) and `settled` fires once the
// drag ends or the number is typed — which is when the file gets written, so a
// drag is one save rather than two hundred.
Item {
    id: root

    property string label: ""
    property real from: 0
    property real to: 1
    property real stepSize: 0.01
    property real value: 0
    property bool integer: false
    property bool enabled: true

    signal moved(real value)
    signal settled(real value)

    implicitHeight: Math.max(Theme.rowHeight, col.implicitHeight)
    implicitWidth: 320

    function format(v) {
        if (root.integer)
            return String(Math.round(v))
        var decimals = root.stepSize >= 1 ? 0 : (root.stepSize >= 0.1 ? 1 : (root.stepSize >= 0.01 ? 2 : 3))
        return v.toFixed(decimals)
    }

    Column {
        id: col
        width: parent.width
        spacing: 2

        Item {
            width: parent.width
            height: caption.implicitHeight

            Text {
                id: caption
                text: root.label
                color: Theme.fgDim
                font.pixelSize: Theme.fontSize
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
            }

            LineEdit {
                id: number
                anchors.right: parent.right
                width: 78
                height: 26
                implicitHeight: 26
                mono: true
                enabled: root.enabled
                horizontalAlignment: Text.AlignRight
                text: root.format(root.value)
                onCommit: function (v) {
                    var parsed = parseFloat(v)
                    if (isNaN(parsed))
                        parsed = root.value
                    parsed = Math.max(root.from, Math.min(root.to, parsed))
                    root.value = parsed
                    root.settled(parsed)
                }
            }
        }

        Slider {
            id: slider
            width: parent.width
            enabled: root.enabled
            from: root.from
            to: root.to
            stepSize: root.stepSize
            value: root.value
            snapMode: Slider.SnapAlways
            implicitHeight: 22

            onMoved: {
                root.value = value
                root.moved(value)
            }
            onPressedChanged: {
                if (!pressed)
                    root.settled(value)
            }

            background: Rectangle {
                x: slider.leftPadding
                y: slider.topPadding + slider.availableHeight / 2 - height / 2
                width: slider.availableWidth
                height: 4
                radius: 2
                color: Theme.bgAlt
                border.width: 1
                border.color: Theme.border

                Rectangle {
                    width: slider.visualPosition * parent.width
                    height: parent.height
                    radius: 2
                    color: root.enabled ? Theme.accent : Theme.muted
                }
            }

            handle: Rectangle {
                x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
                y: slider.topPadding + slider.availableHeight / 2 - height / 2
                width: 14
                height: 14
                radius: 7
                color: slider.pressed ? Theme.accent : Theme.surfaceHi
                border.width: 1
                border.color: root.enabled ? Theme.accent : Theme.border
            }
        }
    }
}
