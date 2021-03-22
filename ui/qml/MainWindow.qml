import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

import "Colors.js" as Colors

RowLayout  {
    id: root
    // Stored as friendCache[accountId][friendId] = Friend
    property var userCache: ({})
    property var chatCache: ({})

    spacing: 0

    Connections {
        target: tocks

        function onAccountActivated(account) {
            accountsModel.append(account)
            root.userCache[account.id] = {}
            root.userCache[account.id][account.userId] = account

            root.chatCache[account.id] = {}
        }

        function onFriendAdded(accountId, friend) {
            root.userCache[accountId][friend.userId] = friend
            root.chatCache[accountId][friend.chatId] = {"name": friend.name}
        }
    }

    ListModel {
        id: accountsModel

        onCountChanged: {
            if (accountSelector.currentIndex === -1) {
                accountSelector.currentIndex = 0
            }
        }
    }


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

                model: accountsModel
                textRole: "name"
            }

            FriendsList {
                id: friendsList

                account: accountsModel.get(accountSelector.currentIndex)

                Layout.fillHeight: true
                Layout.fillWidth: true
            }

            Connections {
                target: friendsList

                function onChatSelected(chat_id) {
                    tocks.updateChatModel(friendsList.account.id, chat_id)
                }
            }
        }
    }

    ChatRoom {
        Layout.fillHeight: true
        Layout.fillWidth: true

        account: accountsModel.get(accountSelector.currentIndex)
        chatId: chatModel.chat
    }
}


