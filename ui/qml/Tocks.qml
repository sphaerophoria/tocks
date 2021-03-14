import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15

ApplicationWindow {
    id: tocksWindow

    title: "Tocks"
    visible: true
    width: 800
    height: 600

    onClosing: {
        tocks.close()
        Qt.quit()
    }


    Connections {
        target: tocks

        function onAccountActivated() {
            applicationStack.replace(login, mainWindow)
        }

        function onError(error) {
            console.log(error)
        }
    }

    Component.onCompleted: {
        login.login.connect(tocks.login)
        login.newAccount.connect(tocks.newAccount)
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
