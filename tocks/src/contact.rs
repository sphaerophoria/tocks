use toxcore::PublicKey;

/// Data associated with a tox friend
#[derive(Clone, Debug)]
pub struct FriendData {
    public_key: PublicKey,
    name: String,
}

impl FriendData {
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl From<&toxcore::Friend> for FriendData {
    fn from(friend: &toxcore::Friend) -> Self {
        let public_key = friend.public_key();
        let name = friend.name();

        FriendData { public_key, name }
    }
}
