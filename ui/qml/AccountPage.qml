import QtQuick 2.0
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.12

Item {
    required property var account

    GridLayout {
        anchors.centerIn: parent
        columns: 2

        Image {
            Layout.alignment: Qt.AlignHCenter
            Layout.columnSpan: 2
            source: "res/placehoder_avatar.png"
        }

        Text {
            Layout.preferredWidth: 50
            text: "Name"
            horizontalAlignment: Text.AlignLeft
        }

        TextField {
            text: account.name
            selectByMouse: true
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
        }

        Text {
            Layout.preferredWidth: 50

            text: "Status"
            horizontalAlignment: Text.AlignLeft
        }

        TextField {
            selectByMouse: true
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
        }

        Text {
            text: "Tox ID"
        }

        TextEdit {
            id: toxId
            text: account.toxId
            Layout.maximumWidth: 300
            readOnly: true
            wrapMode: Text.WrapAnywhere
            selectByMouse: true

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.RightButton

                onClicked: {
                    toxId.persistentSelection = true
                    contextMenu.popup()
                    toxId.persistentSelection = false
                }
            }

            Menu {
                id: contextMenu
                MenuItem {
                    text: "Copy"
                    onTriggered: {
                        toxId.copy()
                    }
                }
            }
        }

        Text {
            Layout.columnSpan: 2
            text: "Blocked Users"
            font.bold: true
        }

        ListView {
            Layout.columnSpan: 2
            Layout.fillHeight: true
            Layout.fillWidth: true

            model: account.blockedUsers

            delegate: Text {
                anchors.fill: parent
                width: 300
                text: modelData.publicKey
            }
        }
    }
}

/*##^##
Designer {
    D{i:0;autoSize:true;height:480;width:640}
}
##^##*/
