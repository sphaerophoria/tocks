import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import "Colors.js" as Colors


ListView {
    id: root

    topMargin: 5
    bottomMargin: 5

    required property var account

    width: 600

    spacing: 10
    verticalLayoutDirection: ListView.BottomToTop

    // Global chatModel defined in rust
    model: chatModel
    delegate: Rectangle {
        property bool sentByMe: model.senderId == account.userId

        property int bubbleTextHorizPadding: 20
        property int bubbleTextVertPadding: 15
        property int bubbleHorizPadding: 10

        anchors.right: sentByMe ? root.contentItem.right : undefined
        anchors.left: sentByMe ? undefined : root.contentItem.left

        anchors.leftMargin: bubbleHorizPadding
        anchors.rightMargin: bubbleHorizPadding

        color: sentByMe ? Colors.selfColor : Colors.friendColor

        height: messageText.height + bubbleTextVertPadding
        width: messageText.paintedWidth + bubbleTextHorizPadding
        radius: 5

        Text {
            id: messageText

            anchors.left: parent.left
            anchors.leftMargin: bubbleTextHorizPadding / 2
            anchors.verticalCenter: parent.verticalCenter

            width: 500

            text: model.message
            wrapMode: Text.Wrap
        }
    }

    ScrollBar.vertical: ScrollBar {}
}
