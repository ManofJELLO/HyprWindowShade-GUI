import QtQuick
import dev.hyprwindowshade.gui

// A labelled row: caption on the left, one control on the right.
Item {
    id: row

    property string label: ""
    property string hint: ""
    property int labelWidth: 150
    default property alias control: holder.data

    implicitHeight: Math.max(Theme.rowHeight, holder.childrenRect.height, caption.implicitHeight)
    implicitWidth: labelWidth + Theme.gap + holder.childrenRect.width

    Column {
        id: caption
        width: row.labelWidth
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: 7
        spacing: 1

        Text {
            text: row.label
            width: parent.width
            wrapMode: Text.WordWrap
            color: Theme.fgDim
            font.pixelSize: Theme.fontSize
        }
        Text {
            text: row.hint
            visible: text !== ""
            width: parent.width
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.pixelSize: Theme.fontSizeSmall
        }
    }

    Item {
        id: holder
        anchors {
            left: caption.right
            leftMargin: Theme.gap
            right: parent.right
            top: parent.top
        }
        implicitHeight: childrenRect.height
    }
}
