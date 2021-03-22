import QtQuick 2.4
import QtQuick.Layouts 1.11
import QtQuick.Controls 2.15
import "Colors.js" as Colors

Rectangle {
    id: root

    property var account

    signal chatSelected(int chat_id);

    color: Colors.sidebarColor

    ColumnLayout {

        anchors.fill: parent
        spacing: 0

        Connections {
            target: tocks

            function onFriendAdded(accountId, friend) {
                if (root.account.id !== account.id) {
                    return
                }

                friendModel.append(friend)
            }
        }


        ListView {

            Layout.fillHeight: true

            id: friendList
            onCurrentItemChanged: {
                root.chatSelected(friendModel.get(currentIndex).chatId)
            }

            model: ListModel {
                id: friendModel
            }

            delegate: Rectangle {
                width: root.width
                height: 30

                color: friendList.currentIndex == index ? Colors.sidebarHighlight : "transparent"

                Text {
                    anchors.left: parent.left
                    anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter

                    text: model.name
                    color: Colors.sidebarText
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: friendList.currentIndex = index
                }
            }

            ScrollBar.vertical: ScrollBar {}
        }
    }
}

/*##^##
Designer {
    D{i:0;autoSize:true;height:480;width:640}
}
##^##*/
