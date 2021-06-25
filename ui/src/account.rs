use crate::{
    chat_model::ChatModel,
    contacts::{Friend, User},
};

use qmetaobject::*;
use tocks::{AccountId, ChatHandle, UserHandle};
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

    friend_by_user_handle: HashMap<UserHandle, usize>,
    friend_by_chat_handle: HashMap<ChatHandle, usize>,
    friends_storage: Vec<Box<RefCell<Friend>>>,
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

            friend_by_user_handle: Default::default(),
            friend_by_chat_handle: Default::default(),
            friends_storage: Default::default(),
            blocked_users_storage: Default::default(),
        }
    }

    pub fn add_friend(&mut self, friend: &tocks::Friend, chat_model: ChatModel) {
        let friend = Box::new(RefCell::new(Friend::new(friend, chat_model)));
        unsafe { QObject::cpp_construct(&friend) };
        self.friends_storage.push(friend);
        self.reset_friend_keys();
        self.friendsChanged()
    }

    pub fn remove_friend(&mut self, user_id: UserHandle) {
        // Keep a reference to the removed friend so it does not go out of scope
        // until QML stops using it
        let idx = self.friend_by_user_handle.get(&user_id).unwrap();
        let _friend = self.friends_storage.remove(*idx);
        self.reset_friend_keys();
        self.friendsChanged()
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

    pub fn friend_from_user_handle(&mut self, user_handle: &UserHandle) -> Option<&RefCell<Friend>>{
        let idx = match self.friend_by_user_handle.get(&user_handle) {
            Some(idx) => *idx,
            None => return None,
        };

        Some(&*self.friends_storage[idx])
    }

    pub fn chat_from_chat_handle(&mut self, chat_handle: &ChatHandle) -> Option<&RefCell<ChatModel>>{
        let idx = match self.friend_by_chat_handle.get(&chat_handle) {
            Some(idx) => *idx,
            None => return None,
        };

        Some(self.friends_storage[idx].get_mut().chat_model())
    }

    fn get_friends(&mut self) -> QVariantList {
        self.friends_storage
            .iter()
            .map(|item| unsafe { (&*item.borrow_mut() as &dyn QObject).as_qvariant() })
            .collect()
    }

    fn get_blocked_users(&mut self) -> QVariantList {
        self.blocked_users_storage
            .values()
            .map(|item| item.to_qvariant())
            .collect()
    }

    fn reset_friend_keys(&mut self) {
        let (chat_map, user_map) = self.friends_storage
            .iter()
            .enumerate()
            .map(|(i, friend)| {
                let friend = friend.borrow_mut();
                ((friend.chat_id().into(), i), (friend.user_id().into(), i))
            })
            .unzip();

        self.friend_by_chat_handle = chat_map;
        self.friend_by_user_handle = user_map;
    }
}
