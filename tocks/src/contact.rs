use crate::storage::{ChatHandle, UserHandle};

use std::collections::HashMap;
use toxcore::{Friend as ToxFriend, PublicKey, Status};

/// Data associated with a tox friend
#[derive(Clone, Debug)]
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
            status
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

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status
    }
}

pub type Friends = HashMap<UserHandle, Friend>;
pub type ToxFriends = HashMap<ChatHandle, toxcore::Friend>;

#[derive(Default)]
pub(crate) struct UserManager {
    // Map chat handle, user handle, public
    chat_mapping: HashMap<ChatHandle, usize>,
    pk_mapping: HashMap<PublicKey, usize>,
    friends: Vec<(Friend, toxcore::Friend)>,
}

impl UserManager {
    pub fn new() -> UserManager {
        Default::default()
    }

    pub fn add_friend(&mut self, friend: Friend, tox_friend: ToxFriend) -> (&Friend, &ToxFriend) {
        assert!(*friend.public_key() == tox_friend.public_key());

        let idx = self.friends.len();

        self.chat_mapping.insert(*friend.chat_handle(), idx);
        self.pk_mapping.insert(friend.public_key().clone(), idx);
        self.friends.push((friend, tox_friend));

        let last = self.friends.last().unwrap();

        (&last.0, &last.1)
    }

    pub fn tox_friend_by_chat_handle(&self, handle: &ChatHandle) -> &ToxFriend {
        &self.friends[self.chat_mapping[handle]].1
    }

    pub fn friend_by_public_key(&mut self, key: &PublicKey) -> &mut Friend {
        &mut self.friends[self.pk_mapping[key]].0
    }

    pub fn friends(&self) -> impl Iterator<Item = &Friend> {
        self.friends.iter().map(|item| &item.0)
    }
}
