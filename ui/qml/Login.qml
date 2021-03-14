import QtQuick 2.0
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.11
import QtQml.Models 2.15

RowLayout {
    id: root

    signal login(string accountName, string password)
    signal newAccount(string password)

    Connections {

        target: tocks

        function onInactiveAccountAdded(name) {
            accountModel.append({name: name})
        }
    }

    width: 200

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

        ComboBox {
            id: comboBox
            Layout.fillWidth: true

            model: ListModel {
                id: accountModel

                ListElement {
                    name: "Create a new account..."
                }
            }
        }

        TextField {
            id: password
            width: parent.width
            echoMode: TextField.Password
            placeholderText: qsTr("Password")
        }

        Button {
            id: loginButton
            Layout.fillWidth: true
            text: qsTr("Login")

            onClicked: {
                if (comboBox.currentIndex != 0) {
                    root.login(comboBox.currentText, password.text)
                } else {
                    root.newAccount(password.text)
                }
            }
        }
    }
}



