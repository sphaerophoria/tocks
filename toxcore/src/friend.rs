use crate::{FriendData, PublicKey, Status};

use std::{
    hash::{Hash, Hasher},
    sync::{Arc, RwLock, RwLockReadGuard},
};

/// Information related to a tox friend
#[derive(Clone, Debug)]
pub struct Friend {
    // FIXME: on friend removal outstanding friend handles are invalid. As of
    // right now we only have one consumer who we know will handle it right, but
    // we should validate that the friend is still valid when the ID is used
    pub(crate) id: u32,
    pub(crate) data: Arc<RwLock<FriendData>>,
}

impl Hash for Friend {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for Friend {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Friend {}

impl Friend {
    /// Retrieves the friend's public key
    pub fn public_key(&self) -> PublicKey {
        self.lock_data().public_key.clone()
    }

    /// Retrieves the friend's advertised name
    pub fn name(&self) -> String {
        self.lock_data().name.clone()
    }

    pub fn status(&self) -> Status {
        self.lock_data().status
    }

    fn lock_data(&self) -> RwLockReadGuard<'_, FriendData> {
        self.data.read().expect("Lock poisoned")
    }
}
