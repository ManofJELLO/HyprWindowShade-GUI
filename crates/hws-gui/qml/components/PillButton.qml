import QtQuick
import QtQuick.Controls.Basic
import dev.hyprwindowshade.gui

Button {
    id: control

    property bool primary: false
    property bool danger: false
    property string tooltip: ""

    implicitHeight: Theme.rowHeight
    padding: Theme.gap
    leftPadding: Theme.pad
    rightPadding: Theme.pad
    hoverEnabled: true

    readonly property color _tint: danger ? Theme.error
                                          : (primary ? Theme.accent : Theme.border)

    background: Rectangle {
        radius: Theme.radiusSmall
        color: {
            if (!control.enabled)
                return "transparent"
            if (control.primary)
                return control.down ? Qt.darker(Theme.accent, 1.2) : Theme.accent
            if (control.down)
                return Theme.selection
            return control.hovered ? Theme.surfaceHi : "transparent"
        }
        border.width: 1
        border.color: control.enabled ? control._tint : Theme.border
        opacity: control.enabled ? 1.0 : 0.45

        Behavior on color {
            ColorAnimation { duration: 90 }
        }
    }

    contentItem: Text {
        text: control.text
        color: {
            if (!control.enabled)
                return Theme.muted
            if (control.primary)
                return Theme.onAccent
            if (control.danger)
                return Theme.error
            return Theme.fg
        }
        font.pixelSize: Theme.fontSize
        font.weight: control.primary ? Font.DemiBold : Font.Normal
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    ToolTip.visible: control.tooltip !== "" && control.hovered
    ToolTip.text: control.tooltip
    ToolTip.delay: 500
}
