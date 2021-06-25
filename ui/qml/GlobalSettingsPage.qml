import QtQuick 2.0
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.12

Item {
    GridLayout {
        anchors.centerIn: parent
        columns: 2

        Text {
            Layout.columnSpan: 2
            font.bold: true
            text: "Audio Settings"
        }

        Text {
            Layout.preferredWidth: 150
            text: "Output Device"
            horizontalAlignment: Text.AlignLeft
        }

        TocksComboBox {
            model: tocks.audioOutputs

            onCurrentIndexChanged: {
                tocks.setAudioOutput(currentIndex)
            }

            Layout.preferredWidth: 400
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
        }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            Layout.columnSpan: 2
            TocksButton {
                Layout.fillWidth: true
                text: "Start audio test"
                onClicked: {
                    tocks.startAudioTest()
                }
            }

            TocksButton {
                Layout.fillWidth: true
                text: "Stop audio test"
                onClicked: {
                    tocks.stopAudioTest()
                }
            }
        }

        Text {
            Layout.columnSpan: 2
            font.bold: true
            text: "Attributions"
        }

        Text {
            Layout.columnSpan: 2
            text: tocks.attribution
        }
    }
}
