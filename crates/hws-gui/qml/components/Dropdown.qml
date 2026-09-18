import QtQuick
import QtQuick.Controls.Basic
import dev.hyprwindowshade.gui

ComboBox {
    id: control

    implicitHeight: Theme.rowHeight
    font.pixelSize: Theme.fontSize

    delegate: ItemDelegate {
        width: control.width
        highlighted: control.highlightedIndex === index
        contentItem: Text {
            text: control.textRole
                  ? (Array.isArray(control.model)
                     ? modelData[control.textRole]
                     : model[control.textRole])
                  : modelData
            color: Theme.fg
            font.pixelSize: Theme.fontSize
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            color: highlighted ? Theme.selection : Theme.surface
        }
    }

    indicator: Text {
        x: control.width - width - Theme.gap
        y: control.topPadding + (control.availableHeight - height) / 2
        text: "▾"
        color: Theme.muted
        font.pixelSize: Theme.fontSize
    }

    contentItem: Text {
        leftPadding: Theme.gap
        rightPadding: control.indicator.width + Theme.gap
        text: control.displayText
        color: control.enabled ? Theme.fg : Theme.muted
        font.pixelSize: Theme.fontSize
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: Theme.radiusSmall
        color: Theme.bgAlt
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.border
    }

    popup: Popup {
        y: control.height + 2
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + 2, 320)
        padding: 1

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            ScrollIndicator.vertical: ScrollIndicator {}
        }

        background: Rectangle {
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
            radius: Theme.radiusSmall
        }
    }
}
