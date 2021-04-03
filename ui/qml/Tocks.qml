import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import "Colors.js" as Colors

ApplicationWindow {
    id: tocksWindow

    title: "Tocks"
    visible: true
    width: 960
    height: 720

    onClosing: {
        tocks.close()
        Qt.quit()
    }

    color: Colors.background

    onActiveFocusControlChanged: {
        tocks.visible = activeFocusControl !== null
    }


    Connections {
        target: tocks

        function onAccountsChanged() {
            if (tocks.accounts.length === 1) {
                applicationStack.replace(login, mainWindow)
            }
        }

        function onError(error) {
            console.log(error)
        }
    }

    Login {
        id: login
        visible: false

        width: parent.width
        height: parent.height
    }

    MainWindow {
        id: mainWindow
        visible: false

        width: parent.width
        height: parent.height
    }

    StackView {
        id: applicationStack
        initialItem: login

        anchors.fill: parent
    }
}
