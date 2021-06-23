use crate::contacts::{Friend, User};

use qmetaobject::*;
use tocks::{AccountId, CallState, ChatHandle, Status, UserHandle};
use toxcore::ToxId;

use std::{cell::RefCell, collections::HashMap};

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
    blockedUsers: qt_property!(QVariantList; READ get_blocked_users NOTIFY blockedUsersChanged),
    blockedUsersChanged: qt_signal!(),

    friends_storage: HashMap<UserHandle, Box<RefCell<Friend>>>,
    blocked_users_storage: HashMap<UserHandle, User>,
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
            blockedUsers: Default::default(),
            blockedUsersChanged: Default::default(),

            friends_storage: Default::default(),
            blocked_users_storage: Default::default(),
        }
    }

    pub fn add_friend(&mut self, friend: &tocks::Friend) {
        let id = *friend.id();
        let friend = Box::new(RefCell::new(Friend::from(friend)));
        unsafe { QObject::cpp_construct(&friend) };
        self.friends_storage.insert(id, friend);
        self.friendsChanged()
    }

    pub fn remove_friend(&mut self, user_id: UserHandle) {
        // Keep a reference to the removed friend so it does not go out of scope
        // until QML stops using it
        let _friend = self.friends_storage.remove(&user_id);
        self.friendsChanged()
    }

    pub fn get_friends(&mut self) -> QVariantList {
        self.friends_storage
            .values()
            .map(|item| unsafe { (&*item.borrow_mut() as &dyn QObject).as_qvariant() })
            .collect()
    }

    pub fn set_friend_status(&mut self, user_id: UserHandle, status: Status) {
        self.friends_storage[&user_id]
            .borrow_mut()
            .set_status(status);
    }

    pub fn set_user_name(&mut self, user_id: UserHandle, name: &str) {
        self.friends_storage[&user_id].borrow_mut().set_name(name);
    }

    pub fn add_blocked_user(&mut self, user: &tocks::User) {
        // Assume we are not duplicating our blocked users
        let qt_user = User {
            id: user.id().id(),
            publicKey: user.public_key().to_string().into(),
            name: user.name().into(),
        };
        self.blocked_users_storage.insert(*user.id(), qt_user);
        self.blockedUsersChanged();
    }

    pub fn self_id(&mut self) -> UserHandle {
        UserHandle::from(self.userId)
    }

    pub fn set_call_state(&mut self, chat_id: ChatHandle, state: &CallState) {
        let item = self
            .friends_storage
            .iter_mut()
            .find(|(_id, f)| f.borrow().chat_id() == chat_id.id());

        if let Some((_, friend)) = item {
            friend.borrow_mut().set_call_state(state)
        }
    }

    fn get_blocked_users(&mut self) -> QVariantList {
        self.blocked_users_storage
            .values()
            .map(|item| item.to_qvariant())
            .collect()
    }
}
