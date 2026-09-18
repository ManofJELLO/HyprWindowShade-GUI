import QtQuick
import dev.hyprwindowshade.gui

// A group heading with a hairline running to the right edge.
Item {
    id: header

    property string text: ""
    property string hint: ""

    implicitHeight: column.implicitHeight + Theme.gapSmall
    implicitWidth: 200

    Column {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2

        Row {
            spacing: Theme.gap
            width: parent.width

            Text {
                id: label
                text: header.text
                color: Theme.fgDim
                font.pixelSize: Theme.fontSizeSmall
                font.weight: Font.DemiBold
                font.capitalization: Font.AllUppercase
                font.letterSpacing: 0.6
            }

            Rectangle {
                width: Math.max(0, parent.width - label.width - Theme.gap)
                height: 1
                color: Theme.border
                anchors.verticalCenter: label.verticalCenter
            }
        }

        Text {
            text: header.hint
            visible: text !== ""
            width: parent.width
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: Theme.fontSizeSmall
        }
    }
}
