import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

ColumnLayout {
    id: root

    required property string account
    required property string friend


    ChatLog {
        Layout.fillHeight: true
        id: chatLog
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: false
        Layout.minimumHeight: 100

        TextArea {
            id: messageText

            Layout.fillWidth: true
            Layout.fillHeight: true

            horizontalAlignment: TextEdit.AlignLeft
            placeholderText: "Type message..."
            wrapMode: TextEdit.Wrap

            Keys.onEnterPressed: {
                tocks.sendMessage(account, friend, text)
            }
        }

        Button {
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
            Layout.fillWidth: false
            Layout.fillHeight: true
            Layout.preferredHeight: 50
            Layout.preferredWidth: 100

            onClicked: {
                tocks.sendMessage(account, friend, messageText.text)
            }

            text: "Send"
        }
    }

}
