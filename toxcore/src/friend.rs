use crate::{FriendData, PublicKey};

use std::sync::{Arc, RwLock, RwLockReadGuard};

/// Information related to a tox friend
#[derive(Debug)]
pub struct Friend {
    pub(crate) id: u32,
    pub(crate) data: Arc<RwLock<FriendData>>,
}

impl Friend {
    /// Retrieves the friend's public key
    pub fn public_key(&self) -> PublicKey {
        self.lock_data().public_key.clone()
    }

    /// Retrieves the friend's advertised name
    pub fn name(&self) -> String {
        self.lock_data().name.clone()
    }

    fn lock_data(&self) -> RwLockReadGuard<'_, FriendData> {
        self.data.read().expect("Lock poisoned")
    }
}
