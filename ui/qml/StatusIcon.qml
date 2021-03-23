import QtQuick 2.15

import "Colors.js" as Colors

Rectangle {
    property string status: "online"

    radius: width / 2

    function colorFromStatus(status) {
        if (status === "online") {
            return Colors.onlineStatus
        }
        else if (status === "away") {
            return Colors.awayStatus
        }
        else if (status === "busy") {
            return Colors.busyStatus
        }
        else {
            return Colors.offlineStatus
        }
    }

    color: colorFromStatus(status)
}

/*##^##
Designer {
    D{i:0;autoSize:true;height:480;width:640}
}
##^##*/
