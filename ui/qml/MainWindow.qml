import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11


RowLayout  {
    Connections {
        target: tocks

        function onAccountActivated(account) {
            accountsModel.append(account)
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

    ColumnLayout {
        Layout.fillWidth: false

        Layout.maximumWidth: 500

        ComboBox {
            id: accountSelector

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

    ChatRoom {
        account: accountsModel.get(accountSelector.currentIndex).id
        chat: chatModel.chat
    }

}


