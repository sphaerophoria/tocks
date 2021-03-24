import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

Rectangle {
    required property var friend

    color: "white"

    Row {
        leftPadding: 10

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: friend !== undefined ? friend.name : ""
            font.bold: true
        }
    }
}
