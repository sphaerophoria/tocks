import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

ScrollView {
    ListView {
        id: root

        anchors.fill: parent

        // Global chatModel defined in rust
        model: chatModel
        delegate: Text {
            text: model.display
            wrapMode: Text.Wrap
        }
    }
}
