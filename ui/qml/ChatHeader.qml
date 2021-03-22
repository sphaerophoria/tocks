import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

Rectangle {
    required property int chatId

    color: "white"

    Row {

        leftPadding: 10

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chatCache[0][chatId]["name"]
            font.bold: true
        }
    }
}
