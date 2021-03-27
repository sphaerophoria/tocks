use crate::contacts::Friend;

use qmetaobject::*;
use tocks::{AccountId, UserHandle, Status};
use toxcore::ToxId;

use std::{cell::RefCell, collections::HashMap, sync::Mutex};

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct Account {
    base: qt_base_class!(trait QObject),
    id: qt_property!(i64),
    userId: qt_property!(i64; NOTIFY userIdChanged),
    userIdChanged: qt_signal!(),
    toxId: qt_property!(QString; NOTIFY toxIdChanged),
    toxIdChanged: qt_signal!(),
    name: qt_property!(QString; NOTIFY nameChanged),
    nameChanged: qt_signal!(),
    friends: qt_property!(QVariantList; READ get_friends NOTIFY friendsChanged),
    friendsChanged: qt_signal!(),

    friends_storage: Mutex<HashMap<UserHandle, Box<RefCell<Friend>>>>,
}

impl Account {
    pub fn new(id: AccountId, user: UserHandle, address: ToxId, name: String) -> Account {
        Account {
            base: Default::default(),
            id: id.id(),
            userId: user.id(),
            userIdChanged: Default::default(),
            toxId: address.to_string().into(),
            toxIdChanged: Default::default(),
            name: name.into(),
            nameChanged: Default::default(),
            friends: Default::default(),
            friendsChanged: Default::default(),

            friends_storage: Default::default(),
        }
    }

    pub fn add_friend(&self, friend: &tocks::Friend) {
        let id = *friend.id();
        let friend = Box::new(RefCell::new(Friend::from(friend)));
        unsafe { QObject::cpp_construct(&friend) };
        self.friends_storage.lock().unwrap().insert(id, friend);
        self.friendsChanged()
    }

    pub fn get_friends(&self) -> QVariantList {
        self.friends_storage
            .lock()
            .unwrap()
            .values()
            .map(|item| unsafe { (&*item.borrow_mut() as &dyn QObject).as_qvariant() })
            .collect()
    }

    pub fn set_friend_status(&self, user_id: UserHandle, status: Status) {
        self.friends_storage.lock().unwrap()[&user_id]
            .borrow_mut()
            .set_status(status);
    }
}

