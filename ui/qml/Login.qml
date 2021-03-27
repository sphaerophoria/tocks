import QtQuick 2.0
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.11
import QtQml.Models 2.15

import "Colors.js" as Colors

RowLayout {
    id: root

    width: 200

    function login() {
        if (comboBox.currentIndex != 0) {
            tocks.login(comboBox.currentText, password.text)
        } else {
            tocks.newAccount(name.text, password.text)
        }
    }

    ColumnLayout {
        Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter

        Layout.preferredWidth: 200
        Layout.minimumWidth: 200
        Layout.maximumWidth: 200

        Text {
            id: title
            Layout.fillWidth: true
            height: 42
            text: qsTr("TOCKS")
            font.pixelSize: 42
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.bold: true
        }

        Image {
            id: toxlogo
            x: 0
            Layout.fillWidth: true
            height: width
            source: "tox-logo.svg"
            sourceSize.width: width
            fillMode: Image.PreserveAspectFit
        }

        TocksComboBox {
            id: comboBox
            Layout.fillWidth: true

            model: tocks.offlineAccounts
        }

        TextField {
            id: name
            visible: comboBox.currentIndex == 0
            width: parent.width
            placeholderText: qsTr("Name")
        }

        TextField {
            id: password
            width: parent.width
            echoMode: TextField.Password
            placeholderText: qsTr("Password")

            onAccepted: login()
        }

        TocksButton {
            id: loginButton
            Layout.fillWidth: true
            text: qsTr("Login")

            onClicked:  login()
        }
    }
}



