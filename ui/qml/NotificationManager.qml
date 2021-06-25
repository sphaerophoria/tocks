QtObject {
    id: root
    property var notificationState: new Map()

    Repeater {
        id: accountRepeater
        model: tocks.accounts

        Repeater {
            model: modelData.friends

            Item {
                Connections {
                    target: modelData

                    function updateNotification() {
                        accountId = accountRepeater.modelData.id
                        chatId = modelData.chatId
                        if (notificationState.get(accountId) === undefined) {
                            notificationState.set(accountId, new Map())
                        }
                        notificationState.get(accountId).set(chatId, true)
                    }

                    function onLastMessageTimeChanged() {
                        updateNotification()
                    }

                    function onLastReadTimeChanged() {
                        updateNotification()
                    }
                }
            }
        }
    }

}
