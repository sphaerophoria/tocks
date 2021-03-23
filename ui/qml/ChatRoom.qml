import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

import "Colors.js" as Colors

Rectangle {
    id: root

    required property var account
    required property var friend

    color: "white"

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ChatHeader {
            friend: root.friend

            Layout.fillWidth: true
            Layout.minimumHeight: 40
        }

        TocksSpacer {
            Layout.fillWidth: true
        }

        ChatLog {
            account: root.account

            z: -1

            Layout.fillHeight: true
            Layout.fillWidth: true
            id: chatLog
        }

        TocksSpacer {
            Layout.fillWidth: true
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: false

            Layout.minimumHeight: 100

            TextArea {
                id: messageText

                anchors.fill: parent

                horizontalAlignment: TextEdit.AlignLeft
                placeholderText: "Type message..."
                wrapMode: TextEdit.Wrap

                function handleReturn(event) {
                    if ((event.modifiers & Qt.ShiftModifier)) {
                        event.accepted = false
                        return
                    }
                    tocks.sendMessage(account.id, friend.chatId, text)
                    text = ""
                }

                Keys.onReturnPressed: {
                    handleReturn(event)
                }

                Keys.onEnterPressed: {
                    handleReturn(event)
                }
            }
        }

    }
}
