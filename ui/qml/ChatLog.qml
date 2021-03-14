import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import QtQuick.Layouts 1.11

ScrollView {
    ListView {
        id: root

        Layout.fillWidth: true
        Layout.fillHeight: true

        // Global chatModel defined in rust
        model: chatModel
        delegate: Text {
            text: model.display
        }
    }
}
