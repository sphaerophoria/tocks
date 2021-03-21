use crate::storage::{ChatHandle, UserHandle};

use toxcore::PublicKey;

/// Data associated with a tox friend
#[derive(Clone, Debug)]
pub struct Friend {
    id: UserHandle,
    chat_handle: ChatHandle,
    public_key: PublicKey,
    name: String,
}

impl Friend {
    pub fn new(
        id: UserHandle,
        chat_handle: ChatHandle,
        public_key: PublicKey,
        name: String,
    ) -> Friend {
        Friend {
            id,
            chat_handle,
            public_key,
            name,
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
}

pub struct UserManager {
    // Map chat handle, user handle, public
}
