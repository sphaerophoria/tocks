import QtQuick 2.4
import QtQuick.Layouts 1.11
import QtQuick.Controls 2.15

ColumnLayout {
    id: root

    property var account

    signal friendSelected(string public_key);

    Connections {
        target: tocks

        function onFriendAdded(account, friend) {
            if (root.account.publicKey !== account.publicKey) {
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
                root.friendSelected(friendModel.get(currentIndex).publicKey)
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
