use crate::{call_state_to_qtring, status_to_qstring};

use qmetaobject::*;
use tocks::{CallState, Friend as TocksFriend, Status};

#[allow(non_snake_case)]
#[derive(QObject, Default)]
pub struct Friend {
    base: qt_base_class!(trait QObject),
    chatId: qt_property!(i64; NOTIFY chatIdChanged),
    chatIdChanged: qt_signal!(),
    userId: qt_property!(i64; NOTIFY userIdChanged),
    userIdChanged: qt_signal!(),
    publicKey: qt_property!(QString; NOTIFY publicKeyChanged),
    publicKeyChanged: qt_signal!(),
    name: qt_property!(QString; NOTIFY nameChanged),
    nameChanged: qt_signal!(),
    status: qt_property!(QString; NOTIFY statusChanged),
    statusChanged: qt_signal!(),
    callState: qt_property!(QString; NOTIFY callStateChanged),
    callStateChanged: qt_signal!(),
}

impl Friend {
    pub fn chat_id(&self) -> i64 {
        self.chatId
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status_to_qstring(&status);
        self.statusChanged();
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = QString::from(name);
        self.nameChanged();
    }

    pub fn set_call_state(&mut self, state: &CallState) {
        self.callState = call_state_to_qtring(state);
        self.callStateChanged()
    }
}

impl From<&TocksFriend> for Friend {
    fn from(friend: &TocksFriend) -> Self {
        Self {
            base: Default::default(),
            chatId: friend.chat_handle().id(),
            chatIdChanged: Default::default(),
            userId: friend.id().id(),
            userIdChanged: Default::default(),
            publicKey: friend.public_key().to_string().into(),
            publicKeyChanged: Default::default(),
            name: friend.name().to_string().into(),
            nameChanged: Default::default(),
            status: status_to_qstring(friend.status()),
            statusChanged: Default::default(),
            callState: call_state_to_qtring(&CallState::Idle),
            callStateChanged: Default::default(),
        }
    }
}

#[allow(non_snake_case)]
#[derive(QGadget, Clone, Default)]
pub struct User {
    pub id: qt_property!(i64),
    pub name: qt_property!(QString),
    pub publicKey: qt_property!(QString),
}
