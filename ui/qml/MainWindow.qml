import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

import "Colors.js" as Colors

RowLayout  {
    id: root
    spacing: 0

    Rectangle {
        Layout.fillWidth: false
        Layout.fillHeight: true
        Layout.preferredWidth: 175

        color: Colors.sidebarColor

        ColumnLayout {
            anchors.fill: parent

            spacing: 0

            TocksComboBox {
                id: accountSelector

                Layout.fillWidth: true

                model: tocks.accounts
                textRole: "name"
            }

            FriendsList {
                id: friendsList
                account: tocks.accounts[accountSelector.currentIndex]

                Layout.fillHeight: true
                Layout.fillWidth: true
            }

            Connections {
                target: friendsList

                function onFriendSelected(friend) {
                    if (tocks.accounts !== undefined) {
                        tocks.updateChatModel(tocks.accounts[accountSelector.currentIndex].id, friend.chatId)
                    }
                    chatRoom.friend = friend
                }
            }
        }
    }

    ChatRoom {
        id: chatRoom
        Layout.fillHeight: true
        Layout.fillWidth: true

        account: tocks.accounts[accountSelector.currentIndex]
        friend: undefined
    }
}


