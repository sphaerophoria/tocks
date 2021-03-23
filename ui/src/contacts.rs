use crate::status_to_qstring;

use qmetaobject::*;
use tocks::Friend as TocksFriend;

#[allow(non_snake_case)]
#[derive(QGadget, Clone, Default)]
pub struct Friend {
    chatId: qt_property!(i64),
    userId: qt_property!(i64),
    publicKey: qt_property!(QString),
    name: qt_property!(QString),
    status: qt_property!(QString),
}

impl From<&TocksFriend> for Friend {
    fn from(friend: &TocksFriend) -> Self {
        Self {
            chatId: friend.chat_handle().id(),
            userId: friend.id().id(),
            publicKey: friend.public_key().to_string().into(),
            name: friend.name().to_string().into(),
            status: status_to_qstring(friend.status())
        }
    }
}

#[derive(QGadget, Clone, Default)]
pub struct FriendRequest {
    pub sender: qt_property!(QString),
    pub message: qt_property!(QString),
}
