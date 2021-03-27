import QtQuick 2.15
import QtQuick.Layouts 1.15

import "Colors.js" as Colors
import "SidebarConstants.js" as SidebarConstants

Item {
    id: root

    property var account
    property var selectedFriend

    signal accountSelected()

    function clearSelection() {
        accountRow.selected = false
        friendsList.clearSelection()
    }

    height: accountRow.height + friendsList.height

    Column {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            id: accountRow

            property bool selected: false

            function getColor() {
                if (selected) {
                    return Colors.sidebarHighlight
                } else if (mouseArea.containsMouse) {
                    return Colors.sidebarAccountHover
                } else {
                    return Colors.sidebarAccount
                }
            }

            anchors.left: parent.left
            anchors.right: parent.right

            color: getColor()
            height: SidebarConstants.accountHeight

            Text {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter

                anchors.leftMargin: SidebarConstants.contentMargins
                anchors.rightMargin: SidebarConstants.contentMargins

                text: account.name
                color: Colors.sidebarText
            }

            MouseArea {
                id: mouseArea
                anchors.fill: parent
                hoverEnabled: true

                onClicked: {
                    accountRow.selected = true
                    friendsList.clearSelection()
                    root.accountSelected()
                }
            }
        }

        FriendsList {
            id: friendsList
            account: root.account

            anchors.left: parent.left
            anchors.right: parent.right

            onSelectedFriendChanged: {
                if (selectedFriend !== undefined) {
                    accountRow.selected = false
                }
                root.selectedFriend = selectedFriend
            }
        }
    }
}
