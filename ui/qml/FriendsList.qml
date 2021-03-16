import QtQuick 2.4
import QtQuick.Layouts 1.11
import QtQuick.Controls 2.15

ColumnLayout {
    id: root

    property var account

    signal chatSelected(int chat_id);

    Connections {
        target: tocks

        function onFriendAdded(accountId, friend) {
            if (root.account.id !== account.id) {
                return
            }

            friendModel.append(friend)
        }
    }

    ScrollView {
        Layout.fillHeight: true

        ListView {
            id: friendList
            onCurrentItemChanged: {
                root.chatSelected(friendModel.get(currentIndex).chatId)
            }

            model: ListModel {
                id: friendModel
            }

            delegate: Rectangle {
                width: 100
                height: 20

                color: friendList.currentIndex == index ? "lightblue" : "transparent"

                Text {
                    text: model.name
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: friendList.currentIndex = index
                }
            }
        }
    }
}

/*##^##
Designer {
    D{i:0;autoSize:true;height:480;width:640}
}
##^##*/
