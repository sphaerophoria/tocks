use qmetaobject::*;

#[allow(non_snake_case)]
#[derive(QGadget, Clone, Default)]
pub struct Friend {
    publicKey: qt_property!(QString),
    name: qt_property!(QString),
}

impl From<&tocks::contact::FriendData> for Friend {
    fn from(data: &tocks::contact::FriendData) -> Friend {
        let mut friend = Friend::default();

        friend.publicKey = data.public_key().to_string().into();
        friend.name = data.name().to_string().into();

        friend
    }
}

#[derive(QGadget, Clone, Default)]
pub struct FriendRequest {
    pub sender: qt_property!(QString),
    pub message: qt_property!(QString),
}
