use crate::{
    builder::ToxBuilder, error::*, Friend, FriendData, FriendRequest, Message, PublicKey, Receipt,
    SecretKey, ToxId,
};

use log::error;

use toxcore_sys::*;

use crate::sys::{ToxApi, ToxApiImpl};

use paste::paste;

use tokio::{sync::broadcast, time};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

macro_rules! impl_self_key_getter {
    ($name:ident, $result_type:ty) => {
        paste! {
            pub fn [<self_ $name>](&self) -> $result_type {
                unsafe {
                    let size = self.api.[<$name _size>]() as usize;

                    let mut ret = Vec::with_capacity(size);
                    self.api.[<self_get_ $name>](self.sys_tox.get(), ret.as_mut_ptr());
                    ret.set_len(size);
                    $result_type {
                        key: ret
                    }
                }
            }
        }
    };
}

/// A tox account
///
/// Run the tox instance. This needs to be running for anything related to
/// this tox instance to happen.
///
/// Note: If this function is just stopped this allows you to effectively "go
/// offline" while still maintaining all related data
///
/// # Example
///
/// An example of how to use in combination with the tokio runtime
///
/// ```
/// # use toxcore::Tox;
/// # async fn run_tox() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a tox instance
/// let mut tox = Tox::builder()?.build()?;
/// // Set up handlers
/// let mut friend_requests = tox.friend_requests();
/// let friend_request_handler = async {
///     while let Ok(request) = friend_requests.recv().await {
///         // Do whatever you want in response to the friend request
///     }
/// };
///
/// // Start the main toxcore loop
/// tokio::join!(tox.run(), friend_request_handler);
/// # Ok(())
/// # }
/// ```
pub struct Tox {
    inner: ToxImpl<ToxApiImpl>,
}

impl Tox {
    pub(crate) fn new(inner: ToxImpl<ToxApiImpl>) -> Tox {
        Tox { inner }
    }

    pub fn builder() -> Result<ToxBuilder, ToxBuilderCreationError> {
        ToxBuilder::new()
    }

    /// Run the tox instance. This needs to be running for anything related to
    /// this tox instance to happen.
    ///
    /// Note: If this function is just stopped this allows you to effectively "go
    /// offline" while still maintaining all related data
    pub async fn run(&mut self) {
        self.inner.run().await
    }

    pub fn self_secret_key(&self) -> SecretKey {
        self.inner.self_secret_key()
    }

    pub fn self_public_key(&self) -> PublicKey {
        self.inner.self_public_key()
    }

    pub fn self_address(&self) -> ToxId {
        self.inner.self_address()
    }

    pub fn self_name(&self) -> String {
        self.inner.self_name()
    }

    /// Retrieves all added toxcore friends
    pub fn friends(&mut self) -> Result<Vec<Friend>, ToxFriendError> {
        self.inner.friends()
    }

    /// Stream of incoming friend requests
    pub fn friend_requests(&self) -> broadcast::Receiver<FriendRequest> {
        self.inner.friend_requests()
    }

    /// Adds a friend without issuing a friend request. This can be called in
    /// response to a friend request, or if two users agree to add eachother via
    /// a different channel
    pub fn add_friend_norequest(
        &mut self,
        public_key: &PublicKey,
    ) -> Result<Friend, ToxFriendError> {
        self.inner.add_friend_norequest(public_key)
    }

    pub fn incoming_friend_messages(&mut self, friend: &Friend) -> broadcast::Receiver<Message> {
        self.inner.incoming_friend_messages(friend)
    }

    pub fn send_message(
        &mut self,
        friend: &Friend,
        message: &Message,
    ) -> Result<Receipt, ToxSendMessageError> {
        self.inner.send_message(friend, message)
    }
}

/// Wrapper struct to help us manage mutability of the interior tox pointer
struct SysToxMutabilityWrapper {
    sys_tox: *mut toxcore_sys::Tox,
}

impl SysToxMutabilityWrapper {
    fn get(&self) -> *const toxcore_sys::Tox {
        self.sys_tox
    }

    fn get_mut(&mut self) -> *mut toxcore_sys::Tox {
        self.sys_tox
    }
}

/// Stored data separate from the toxcore api itself. This needs to be separated
/// so we can use it as the toxcore userdata pointer without breaking any
/// mutability rules
struct ToxData {
    friend_request_tx: broadcast::Sender<FriendRequest>,
    friend_message_tx: HashMap<u32, broadcast::Sender<Message>>,
    friend_data: HashMap<u32, Arc<RwLock<FriendData>>>,
}

struct CallbackData<'a, Api: ToxApi> {
    api: &'a Api,
    data: &'a mut ToxData,
}

/// Generic implementation of [`Tox`]. Abstracted this way to allow for
/// testing/mocking without exposing generics to API consumers
pub(crate) struct ToxImpl<Api: ToxApi> {
    api: Api,
    sys_tox: SysToxMutabilityWrapper,
    data: ToxData,
}

// toxcore claims that it is safe to use the const APIs from multiple threads.
// As long as it isn't casting out the const anywhere under the hood I don't see
// why we can't trust it. That means that we implement both Send + Sync. Rust's
// mutability rules will prevent us from modifying the interior tox state while
// reading from it
unsafe impl<Api: ToxApi> Send for ToxImpl<Api> {}
unsafe impl<Api: ToxApi> Sync for ToxImpl<Api> {}

impl<Api: ToxApi> ToxImpl<Api> {
    pub(crate) fn new(api: Api, sys_tox: *mut toxcore_sys::Tox) -> ToxImpl<Api> {
        // FIXME: is 100 a sane size here?
        let (friend_request_tx, _) = broadcast::channel(100);

        unsafe {
            api.callback_friend_request(sys_tox, Some(tox_friend_request_callback::<Api>));
            api.callback_friend_message(sys_tox, Some(tox_friend_message_callback::<Api>));
        }

        ToxImpl {
            api: api,
            sys_tox: SysToxMutabilityWrapper { sys_tox },
            data: ToxData {
                friend_request_tx,
                friend_message_tx: HashMap::new(),
                friend_data: HashMap::new(),
            },
        }
    }

    pub async fn run(&mut self) {
        unsafe {
            let mut sleep_interval = None;

            loop {
                {
                    let sys_tox = self.sys_tox.get_mut();

                    let mut callback_data = CallbackData {
                        api: &self.api,
                        data: &mut self.data,
                    };

                    self.api.iterate(
                        sys_tox,
                        (&mut callback_data as *mut CallbackData<Api>) as *mut std::os::raw::c_void,
                    );

                    if sleep_interval.is_none() {
                        sleep_interval =
                            Some(self.api.iteration_interval(self.sys_tox.get()) as u64);
                    }
                }

                time::sleep(std::time::Duration::from_millis(sleep_interval.unwrap())).await;
            }
        }
    }

    impl_self_key_getter!(public_key, PublicKey);
    impl_self_key_getter!(secret_key, SecretKey);
    impl_self_key_getter!(address, ToxId);

    pub fn self_name(&self) -> String {
        unsafe {
            let length = self.api.self_get_name_size(self.sys_tox.get()) as usize;

            let mut name_unparsed = Vec::with_capacity(length);
            self.api
                .self_get_name(self.sys_tox.get(), name_unparsed.as_mut_ptr());
            name_unparsed.set_len(length);

            String::from_utf8_lossy(&name_unparsed).to_string()
        }
    }

    pub fn friends(&mut self) -> Result<Vec<Friend>, ToxFriendError> {
        unsafe {
            let friend_indexes = {
                let length = self.api.self_get_friend_list_size(self.sys_tox.get()) as usize;

                let mut friend_indexes = Vec::with_capacity(length);
                self.api
                    .self_get_friend_list(self.sys_tox.get(), friend_indexes.as_mut_ptr());
                friend_indexes.set_len(length);

                friend_indexes
            };

            let mut ret = Vec::new();
            for index in friend_indexes {
                ret.push(self.friend_from_id(index)?);
            }

            Ok(ret)
        }
    }

    pub fn friend_requests(&self) -> broadcast::Receiver<FriendRequest> {
        self.data.friend_request_tx.subscribe()
    }

    pub fn add_friend_norequest(
        &mut self,
        public_key: &PublicKey,
    ) -> Result<Friend, ToxFriendError> {
        unsafe {
            let mut err: TOX_ERR_FRIEND_ADD = TOX_ERR_FRIEND_ADD_OK;

            let friend_num = {
                if public_key.key.len() != self.api.public_key_size() as usize {
                    return Err(ToxFriendError::InvalidKey);
                }

                self.api.friend_add_norequest(
                    self.sys_tox.get_mut(),
                    public_key.key.as_ptr(),
                    &mut err as *mut TOX_ERR_FRIEND_ADD,
                )
            };

            match err {
                TOX_ERR_FRIEND_ADD_OK => (),
                TOX_ERR_FRIEND_ADD_NULL => return Err(ToxFriendError::NullArgument),
                TOX_ERR_FRIEND_ADD_TOO_LONG => return Err(ToxFriendError::MessageTooLong),
                TOX_ERR_FRIEND_ADD_NO_MESSAGE => return Err(ToxFriendError::MessageEmpty),
                TOX_ERR_FRIEND_ADD_OWN_KEY => return Err(ToxFriendError::AddSelf),
                TOX_ERR_FRIEND_ADD_ALREADY_SENT => return Err(ToxFriendError::AlreadySent),
                TOX_ERR_FRIEND_ADD_BAD_CHECKSUM => return Err(ToxFriendError::BadChecksum),
                TOX_ERR_FRIEND_ADD_SET_NEW_NOSPAM => return Err(ToxFriendError::NewNospam),
                TOX_ERR_FRIEND_ADD_MALLOC => return Err(ToxFriendError::Malloc),
                _ => return Err(ToxFriendError::Unknown),
            }

            self.friend_from_id(friend_num)
        }
    }

    pub fn incoming_friend_messages(&mut self, friend: &Friend) -> broadcast::Receiver<Message> {
        let message_tx = self
            .data
            .friend_message_tx
            .entry(friend.id)
            .or_insert_with(|| {
                let channel = broadcast::channel(100);
                channel.0
            });

        message_tx.subscribe()
    }

    pub fn send_message(
        &mut self,
        friend: &Friend,
        message: &Message,
    ) -> Result<Receipt, ToxSendMessageError> {
        let (t, ptr, len) = match message {
            Message::Action(s) => (TOX_MESSAGE_TYPE_ACTION, s.as_ptr(), s.len()),
            Message::Normal(s) => (TOX_MESSAGE_TYPE_NORMAL, s.as_ptr(), s.len()),
        };

        let mut err = TOX_ERR_FRIEND_SEND_MESSAGE_OK;

        let receipt_id = unsafe {
            self.api.friend_send_message(
                self.sys_tox.get_mut(),
                friend.id,
                t,
                ptr,
                len as u64,
                &mut err,
            )
        };

        match err {
            TOX_ERR_FRIEND_SEND_MESSAGE_OK => Ok(Receipt { id: receipt_id }),
            TOX_ERR_FRIEND_SEND_MESSAGE_NULL => Err(ToxSendMessageError::InvalidArgument),
            TOX_ERR_FRIEND_SEND_MESSAGE_FRIEND_NOT_FOUND => {
                Err(ToxSendMessageError::InvalidFriendId)
            }
            TOX_ERR_FRIEND_SEND_MESSAGE_FRIEND_NOT_CONNECTED => {
                Err(ToxSendMessageError::NotConnected)
            }
            TOX_ERR_FRIEND_SEND_MESSAGE_SENDQ => Err(ToxSendMessageError::InternalError),
            TOX_ERR_FRIEND_SEND_MESSAGE_TOO_LONG => Err(ToxSendMessageError::MessageTooLong),
            TOX_ERR_FRIEND_SEND_MESSAGE_EMPTY => Err(ToxSendMessageError::MessageEmpty),
            _ => Err(ToxSendMessageError::Unknown),
        }
    }

    /// Calls into toxcore to get the public key for the provided friend id
    fn public_key_from_id(&self, id: u32) -> Result<PublicKey, ToxFriendError> {
        unsafe {
            let length = self.api.public_key_size() as usize;
            let mut public_key = Vec::with_capacity(length);
            let success = self.api.friend_get_public_key(
                self.sys_tox.get(),
                id,
                public_key.as_mut_ptr(),
                std::ptr::null_mut(),
            );
            public_key.set_len(length);

            if !success {
                return Err(ToxFriendError::PublicKey);
            }

            Ok(PublicKey { key: public_key })
        }
    }

    /// Calls into toxcore to get the name for the provided friend id
    fn name_from_id(&self, id: u32) -> Result<String, ToxFriendError> {
        unsafe {
            let mut err = TOX_ERR_FRIEND_QUERY_OK;

            let length = self.api.friend_get_name_size(
                self.sys_tox.get(),
                id,
                &mut err as *mut TOX_ERR_FRIEND_QUERY,
            ) as usize;
            let mut name = Vec::with_capacity(length);
            // FIXME: handle the errors gracefully
            let success = self.api.friend_get_name(
                self.sys_tox.get(),
                id,
                name.as_mut_ptr(),
                std::ptr::null_mut(),
            );
            name.set_len(length);
            Ok(String::from_utf8_lossy(&name).to_string())
        }
    }

    /// Creates a [`Friend`], populating the data in [`ToxData::friend_data`] if necessary.
    ///
    /// If [`ToxData::friend_data`] already exists the data in it will be overwritten
    fn friend_from_id(&mut self, id: u32) -> Result<Friend, ToxFriendError> {
        // If it exists we have to update the existing fields, otherwise we have to create with correct fields, either way we need to get the fields

        if let Some(existing_data) = self.data.friend_data.get(&id) {
            Ok(Friend {
                id,
                data: Arc::clone(existing_data),
            })
        } else {
            let public_key = self.public_key_from_id(id)?;
            let name = self.name_from_id(id)?;

            let friend_data = FriendData { public_key, name };

            let friend_data = Arc::new(RwLock::new(friend_data));
            self.data.friend_data.insert(id, Arc::clone(&friend_data));

            Ok(Friend {
                id,
                data: friend_data,
            })
        }
    }
}

impl<Api: ToxApi> Drop for ToxImpl<Api> {
    fn drop(&mut self) {
        unsafe { self.api.kill(self.sys_tox.get_mut()) }
    }
}

/// Callback function provided to toxcore for incoming friend requests
///
/// Messages wil be forwarded to [`ToxData::friend_request_tx`]
pub(crate) unsafe extern "C" fn tox_friend_request_callback<Api: ToxApi>(
    _sys_tox: *mut toxcore_sys::Tox,
    input_public_key: *const u8,
    input_message: *const u8,
    length: u64,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &*(user_data as *mut CallbackData<Api>);

    let public_key_length = tox_data.api.public_key_size() as usize;

    let mut public_key_storage = Vec::with_capacity(public_key_length);
    std::ptr::copy_nonoverlapping(
        input_public_key,
        public_key_storage.as_mut_ptr(),
        public_key_length,
    );
    public_key_storage.set_len(public_key_length);
    let public_key = PublicKey {
        key: public_key_storage,
    };

    let mut message = Vec::with_capacity(length as usize);
    std::ptr::copy_nonoverlapping(input_message, message.as_mut_ptr(), length as usize);
    message.set_len(length as usize);

    let message = match String::from_utf8(message) {
        Ok(s) => s,
        Err(_) => {
            error!("Failed to parse friend request message");
            return;
        }
    };

    let request = FriendRequest {
        public_key,
        message,
    };

    let friend_request_tx = &tox_data.data.friend_request_tx;

    if let Err(e) = friend_request_tx.send(request) {
        error!("Failed to propagate friend request: {}", e);
    }
}

/// Callback function provided to toxcore for incoming messages.
///
/// Messages will be forwarded to the appropriate [`FriendData::message_received_tx`]
pub(crate) unsafe extern "C" fn tox_friend_message_callback<Api: ToxApi>(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    message_type: TOX_MESSAGE_TYPE,
    message: *const u8,
    length: u64,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &*(user_data as *mut CallbackData<Api>);

    let message_content =
        String::from_utf8_lossy(std::slice::from_raw_parts(message, length as usize)).to_string();

    let message = match message_type {
        TOX_MESSAGE_TYPE_ACTION => Message::Action(message_content),
        TOX_MESSAGE_TYPE_NORMAL => Message::Normal(message_content),
        _ => {
            error!("Failed to parse message type");
            return;
        }
    };

    let message_sender = tox_data.data.friend_message_tx.get(&friend_number);

    if let Some(message_sender) = message_sender {
        if let Err(e) = message_sender.send(message) {
            error!("Failed to propagate incoming message: {}", e);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sys::MockToxApi as MockSysToxApi;
    use std::sync::atomic::AtomicU64;

    pub(crate) struct ToxFixture {
        tox: ToxImpl<MockSysToxApi>,
        pk_len: usize,
        default_peer_pk: PublicKey,
        default_peer_id: u32,
        default_peer_name: String,
    }

    impl ToxFixture {
        pub(crate) fn new(mut mock: MockSysToxApi) -> ToxFixture {
            let default_peer_pk = PublicKey {
                key: "testkey".to_string().into_bytes(),
            };

            let default_peer_id = 10u32;

            let default_peer_name = "TestPeer";

            mock.expect_callback_friend_request().return_const(()).once();

            mock.expect_callback_friend_message().return_const(()).once();

            mock.expect_kill().return_const(());

            mock.expect_public_key_size()
                .return_const(default_peer_pk.key.len() as u32);

            let default_peer_pk_clone = default_peer_pk.clone();
            mock.expect_friend_get_public_key()
                .withf_st(move |_, id, _ptr, _err| *id == default_peer_id)
                .returning_st(move |_, _id, ptr, _err| {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            default_peer_pk.key.as_ptr(),
                            ptr,
                            default_peer_pk.key.len(),
                        )
                    };
                    true
                });

            mock.expect_friend_get_name_size()
                .withf_st(move |_, id, _err| *id == default_peer_id)
                .return_const(default_peer_name.len() as u32);

            mock.expect_friend_get_name()
                .withf_st(move |_, id, _name, _err| *id == default_peer_id)
                .returning_st(move |_, _id, name, _err| {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            default_peer_name.as_ptr(),
                            name,
                            default_peer_name.len(),
                        )
                    };
                    true
                });

            let tox = ToxImpl::new(mock, std::ptr::null_mut());

            ToxFixture {
                tox,
                pk_len: default_peer_pk_clone.key.len(),
                default_peer_pk: default_peer_pk_clone,
                default_peer_id,
                default_peer_name: default_peer_name.to_string(),
            }
        }
    }

    #[tokio::test]
    async fn test_iteration() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockSysToxApi::default();

        const ITERATION_INTERVAL: u32 = 20;

        mock.expect_iteration_interval()
            .return_const(ITERATION_INTERVAL);

        use std::sync::atomic::Ordering;
        let iterations = Arc::new(AtomicU64::new(0));
        let closure_iterations = Arc::clone(&iterations);

        mock.expect_iterate().returning_st(move |_, _| {
            closure_iterations.store(
                closure_iterations.load(Ordering::Relaxed) + 1u64,
                Ordering::Relaxed,
            );
        });

        const NUM_ITERATIONS: u64 = 50;

        let cancel_future = async {
            time::sleep(std::time::Duration::from_millis(
                NUM_ITERATIONS * ITERATION_INTERVAL as u64,
            ))
            .await
        };

        let mut fixture = ToxFixture::new(mock);

        tokio::select! {
            _ = fixture.tox.run() => { }
            _ = cancel_future => { }
        };

        // toxcore asks us to sleep for iteration_interval, we can have some
        // leeway from what they request since toxcore_iterate will naturally
        // take some time
        assert!(iterations.load(Ordering::Relaxed) > NUM_ITERATIONS * 4 / 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_friend_request_dispatch() -> Result<(), Box<dyn std::error::Error>> {
        let mock = MockSysToxApi::default();

        let message_str = "message";
        let message = message_str.to_string().into_bytes();

        let mut fixture = ToxFixture::new(mock);

        let mut friend_requests = fixture.tox.friend_requests();
        let default_peer_pk = fixture.default_peer_pk.clone();
        let confirm_friend_request = async {
            let friend_request = friend_requests.recv().await.unwrap();
            assert_eq!(friend_request.message, message_str);
            assert_eq!(friend_request.public_key, default_peer_pk);
        };

        let mut callback_data = CallbackData {
            api: &fixture.tox.api,
            data: &mut fixture.tox.data,
        };

        unsafe {
            tox_friend_request_callback::<MockSysToxApi>(
                std::ptr::null_mut(),
                fixture.default_peer_pk.key.as_ptr(),
                message.as_ptr(),
                message.len() as u64,
                (&mut callback_data as *mut CallbackData<MockSysToxApi>)
                    as *mut std::os::raw::c_void,
            );
        }

        confirm_friend_request.await;

        Ok(())
    }

    macro_rules! test_array_getter {
        ($name:ident, $value:expr) => {
            paste! {
                #[test]
                fn [<test_self_ $name>]() -> Result<(), Box<dyn std::error::Error>> {
                    let mut mock = MockSysToxApi::default();


                    let key = $value.chars().map(|c| c as u8).collect::<Vec<u8>>();

                    mock.[<expect_ $name _size>]()
                        .return_const(key.len() as u32)
                        .once();

                    let key_clone = key.clone();

                    mock.[<expect_self_get_ $name>]()
                        .return_const(())
                        .returning_st(move |_, output_key| {
                            unsafe {
                                std::ptr::copy_nonoverlapping(key_clone.as_ptr(), output_key, key_clone.len());
                            }
                        });

                    let fixture = ToxFixture::new(mock);

                    let retrieved_key = fixture.tox.[<self_ $name>]().key;
                    assert_eq!(retrieved_key, key);

                    Ok(())
                }

            }
        }
    }

    test_array_getter!(public_key, "public_key");
    test_array_getter!(secret_key, "secret_key");
    test_array_getter!(address, "address");

    #[test]
    fn test_get_self_name() {
        let mut mock = MockSysToxApi::default();

        let self_name = "TestName";

        mock.expect_self_get_name_size()
            .return_const(self_name.len() as u64);

        mock.expect_self_get_name()
            .returning_st(move |_, name_out| {
                unsafe {std::ptr::copy_nonoverlapping(self_name.as_ptr(), name_out, self_name.len())}
            });

        let fixture = ToxFixture::new(mock);

        assert_eq!(fixture.tox.self_name(), self_name);
    }

    #[test]
    fn friend_retrieval() {
        let mut mock = MockSysToxApi::default();

        // Set up fake friends list with 3 items
        mock.expect_self_get_friend_list_size().return_const(3 as u32);
        mock.expect_self_get_friend_list()
            .returning_st(|_, output_list| {
                unsafe {
                    *output_list = 1;
                    *output_list.offset(1) = 2;
                    *output_list.offset(2) = 3;
                }
            });

        // mocked friend PKs will only be 3 long, "pk1", "pk2", "pk3"
        mock.expect_public_key_size().return_const(3 as u32);
        mock.expect_friend_get_public_key()
            .withf_st(|_, id, _output, _error| {
                *id == 1u32 || *id == 2u32 || *id == 3u32
            })
            .returning_st(|_, id, output, _error| {
                unsafe {
                    let key = format!("pk{}", id);
                    std::ptr::copy_nonoverlapping(key.as_ptr(), output, key.len())
                }
                true
            })
            .times(3);

        mock.expect_friend_get_name_size().return_const(5 as u32);
        mock.expect_friend_get_name()
            .withf_st(|_, id, _output, _error| {
                *id == 1u32 || *id == 2u32 || *id == 3u32
            })
            .returning_st(|_, id, output, _error| {
                unsafe {
                    let name = format!("name{}", id);
                    std::ptr::copy_nonoverlapping(name.as_ptr(), output, name.len())
                }
                true
            })
            .times(3);

        let mut fixture = ToxFixture::new(mock);

        let friends = fixture.tox.friends().unwrap();

        let friend_matches_id = |friend: &Friend, id: u32| {
            friend.name() == format!("name{}", id) &&
            friend.public_key().key == format!("pk{}", id).into_bytes()
        };

        assert_eq!(friends.len(), 3);
        assert!(friends.iter().find(|item| friend_matches_id(item, 1)).is_some());
        assert!(friends.iter().find(|item| friend_matches_id(item, 2)).is_some());
        assert!(friends.iter().find(|item| friend_matches_id(item, 3)).is_some());
    }

    #[test]
    fn test_friend_retrieval_name_failure()
    {
        let mut mock = MockSysToxApi::default();
        mock.expect_friend_get_name_size()
            .withf_st(|_, id, _err| *id == 0u32)
            .return_const(10 as u64)
            .once();

        mock.expect_friend_get_name()
            .withf_st(|_, id, _output, _err| *id == 0u32)
            .returning_st(|_, _id, _output, err| {
                unsafe { *err = TOX_ERR_FRIEND_QUERY_NULL; }
                return false;
            })
            .once();

        // Expect a second call where we fail on retrieval of the name size
        // instead of the name itself
        mock.expect_friend_get_name_size()
            .withf_st(|_, id, _err| *id == 0u32)
            .returning_st(|_, _id, err| {
                unsafe { *err = TOX_ERR_FRIEND_QUERY_NULL; }
                99348
            })
            .once();


        let fixture = ToxFixture::new(mock);
        assert!(fixture.tox.name_from_id(0).is_err());
        assert!(fixture.tox.name_from_id(0).is_err());
    }

    #[test]
    fn test_friend_retrieval_pk_failure()
    {
        let mut mock = MockSysToxApi::default();
        mock.expect_friend_get_public_key()
            .withf_st(|_, id, _output, _err| *id == 0u32)
            .returning_st(|_, _id, _output, _err| {
                // NOTE: at the time of writing the caller passes in a null err
                // pointer and relies on the return value
                return false;
            });

        let fixture = ToxFixture::new(mock);
        assert!(fixture.tox.public_key_from_id(0).is_err());
    }

    #[test]
    fn test_add_friend_norequest() -> Result<(), Box<dyn std::error::Error>> {
        let mock = MockSysToxApi::default();
        let mut fixture = ToxFixture::new(mock);

        let peer_pk = fixture.default_peer_pk.clone();
        let pk_len = fixture.pk_len;

        fixture
            .tox
            .api
            .expect_friend_add_norequest()
            .withf_st(move |_, input_public_key, _err| {
                let slice = unsafe { std::slice::from_raw_parts(*input_public_key, pk_len) };
                slice == peer_pk.key
            })
            .return_const(fixture.default_peer_id)
            .once();

        let friend = fixture.tox.add_friend_norequest(&fixture.default_peer_pk)?;
        assert_eq!(friend.id, fixture.default_peer_id);
        assert_eq!(friend.public_key(), fixture.default_peer_pk);
        assert_eq!(friend.name(), fixture.default_peer_name);

        Ok(())
    }

    #[test]
    fn test_add_friend_norequest_invalid_pk() -> Result<(), Box<dyn std::error::Error>> {
        let mut fixture = ToxFixture::new(MockSysToxApi::default());

        // Test that invalid keys are not passed on
        let public_key = &fixture.default_peer_pk;

        let bad_pk1 = PublicKey {
            key: Vec::from(&public_key.key[..public_key.key.len() - 1]),
        };
        let bad_pk2 = PublicKey {
            key: Vec::from(&public_key.key[0..0]),
        };

        assert!(fixture.tox.add_friend_norequest(&bad_pk1).is_err());
        assert!(fixture.tox.add_friend_norequest(&bad_pk2).is_err());

        Ok(())
    }

    #[test]
    fn test_add_friend_norequest_failure() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockSysToxApi::default();

        // Test toxcore failure triggers a failure for us
        mock.expect_friend_add_norequest()
            .returning_st(move |_, _, err| {
                unsafe {
                    *err = TOX_ERR_FRIEND_ADD_NO_MESSAGE;
                }
                u32::MAX
            })
            .once();

        let mut fixture = ToxFixture::new(mock);

        assert!(fixture
            .tox
            .add_friend_norequest(&fixture.default_peer_pk)
            .is_err());

        Ok(())
    }
}
