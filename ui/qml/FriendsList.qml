import QtQuick 2.4
import QtQuick.Layouts 1.11
import QtQuick.Controls 2.15
import "Colors.js" as Colors
import "SidebarConstants.js" as SidebarConstants

Rectangle {
    id: root

    property var account
    property var selectedFriend

    function clearSelection() {
        friendList.currentIndex = -1
    }

    color: Colors.sidebarColor

    // As far as I understand the ListView doesn't know how large the view
    // will be until it is rendered. I want this to be dynamically sized to
    // the contents, so inform the root object of the size I know will will be
    // calculated by the listview
    height: account.friends.length * SidebarConstants.friendHeight

    ListView {
        id: friendList

        anchors.fill: parent
        interactive: false

        onCurrentItemChanged: {
            root.selectedFriend = account.friends[currentIndex]
        }

        model: account.friends

        delegate: Rectangle {
            width: root.width
            height: SidebarConstants.friendHeight

            function getColor() {
                if (friendList.currentIndex === index) {
                    return Colors.sidebarHighlight
                } else if (mouseArea.containsMouse) {
                    return Colors.sidebarItemHover
                } else {
                    return "transparent"
                }
            }

            color: getColor()

            RowLayout {
                anchors.fill: parent
                width: root.width

                Text {
                    Layout.fillWidth: true
                    Layout.leftMargin: SidebarConstants.contentMargins
                    Layout.alignment: Qt.AlignVCenter

                    property bool unread: modelData.chatModel.lastReadTime < modelData.chatModel.lastMessageTime

                    text: {
                        console.log(unread)
                        var chatModel = modelData.chatModel
                        if (unread) {
                            return modelData.name + " (unread)"
                        } else {
                            return modelData.name
                        }
                    }
                    color: Colors.sidebarText
                }

                StatusIcon {
                    Layout.fillHeight: true
                    Layout.margins: SidebarConstants.contentMargins
                    Layout.preferredWidth: height

                    status: modelData.status
                }
            }


            MouseArea {
                id: mouseArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: friendList.currentIndex = index
            }
        }

        ScrollBar.vertical: ScrollBar {}
    }
}

/*##^##
Designer {
    D{i:0;autoSize:true;height:480;width:640}
}
##^##*/
