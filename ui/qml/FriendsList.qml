import QtQuick 2.4
import QtQuick.Layouts 1.11
import QtQuick.Controls 2.15
import "Colors.js" as Colors

Rectangle {
    id: root

    property var account

    signal friendSelected(var friend);

    color: Colors.sidebarColor

    ColumnLayout {

        anchors.fill: parent
        spacing: 0

        ListView {

            Layout.fillHeight: true

            id: friendList

            onCurrentItemChanged: {
                root.friendSelected(account.friends[currentIndex])
            }

            model: account.friends

            delegate: Rectangle {
                width: root.width
                height: 30

                color: friendList.currentIndex == index ? Colors.sidebarHighlight : "transparent"

                RowLayout {
                    anchors.fill: parent
                    width: root.width

                    Text {
                        Layout.fillWidth: true
                        Layout.leftMargin: 10
                        Layout.alignment: Qt.AlignVCenter
                        text: modelData.name
                        color: Colors.sidebarText
                    }

                    StatusIcon {
                        Layout.fillHeight: true
                        Layout.margins: 10
                        Layout.preferredWidth: height

                        status: modelData.status
                    }
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
