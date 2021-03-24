import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

import "Colors.js" as Colors
import "SidebarConstants.js" as SidebarConstants

Rectangle {
    id: root
    property var selectedAccount
    property var selectedFriend

    signal settingsClicked()
    signal newAccountClicked()

    color: Colors.sidebarColor

    ColumnLayout {
        anchors.fill: parent

        spacing: 0

        ListView {
            id: sidebarAccounts

            Layout.fillWidth: true
            Layout.fillHeight: true

            model: tocks.accounts

            delegate: SidebarAccount {
                id: sidebarAccount
                anchors.left: parent.left
                anchors.right: parent.right
                account: modelData

                onAccountSelected: {
                    root.selectedAccount = account
                    root.selectedFriend = undefined
                }

                onSelectedFriendChanged: {
                    root.selectedAccount = account
                    root.selectedFriend = selectedFriend
                }

                MouseArea {
                    anchors.fill: sidebarAccount

                    propagateComposedEvents: true

                    onClicked: {
                        sidebarAccounts.currentItem.clearSelection()
                        sidebarAccounts.currentIndex = index
                        mouse.accepted = false
                    }
                }
            }

            ScrollBar.vertical: ScrollBar {}
        }

        //Rectangle {
        //    id: newAccountBox

        //    Layout.fillWidth: true
        //    Layout.minimumHeight: SidebarConstants.accountHeight
        //    color: "white"

        //    MouseArea {
        //        id: newAccountArea

        //        anchors.fill: parent

        //        onClicked: {
        //            root.newAccountClicked()
        //            sidebarAccounts.currentItem.clearSelection()
        //        }
        //    }
        //}

        Rectangle {
            id: settingsBox
            color: Colors.sidebarSettingsBackground

            Layout.fillWidth: true
            Layout.minimumHeight: SidebarConstants.settingsGearHeight

            Image {
                id: settingsImage
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                anchors.margins: SidebarConstants.contentMargins

                sourceSize.width: height

                source: "res/settings.svg"
            }

            MouseArea {
                id: settingsMouseArea

                anchors.fill: settingsImage

                cursorShape: Qt.PointingHandCursor

                onClicked: {
                    root.settingsClicked()
                    sidebarAccounts.currentItem.clearSelection()
                }
            }
        }
    }
}
