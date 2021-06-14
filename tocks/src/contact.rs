use crate::storage::{ChatHandle, UserHandle};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use toxcore::{Friend as ToxFriend, PublicKey, Status as ToxStatus};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Online,
    Away,
    Busy,
    Offline,
    Pending,
}

impl From<ToxStatus> for Status {
    fn from(status: ToxStatus) -> Status {
        match status {
            ToxStatus::Online => Status::Online,
            ToxStatus::Away => Status::Away,
            ToxStatus::Busy => Status::Busy,
            ToxStatus::Offline => Status::Offline,
        }
    }
}

/// Data associated with a tox friend
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Friend {
    id: UserHandle,
    chat_handle: ChatHandle,
    public_key: PublicKey,
    name: String,
    status: Status,
}

impl Friend {
    pub fn new(
        id: UserHandle,
        chat_handle: ChatHandle,
        public_key: PublicKey,
        name: String,
        status: Status,
    ) -> Friend {
        Friend {
            id,
            chat_handle,
            public_key,
            name,
            status,
        }
    }

    pub fn id(&self) -> &UserHandle {
        &self.id
    }

    pub fn chat_handle(&self) -> &ChatHandle {
        &self.chat_handle
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status
    }
}

pub type Friends = HashMap<UserHandle, Friend>;
pub type ToxFriends = HashMap<ChatHandle, toxcore::Friend>;

pub struct FriendBundle {
    pub friend: Friend,
    pub tox_friend: Option<toxcore::Friend>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    id: UserHandle,
    public_key: PublicKey,
    name: String,
}

impl User {
    pub fn new(id: UserHandle, public_key: PublicKey, name: String) -> User {
        User {
            id,
            public_key,
            name,
        }
    }

    pub fn id(&self) -> &UserHandle {
        &self.id
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Default)]
pub(crate) struct UserManager {
    // Map chat handle, user handle, public
    chat_mapping: HashMap<ChatHandle, usize>,
    user_mapping: HashMap<UserHandle, usize>,
    pk_mapping: HashMap<PublicKey, usize>,
    friends: Vec<FriendBundle>,
}

impl UserManager {
    pub fn new() -> UserManager {
        Default::default()
    }

    pub fn add_friend(&mut self, friend: Friend, tox_friend: ToxFriend) -> &FriendBundle {
        assert!(*friend.public_key() == tox_friend.public_key());

        let tox_friend = Some(tox_friend);

        let idx = self.friends.len();

        self.chat_mapping.insert(*friend.chat_handle(), idx);
        self.user_mapping.insert(*friend.id(), idx);
        self.pk_mapping.insert(friend.public_key().clone(), idx);
        self.friends.push(FriendBundle { friend, tox_friend });

        self.friends.last().unwrap()
    }

    pub fn add_pending_friend(&mut self, friend: Friend) -> &Friend {
        let idx = self.friends.len();

        self.chat_mapping.insert(*friend.chat_handle(), idx);
        self.user_mapping.insert(*friend.id(), idx);
        self.pk_mapping.insert(friend.public_key().clone(), idx);
        self.friends.push(FriendBundle {
            friend,
            tox_friend: None,
        });

        &self.friends.last().unwrap().friend
    }

    pub fn friend_by_chat_handle(&self, handle: &ChatHandle) -> &FriendBundle {
        &self.friends[self.chat_mapping[handle]]
    }

    pub fn friend_by_public_key(&mut self, key: &PublicKey) -> &mut Friend {
        &mut self.friends[self.pk_mapping[key]].friend
    }

    pub fn friend_by_user_handle(&mut self, handle: &UserHandle) -> &mut FriendBundle {
        &mut self.friends[self.user_mapping[handle]]
    }

    pub fn friends(&self) -> impl Iterator<Item = &Friend> {
        self.friends.iter().map(|item| &item.friend)
    }
}
