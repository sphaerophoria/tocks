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
    }
}
