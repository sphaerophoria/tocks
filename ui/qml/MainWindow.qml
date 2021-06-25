import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

import "Colors.js" as Colors
import "SidebarConstants.js" as SidebarConstants

RowLayout  {
    id: root
    required property bool windowVisible
    spacing: 0

    Sidebar {
        id: sidebar
        Layout.fillWidth: false
        Layout.fillHeight: true
        Layout.preferredWidth: SidebarConstants.sidebarWidth

        onSelectedAccountChanged: {
            if (selectedFriend === undefined) {
                contentLoader.sourceComponent = accountPage
            }
        }

        onSelectedFriendChanged: {
            if (selectedAccount !== undefined && selectedFriend !== undefined) {
                contentLoader.sourceComponent = chatRoom
            }
        }
    }

    Connections {
        target: sidebar

        function onNewAccountClicked() {
            contentLoader.sourceComponent = loginPage
        }

        function onSettingsClicked() {
            contentLoader.sourceComponent = settingsPage
        }
    }

    Loader {
        id: contentLoader
        Layout.fillHeight: true
        Layout.fillWidth: true
    }

    Component {
        id: chatRoom
        ChatRoom {
            anchors.fill: parent
            account: sidebar.selectedAccount
            friend: sidebar.selectedFriend
            windowVisible: root.windowVisible
        }
    }

    Component {
        id: settingsPage
        GlobalSettingsPage {
            anchors.fill: parent
        }
    }

    Component {
        id: accountPage
        AccountPage {
            anchors.fill: parent
            account: sidebar.selectedAccount
        }
    }

    Component {
        id: loginPage
        Login {
            anchors.fill: parent
        }
    }
}


