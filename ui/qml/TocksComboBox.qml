import QtQuick 2.15
import QtQuick.Controls 2.15
import "Colors.js" as Colors

ComboBox {
    id: control

    background: Rectangle {
        radius: 3
        implicitWidth: 120
        implicitHeight: 40
        color: control.pressed ? Colors.buttonDown : Colors.button
    }

    delegate: ItemDelegate {
        width: control.width

        contentItem: Text {
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: 10

            text: control.textRole ? model[control.textRole] : modelData
            color: Colors.buttonText
            font: control.font
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }

        highlighted: control.highlightedIndex === index
    }

    contentItem: Text {
        leftPadding: 10
        rightPadding: control.indicator.width + control.spacing

        text: control.displayText
        font: control.font
        color: Colors.buttonText
        verticalAlignment: Text.AlignVCenter
    }


    popup: Popup {
        y: control.height - 1
        width: control.width
        implicitHeight: contentItem.implicitHeight
        padding: 1

        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex

            delegate: Rectangle {
                anchors.fill: parent
                width: control.width
                color: Colors.button
            }

            ScrollIndicator.vertical: ScrollIndicator { }
        }

        background: Rectangle {
            color: Colors.button
            radius: 2
        }
    }
}
