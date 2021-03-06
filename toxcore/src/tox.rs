use crate::{
    av::{ActiveCall, AudioFrame, CallControl, CallData, CallEvent, CallState, IncomingCall},
    builder::ToxBuilder,
    error::*,
    sys, Event, Friend, FriendData, FriendRequest, Message, PublicKey, Receipt, SecretKey, Status,
    ToxId,
};

use toxcore_sys::*;

use log::{error, warn};
use paste::paste;

use tokio::time;

use futures::{
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    prelude::*,
};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    pin::Pin,
};

macro_rules! impl_self_key_getter {
    ($name:ident, $result_type:ty) => {
        paste! {
            pub fn [<self_ $name>](&self) -> $result_type {
                unsafe {
                    let size = sys::[<tox_ $name _size>]() as usize;

                    let mut ret = Vec::with_capacity(size);
                    sys::[<tox_self_get_ $name>](self.sys_tox.get(), ret.as_mut_ptr());
                    ret.set_len(size);
                    $result_type {
                        key: ret
                    }
                }
            }
        }
    };
}

pub type ToxEventCallback = Box<dyn FnMut(Event)>;

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
/// let mut tox = Tox::builder()?
///    // Setup handlers
///    .event_callback(|event| {
///        // Do what you want in response to the event
///    })
///    .build()?;
///
/// // Start the main toxcore loop
/// tox.run().await;
/// # Ok(())
/// # }
/// ```
pub struct Tox {
    sys_tox: SysToxMutabilityWrapper,
    next_tox: time::Instant,
    av: ToxAvMutabilityWrapper,
    next_av: time::Instant,
    data: Pin<Box<ToxData>>,
}

impl Tox {
    pub fn builder() -> Result<ToxBuilder, ToxBuilderCreationError> {
        ToxBuilder::new()
    }

    pub(crate) fn new(
        sys_tox: *mut toxcore_sys::Tox,
        av: *mut toxcore_sys::ToxAV,
        event_callback: Option<ToxEventCallback>,
    ) -> Tox {
        // FIXME: friends should be initialized here and only accessed later,
        // initializing during a call to retrieve the friends seems a little
        // strange

        let mut tox = Tox {
            sys_tox: SysToxMutabilityWrapper::new(sys_tox),
            next_tox: time::Instant::now(),
            av: ToxAvMutabilityWrapper::new(av),
            next_av: time::Instant::now(),
            data: Pin::new(Box::new(ToxData {
                event_callback,
                friend_data: HashMap::new(),
                call_data: HashMap::new(),
            })),
        };

        unsafe {
            sys::tox_callback_friend_request(sys_tox, Some(tox_friend_request_callback));
            sys::tox_callback_friend_message(sys_tox, Some(tox_friend_message_callback));
            sys::tox_callback_friend_read_receipt(sys_tox, Some(tox_friend_read_receipt_callback));
            sys::tox_callback_friend_status(sys_tox, Some(tox_friend_status_callback));
            sys::tox_callback_friend_connection_status(
                sys_tox,
                Some(tox_friend_connection_status_callback),
            );
            sys::tox_callback_friend_name(sys_tox, Some(tox_friend_name_callback));

            sys::toxav_callback_call(
                av,
                Some(toxav_call_callback),
                (&mut *tox.data as *mut ToxData) as *mut std::ffi::c_void,
            );
            sys::toxav_callback_call_state(
                av,
                Some(toxav_call_state_callback),
                (&mut *tox.data as *mut ToxData) as *mut std::ffi::c_void,
            );
            sys::toxav_callback_audio_receive_frame(
                av,
                Some(toxav_receive_audio),
                (&mut *tox.data as *mut ToxData) as *mut std::ffi::c_void,
            );
        }

        tox
    }

    /// Run the tox instance. This needs to be running for anything related to
    /// this tox instance to happen.
    ///
    /// Note: If this function is just stopped this allows you to effectively "go
    /// offline" while still maintaining all related data
    pub async fn run(&mut self) {
        loop {
            futures::select! {
                _ = time::sleep_until(self.next_tox).fuse() => {
                    self.iterate();
                },
                _ = time::sleep_until(self.next_av).fuse() => {
                    self.av_iterate();
                },
                (f_num, val) = wait_for_call_control(&mut self.data.call_data).fuse() => {
                    self.handle_call_control(f_num, val)
                }
            }
        }
    }

    impl_self_key_getter!(public_key, PublicKey);
    impl_self_key_getter!(secret_key, SecretKey);
    impl_self_key_getter!(address, ToxId);

    pub fn self_name(&self) -> String {
        unsafe {
            let length = sys::tox_self_get_name_size(self.sys_tox.get()) as usize;

            let mut name_unparsed = Vec::with_capacity(length);
            sys::tox_self_get_name(self.sys_tox.get(), name_unparsed.as_mut_ptr());
            name_unparsed.set_len(length);

            String::from_utf8_lossy(&name_unparsed).to_string()
        }
    }

    pub fn self_set_name(&mut self, name: &str) -> Result<(), SetInfoError> {
        unsafe {
            let mut err = TOX_ERR_SET_INFO_OK;
            sys::tox_self_set_name(
                self.sys_tox.get_mut(),
                name.as_ptr(),
                name.len() as u64,
                &mut err,
            );

            if err != TOX_ERR_SET_INFO_OK {
                return Err(SetInfoError);
            }

            Ok(())
        }
    }

    /// Retrieves all added toxcore friends
    pub fn friends(&mut self) -> Result<Vec<Friend>, ToxAddFriendError> {
        unsafe {
            let friend_indexes = {
                let length = sys::tox_self_get_friend_list_size(self.sys_tox.get()) as usize;

                let mut friend_indexes = Vec::with_capacity(length);
                sys::tox_self_get_friend_list(self.sys_tox.get(), friend_indexes.as_mut_ptr());
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

    pub fn add_friend(
        &mut self,
        address: ToxId,
        message: String,
    ) -> Result<Friend, ToxAddFriendError> {
        unsafe {
            let mut err = TOX_ERR_FRIEND_ADD_OK;
            let friend_num = sys::tox_friend_add(
                self.sys_tox.get_mut(),
                address.key.as_ptr(),
                message.as_ptr(),
                message.len() as u64,
                &mut err,
            );

            if err != TOX_ERR_FRIEND_ADD_OK {
                return Err(ToxAddFriendError::from(err));
            }

            self.friend_from_id(friend_num)
        }
    }

    /// Adds a friend without issuing a friend request. This can be called in
    /// response to a friend request, or if two users agree to add eachother via
    /// a different channel
    pub fn add_friend_norequest(
        &mut self,
        public_key: &PublicKey,
    ) -> Result<Friend, ToxAddFriendError> {
        unsafe {
            let mut err = TOX_ERR_FRIEND_ADD_OK;

            let friend_num = {
                if public_key.key.len() != sys::tox_public_key_size() as usize {
                    return Err(ToxAddFriendError::InvalidKey);
                }

                sys::tox_friend_add_norequest(
                    self.sys_tox.get_mut(),
                    public_key.key.as_ptr(),
                    &mut err as *mut TOX_ERR_FRIEND_ADD,
                )
            };

            if err != TOX_ERR_FRIEND_ADD_OK {
                return Err(ToxAddFriendError::from(err));
            }

            self.friend_from_id(friend_num)
        }
    }

    pub fn remove_friend(&mut self, friend: &Friend) -> Result<(), ToxFriendRemoveError> {
        unsafe {
            let mut err = TOX_ERR_FRIEND_DELETE_OK;
            sys::tox_friend_delete(self.sys_tox.get_mut(), friend.id, &mut err);

            if err != TOX_ERR_FRIEND_DELETE_OK {
                return Err(ToxFriendRemoveError::from(err));
            }

            self.data.friend_data.remove(&friend.id);
        }

        Ok(())
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
            sys::tox_friend_send_message(
                self.sys_tox.get_mut(),
                friend.id,
                t,
                ptr,
                len as u64,
                &mut err,
            )
        };

        if err != TOX_ERR_FRIEND_SEND_MESSAGE_OK {
            return Err(ToxSendMessageError::from(err));
        }

        Ok(Receipt {
            id: receipt_id,
            friend: friend.clone(),
        })
    }

    pub fn get_savedata(&self) -> Vec<u8> {
        unsafe {
            let data_size = sys::tox_get_savedata_size(self.sys_tox.get()) as usize;

            let mut data = Vec::with_capacity(data_size);
            sys::tox_get_savedata(self.sys_tox.get(), data.as_mut_ptr());
            data.set_len(data_size);
            data
        }
    }

    pub fn max_message_length(&self) -> usize {
        unsafe { sys::tox_max_message_length() as usize }
    }

    pub fn call_friend(&mut self, friend: &Friend) -> Result<ActiveCall, ToxCallError> {
        unsafe {
            let mut err = TOXAV_ERR_CALL_OK;
            sys::toxav_call(self.av.get_mut(), friend.id, 64u32, 0u32, &mut err);
            if err != TOXAV_ERR_CALL_OK {
                return Err(err.into());
            }
        }

        let (control_tx, control_rx) = mpsc::unbounded();
        let (event_tx, event_rx) = mpsc::unbounded();

        let call_data = Arc::new(RwLock::new(CallData {
            call_state: CallState::WaitingForPeerAnswer,
            _audio_enabled: true,
            _video_enabled: false,
        }));

        let data = Arc::clone(&call_data);

        self.data.call_data.insert(
            friend.id,
            ToxCallData {
                control: control_rx,
                event_channel: event_tx,
                data,
            },
        );

        Ok(ActiveCall::new(control_tx, event_rx, call_data))
    }

    /// Calls into toxcore to get the public key for the provided friend id
    fn public_key_from_id(&self, id: u32) -> Result<PublicKey, ToxFriendQueryError> {
        unsafe {
            let length = sys::tox_public_key_size() as usize;
            let mut public_key = Vec::with_capacity(length);
            let success = sys::tox_friend_get_public_key(
                self.sys_tox.get(),
                id,
                public_key.as_mut_ptr(),
                // For this API there is only one possible failure, we'll use
                // the return value instead
                std::ptr::null_mut(),
            );
            public_key.set_len(length);

            if !success {
                // NOTE: This isn't a 100% correct mapping from toxcore -> rust
                // errors, but the only possible failure from toxcore is that
                // the friend didn't exist, which really fits into the
                // ToxFriendQueryError enum conceptually
                return Err(ToxFriendQueryError::NotFound);
            }

            Ok(PublicKey { key: public_key })
        }
    }

    /// Calls into toxcore to get the name for the provided friend id
    fn name_from_id(&self, id: u32) -> Result<String, ToxFriendQueryError> {
        unsafe {
            let mut err = TOX_ERR_FRIEND_QUERY_OK;

            let length = sys::tox_friend_get_name_size(
                self.sys_tox.get(),
                id,
                &mut err as *mut TOX_ERR_FRIEND_QUERY,
            ) as usize;

            if err != TOX_ERR_FRIEND_QUERY_OK {
                return Err(ToxFriendQueryError::from(err));
            }

            let mut name = Vec::with_capacity(length);

            // Ignore return value since the error output will indicate the same thing
            let _success = sys::tox_friend_get_name(
                self.sys_tox.get(),
                id,
                name.as_mut_ptr(),
                &mut err as *mut TOX_ERR_FRIEND_QUERY,
            );

            if err != TOX_ERR_FRIEND_QUERY_OK {
                return Err(ToxFriendQueryError::from(err));
            }

            name.set_len(length);

            Ok(String::from_utf8_lossy(&name).to_string())
        }
    }

    fn status_from_id(&self, id: u32) -> Result<Status, ToxFriendQueryError> {
        let mut err = TOX_ERR_FRIEND_QUERY_OK;

        let connection_status = unsafe {
            sys::tox_friend_get_connection_status(
                self.sys_tox.get(),
                id,
                &mut err as *mut TOX_ERR_FRIEND_QUERY,
            )
        };

        if connection_status == TOX_CONNECTION_NONE {
            return Ok(Status::Offline);
        }

        if err != TOX_ERR_FRIEND_QUERY_OK {
            return Err(ToxFriendQueryError::from(err));
        }

        let status = unsafe {
            sys::tox_friend_get_status(
                self.sys_tox.get(),
                id,
                &mut err as *mut TOX_ERR_FRIEND_QUERY,
            )
        };

        if err != TOX_ERR_FRIEND_QUERY_OK {
            return Err(ToxFriendQueryError::from(err));
        }

        convert_status(status)
    }

    /// Creates a [`Friend`], populating the data in [`ToxData::friend_data`] if necessary.
    ///
    /// If [`ToxData::friend_data`] already exists the data in it will be overwritten
    fn friend_from_id(&mut self, id: u32) -> Result<Friend, ToxAddFriendError> {
        // If it exists we have to update the existing fields, otherwise we have to create with correct fields, either way we need to get the fields

        if let Some(existing_data) = self.data.friend_data.get(&id) {
            Ok(Friend {
                id,
                data: Arc::clone(existing_data),
            })
        } else {
            let public_key = self.public_key_from_id(id)?;
            let name = self.name_from_id(id)?;
            let status = self.status_from_id(id)?;

            let friend_data = FriendData {
                public_key,
                name,
                status,
            };

            let friend_data = Arc::new(RwLock::new(friend_data));
            self.data.friend_data.insert(id, Arc::clone(&friend_data));

            Ok(Friend {
                id,
                data: friend_data,
            })
        }
    }

    fn iterate(&mut self) {
        unsafe {
            let sys_tox = self.sys_tox.get_mut();

            sys::tox_iterate(
                sys_tox,
                (&mut *self.data as *mut ToxData) as *mut std::os::raw::c_void,
            );

            let now = time::Instant::now();
            while self.next_tox < now {
                self.next_tox +=
                    time::Duration::from_millis(sys::tox_iteration_interval(sys_tox) as u64);
            }
        }
    }

    fn av_iterate(&mut self) {
        unsafe {
            let av = self.av.get_mut();

            sys::toxav_iterate(av);

            let now = time::Instant::now();
            while self.next_av < now {
                self.next_av +=
                    time::Duration::from_millis(sys::toxav_iteration_interval(av) as u64);
            }
        }
    }

    fn handle_call_control(&mut self, friend_number: u32, event: Option<CallControl>) {
        if event.is_none() {
            self.data.call_data.remove(&friend_number);
            return;
        }

        let event = event.unwrap();

        match event {
            CallControl::Reject => {
                let mut err = TOXAV_ERR_CALL_CONTROL_OK;

                if let Some(data) = self.data.call_data.remove(&friend_number) {
                    let mut data = data.data.write().unwrap();
                    if data.call_state == CallState::Finished {
                        return;
                    } else {
                        data.call_state = CallState::Finished;
                    }
                } else {
                    error!("Internal call state invalid, rejecting call just in case...");
                }

                unsafe {
                    sys::toxav_call_control(
                        self.av.get_mut(),
                        friend_number,
                        TOXAV_CALL_CONTROL_CANCEL,
                        &mut err,
                    );
                }
                if err != TOXAV_ERR_CALL_CONTROL_OK {
                    error!("Failed to reject call: {}", CallControlError::from(err));
                }
            }
            CallControl::Accepted => {
                let mut err = TOXAV_ERR_ANSWER_OK;
                unsafe {
                    sys::toxav_answer(self.av.get_mut(), friend_number, 64u32, 0, &mut err);
                }

                if err != TOXAV_ERR_ANSWER_OK {
                    error!("Failed to answer call {}", err);
                    if let Some(data) = self.data.call_data.remove(&friend_number) {
                        data.set_call_state(CallState::Finished);
                    }
                }

                match self.data.call_data.get(&friend_number) {
                    Some(data) => {
                        data.set_call_state(CallState::Active);
                    }
                    None => error!("Call data missing"),
                }
            }
            CallControl::SendAudio(frame) => {
                let active_call_friends =
                    self.data
                        .call_data
                        .iter()
                        .filter_map(|(friend, call_data)| {
                            match call_data.data.read().unwrap().call_state {
                                CallState::Active => Some(friend),
                                _ => None,
                            }
                        });

                for friend in active_call_friends {
                    unsafe {
                        let mut err = TOXAV_ERR_SEND_FRAME_OK;
                        sys::toxav_audio_send_frame(
                            self.av.get_mut(),
                            *friend,
                            frame.data.as_ptr(),
                            (frame.data.len() / frame.channels as usize) as u64,
                            frame.channels,
                            frame.sample_rate,
                            &mut err,
                        );
                        if err != TOXAV_ERR_SEND_FRAME_OK {
                            error!("oh no: {}", err);
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Tox {
    fn drop(&mut self) {
        unsafe { sys::toxav_kill(self.av.get_mut()) }
        unsafe { sys::tox_kill(self.sys_tox.get_mut()) }
    }
}

// toxcore claims that it is safe to use the const APIs from multiple threads.
// As long as it isn't casting out the const anywhere under the hood I don't see
// why we can't trust it. That means that we implement both Send + Sync. Rust's
// mutability rules will prevent us from modifying the interior tox state while
// reading from it
unsafe impl Send for Tox {}
unsafe impl Sync for Tox {}

/// Wrapper struct to help us manage mutability of the interior tox pointer
struct MutabilityWrapper<T> {
    val: *mut T,
}

impl<T> MutabilityWrapper<T> {
    fn new(val: *mut T) -> Self {
        Self { val }
    }

    fn get(&self) -> *const T {
        self.val
    }

    fn get_mut(&mut self) -> *mut T {
        self.val
    }
}

type SysToxMutabilityWrapper = MutabilityWrapper<toxcore_sys::Tox>;
type ToxAvMutabilityWrapper = MutabilityWrapper<toxcore_sys::ToxAV>;

struct ToxCallData {
    control: UnboundedReceiver<CallControl>,
    event_channel: UnboundedSender<CallEvent>,
    data: Arc<RwLock<CallData>>,
}

impl ToxCallData {
    fn set_call_state(&self, state: CallState) {
        let mut data = self.data.write().unwrap();
        if data.call_state == state {
            return;
        }

        data.call_state = state;

        if let Err(e) = self
            .event_channel
            .unbounded_send(CallEvent::CallStateChanged(state))
        {
            warn!("Failed to send call state to call handle: {}", e);
        }
    }
}

/// mutability rules
struct ToxData {
    event_callback: Option<ToxEventCallback>,
    friend_data: HashMap<u32, Arc<RwLock<FriendData>>>,
    call_data: HashMap<u32, ToxCallData>,
}

async fn wait_for_call_control(
    call_data: &mut HashMap<u32, ToxCallData>,
) -> (u32, Option<CallControl>) {
    if call_data.is_empty() {
        futures::future::pending::<()>().await;
    }

    let call_controls = call_data
        .iter_mut()
        .map(|(f, call_data)| call_data.control.next().map(move |v| (*f, v)));

    futures::future::select_all(call_controls).await.0
}
/// Callback function provided to toxcore for incoming friend requests
///
/// Messages wil be forwarded to [`ToxData::friend_request_tx`]
pub(crate) unsafe extern "C" fn tox_friend_request_callback(
    _sys_tox: *mut toxcore_sys::Tox,
    input_public_key: *const u8,
    input_message: *const u8,
    length: u64,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let public_key_length = sys::tox_public_key_size() as usize;

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

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::FriendRequest(request));
    }
}

/// Callback function provided to toxcore for incoming messages.
///
/// Messages will be forwarded to the appropriate [`FriendData::message_received_tx`]
pub(crate) unsafe extern "C" fn tox_friend_message_callback(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    message_type: TOX_MESSAGE_TYPE,
    message: *const u8,
    length: u64,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

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

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    let f = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::MessageReceived(f, message));
    }
}

unsafe extern "C" fn tox_friend_read_receipt_callback(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    message_id: u32,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    let f = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::ReadReceipt(Receipt {
            id: message_id,
            friend: f,
        }));
    }
}

unsafe extern "C" fn tox_friend_status_callback(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    status: TOX_USER_STATUS,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    let converted_status = convert_status(status);

    if converted_status.is_err() {
        warn!("Invalid incoming status: {}", status);
        return;
    }

    let converted_status = converted_status.unwrap();

    friend_data.write().unwrap().status = converted_status;

    let f = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::StatusUpdated(f));
    }
}

unsafe extern "C" fn tox_friend_connection_status_callback(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    connection: TOX_CONNECTION,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    // We only care about the offline callback, We determine a friend has gone "online" via the friend status callback
    if connection != TOX_CONNECTION_NONE {
        return;
    }

    friend_data.write().unwrap().status = Status::Offline;

    let f = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::StatusUpdated(f));
    }
}

fn convert_status(status: TOX_USER_STATUS) -> Result<Status, ToxFriendQueryError> {
    let status = match status {
        TOX_USER_STATUS_NONE => Status::Online,
        TOX_USER_STATUS_AWAY => Status::Away,
        TOX_USER_STATUS_BUSY => Status::Busy,
        _ => return Err(ToxFriendQueryError::Unknown),
    };

    Ok(status)
}

unsafe extern "C" fn tox_friend_name_callback(
    _tox: *mut toxcore_sys::Tox,
    friend_number: u32,
    input_name: *const u8,
    len: u64,
    user_data: *mut std::os::raw::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    let name = std::slice::from_raw_parts(input_name, len as usize);

    friend_data.write().unwrap().name = String::from_utf8_lossy(name).to_string();

    let f = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::NameUpdated(f));
    }
}

unsafe extern "C" fn toxav_call_callback(
    _av: *mut toxcore_sys::ToxAV,
    friend_number: u32,
    audio_enabled: bool,
    video_enabled: bool,
    user_data: *mut std::ffi::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_data = match tox_data.friend_data.get(&friend_number) {
        Some(d) => d,
        None => {
            error!("Friend data is not initialized");
            return;
        }
    };

    let friend = Friend {
        id: friend_number,
        data: Arc::clone(&friend_data),
    };

    let call_data = Arc::new(RwLock::new(CallData {
        call_state: CallState::WaitingForSelfAnswer,
        _audio_enabled: audio_enabled,
        _video_enabled: video_enabled,
    }));

    let (control_tx, control_rx) = mpsc::unbounded();
    let (event_tx, event_rx) = mpsc::unbounded();

    tox_data.call_data.insert(
        friend_number,
        ToxCallData {
            control: control_rx,
            event_channel: event_tx,
            data: Arc::clone(&call_data),
        },
    );

    let call = IncomingCall::new(control_tx, event_rx, call_data, friend);

    if let Some(callback) = &mut tox_data.event_callback {
        (*callback)(Event::IncomingCall(call))
    }
}

unsafe extern "C" fn toxav_call_state_callback(
    _av: *mut toxcore_sys::ToxAV,
    friend_number: u32,
    state: u32,
    user_data: *mut std::ffi::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_call_data = tox_data.call_data.get_mut(&friend_number);

    if friend_call_data.is_none() {
        error!(
            "No call data handler registered for friend {}",
            friend_number
        );
        return;
    }

    let call_data = friend_call_data.unwrap();

    if state & (TOXAV_FRIEND_CALL_STATE_ERROR | TOXAV_FRIEND_CALL_STATE_FINISHED) == 0 {
        call_data.set_call_state(CallState::Active);
        return;
    }

    call_data.set_call_state(CallState::Finished);
    tox_data.call_data.remove(&friend_number);
}

unsafe extern "C" fn toxav_receive_audio(
    _av: *mut ToxAV,
    friend_number: u32,
    pcm: *const i16,
    sample_count: size_t,
    channels: u8,
    sample_rate: u32,
    user_data: *mut std::ffi::c_void,
) {
    let tox_data = &mut *(user_data as *mut ToxData);

    let friend_call_data = tox_data.call_data.get_mut(&friend_number);

    if friend_call_data.is_none() {
        // FIXME: Log spammmmmm
        error!(
            "No call data handler registered for friend {}",
            friend_number
        );
        return;
    }

    let call_data = friend_call_data.unwrap();

    let data = std::slice::from_raw_parts(pcm, sample_count as usize * channels as usize);

    let frame = AudioFrame {
        data: Arc::new(data.into()),
        channels,
        sample_rate,
    };

    if let Err(e) = call_data
        .event_channel
        .unbounded_send(CallEvent::AudioReceived(frame))
    {
        warn!("Failed to send audio to call handle: {}", e);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use futures::FutureExt;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    pub(crate) struct ToxFixture {
        tox: Tox,
        _kill_ctx: sys::__tox_kill::Context,
        _kill_av_ctx: sys::__toxav_kill::Context,
        _public_key_size_ctx: sys::__tox_public_key_size::Context,
        _toxav_callback_call_ctx: sys::__toxav_callback_call::Context,
        _toxav_callback_call_state_ctx: sys::__toxav_callback_call_state::Context,
        _toxav_callback_audio_receive_frame_ctx: sys::__toxav_callback_audio_receive_frame::Context,
        _callback_friend_request_ctx: sys::__tox_callback_friend_request::Context,
        _callback_friend_message_ctx: sys::__tox_callback_friend_message::Context,
        _callback_friend_read_receipt_ctx: sys::__tox_callback_friend_read_receipt::Context,
        _callback_friend_status_ctx: sys::__tox_callback_friend_status::Context,
        _callback_friend_connection_status_ctx:
            sys::__tox_callback_friend_connection_status::Context,
        _callback_friend_name_ctx: sys::__tox_callback_friend_name::Context,
        _friend_get_public_key_ctx: sys::__tox_friend_get_public_key::Context,
        _friend_get_name_size_ctx: sys::__tox_friend_get_name_size::Context,
        _friend_get_name_ctx: sys::__tox_friend_get_name::Context,
        _friend_get_status_ctx: sys::__tox_friend_get_status::Context,
        _friend_get_connection_status_ctx: sys::__tox_friend_get_connection_status::Context,
        pk_len: usize,
        default_peer_pk: PublicKey,
        default_peer_id: u32,
        default_peer_name: String,
    }

    impl ToxFixture {
        pub(crate) fn new() -> ToxFixture {
            let default_peer_pk = PublicKey {
                key: "testkey1".to_string().into_bytes(),
            };

            let default_peer_id = 10u32;

            let default_peer_name = "TestPeer";

            let callback_friend_request_ctx = sys::tox_callback_friend_request_context();
            callback_friend_request_ctx.expect().return_const(()).once();

            let callback_friend_message_ctx = sys::tox_callback_friend_message_context();
            callback_friend_message_ctx.expect().return_const(()).once();

            let callback_friend_read_receipt_ctx = sys::tox_callback_friend_read_receipt_context();
            callback_friend_read_receipt_ctx
                .expect()
                .return_const(())
                .once();

            let kill_ctx = sys::tox_kill_context();
            kill_ctx.expect().return_const(()).once();

            let kill_av_ctx = sys::toxav_kill_context();
            kill_av_ctx.expect().return_const(()).once();

            let public_key_size_ctx = sys::tox_public_key_size_context();
            public_key_size_ctx
                .expect()
                .return_const(default_peer_pk.key.len() as u32);

            let default_peer_pk_clone = default_peer_pk.clone();
            let friend_get_public_key_ctx = sys::tox_friend_get_public_key_context();
            friend_get_public_key_ctx
                .expect()
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

            let friend_get_name_size_ctx = sys::tox_friend_get_name_size_context();
            friend_get_name_size_ctx
                .expect()
                .withf_st(move |_, id, _err| *id == default_peer_id)
                .return_const(default_peer_name.len() as u32);

            let friend_get_name_ctx = sys::tox_friend_get_name_context();
            friend_get_name_ctx
                .expect()
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

            let friend_get_status_ctx = sys::tox_friend_get_status_context();
            friend_get_status_ctx
                .expect()
                .withf_st(move |_, id, _err| *id == default_peer_id)
                .returning_st(move |_, _id, _err| TOX_USER_STATUS_NONE);

            let friend_get_connection_status_ctx = sys::tox_friend_get_connection_status_context();
            friend_get_connection_status_ctx
                .expect()
                .withf_st(move |_, id, _err| *id == default_peer_id)
                .returning_st(move |_, _id, _err| TOX_CONNECTION_UDP);

            let callback_friend_status_ctx = sys::tox_callback_friend_status_context();
            callback_friend_status_ctx
                .expect()
                .return_const(())
                .times(1);

            let callback_friend_connection_status_ctx =
                sys::tox_callback_friend_connection_status_context();
            callback_friend_connection_status_ctx
                .expect()
                .return_const(())
                .times(1);

            let callback_friend_name_ctx = sys::tox_callback_friend_name_context();
            callback_friend_name_ctx.expect().return_const(()).times(1);

            let toxav_callback_call_ctx = sys::toxav_callback_call_context();
            toxav_callback_call_ctx.expect().return_const(()).times(1);

            let toxav_callback_call_state_ctx = sys::toxav_callback_call_state_context();
            toxav_callback_call_state_ctx.expect().return_const(()).times(1);

            let toxav_callback_audio_receive_frame_ctx = sys::toxav_callback_audio_receive_frame_context();
            toxav_callback_audio_receive_frame_ctx.expect().return_const(()).times(1);

            let tox = Tox::new(std::ptr::null_mut(), std::ptr::null_mut(), None);

            ToxFixture {
                tox,
                _kill_ctx: kill_ctx,
                _kill_av_ctx: kill_av_ctx,
                _public_key_size_ctx: public_key_size_ctx,
                _toxav_callback_call_ctx: toxav_callback_call_ctx,
                _toxav_callback_call_state_ctx: toxav_callback_call_state_ctx,
                _toxav_callback_audio_receive_frame_ctx: toxav_callback_audio_receive_frame_ctx,
                _callback_friend_request_ctx: callback_friend_request_ctx,
                _callback_friend_message_ctx: callback_friend_message_ctx,
                _callback_friend_read_receipt_ctx: callback_friend_read_receipt_ctx,
                _callback_friend_status_ctx: callback_friend_status_ctx,
                _callback_friend_connection_status_ctx: callback_friend_connection_status_ctx,
                _callback_friend_name_ctx: callback_friend_name_ctx,
                _friend_get_public_key_ctx: friend_get_public_key_ctx,
                _friend_get_name_size_ctx: friend_get_name_size_ctx,
                _friend_get_name_ctx: friend_get_name_ctx,
                _friend_get_status_ctx: friend_get_status_ctx,
                _friend_get_connection_status_ctx: friend_get_connection_status_ctx,
                pk_len: default_peer_pk_clone.key.len(),
                default_peer_pk: default_peer_pk_clone,
                default_peer_id,
                default_peer_name: default_peer_name.to_string(),
            }
        }
    }

    rusty_fork::rusty_fork_test! {
            #[test]
            fn test_iteration() {
                async fn test_iteration_async() -> Result<(), Box<dyn std::error::Error>>  {
                    const ITERATION_INTERVAL: u32 = 20;
                    const AV_ITERATION_INTERVAL: u32 = ITERATION_INTERVAL * 2;

                    let iteration_interval_ctx = sys::tox_iteration_interval_context();
                    iteration_interval_ctx.expect()
                        .return_const(ITERATION_INTERVAL);

                    let av_iteration_interval_ctx = sys::toxav_iteration_interval_context();
                    av_iteration_interval_ctx.expect()
                        .return_const(AV_ITERATION_INTERVAL);

                    use std::sync::atomic::Ordering;
                    let iterations = Arc::new(AtomicU64::new(0));
                    let closure_iterations = Arc::clone(&iterations);

                    let iterate_ctx = sys::tox_iterate_context();
                    iterate_ctx.expect().returning_st(move |_, _| {
                        closure_iterations.store(
                            closure_iterations.load(Ordering::Relaxed) + 1u64,
                            Ordering::Relaxed,
                        );
                    });

                    let av_iterations = Arc::new(AtomicU64::new(0));
                    let av_closure_iterations = Arc::clone(&av_iterations);
                    let av_iterate_ctx = sys::toxav_iterate_context();
                    av_iterate_ctx.expect().returning_st(move |_| {
                        av_closure_iterations.store(
                            av_closure_iterations.load(Ordering::Relaxed) + 1u64,
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

                    let mut fixture = ToxFixture::new();

                    futures::select! {
                        _ = fixture.tox.run().fuse() => { }
                        _ = cancel_future.fuse() => { }
                    };

                    // toxcore asks us to sleep for iteration_interval, we can have some
                    // leeway from what they request since toxcore_iterate will naturally
                    // take some time
                    assert!(iterations.load(Ordering::Relaxed) > NUM_ITERATIONS * 4 / 5);

                    assert!(av_iterations.load(Ordering::Relaxed) > NUM_ITERATIONS * 2 / 5);
                    assert!(av_iterations.load(Ordering::Relaxed) < NUM_ITERATIONS * 3 / 5);

                    Ok(())
                }

                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(test_iteration_async())
                    .unwrap();
            }

        #[test]
        fn test_friend_request_dispatch() -> Result<(), Box<dyn std::error::Error>> {
            let mut fixture = ToxFixture::new();

            let message_str = "message";
            let message = message_str.to_string().into_bytes();

            let default_peer_pk = fixture.default_peer_pk.clone();

            let callback_called = Arc::new(AtomicBool::new(false));
            let callback_called_clone = Arc::clone(&callback_called);

            use std::sync::atomic::Ordering;

            // Hack in the friend request callback instead of making a fixture builder
            fixture.tox.data.event_callback = Some(Box::new(move |event| {
                callback_called_clone.store(true, Ordering::Relaxed);
                match event {
                    Event::FriendRequest(friend_request) => {
                        assert_eq!(friend_request.message, message_str);
                        assert_eq!(friend_request.public_key, default_peer_pk);
                    }
                    _ => assert!(false),
                }
            }));

            unsafe {
                tox_friend_request_callback(
                    std::ptr::null_mut(),
                    fixture.default_peer_pk.key.as_ptr(),
                    message.as_ptr(),
                    message.len() as u64,
                    (&mut *fixture.tox.data as *mut ToxData)
                        as *mut std::os::raw::c_void,
                );
            }

            assert!(callback_called.load(Ordering::Relaxed));

            Ok(())
        }

        #[test]
        fn test_friend_status_dispatch() -> Result<(), Box<dyn std::error::Error>> {
            // Initialize our default friend
            let mut fixture = ToxFixture::new();

            let callback_called = Arc::new(AtomicBool::new(false));
            let callback_called_clone = Arc::clone(&callback_called);

            use std::sync::atomic::Ordering;

            // Hack in the friend request callback instead of making a fixture builder
            fixture.tox.data.event_callback = Some(Box::new(move |event| {
                callback_called_clone.store(true, Ordering::Relaxed);
                match event {
                    Event::StatusUpdated(friend) => {
                        assert_eq!(friend.status(), Status::Busy);
                    }
                    _ => assert!(false),
                }
            }));

            // Initialize our default friend
            // FIXME we need a better test infra for testing this
            let peer_pk = fixture.default_peer_pk.clone();
            let pk_len = peer_pk.key.len();
            let friend_add_norequest_ctx = sys::tox_friend_add_norequest_context();
            friend_add_norequest_ctx
                .expect()
                .withf_st(move |_, input_public_key, _err| {
                    let slice = unsafe { std::slice::from_raw_parts(*input_public_key, pk_len) };
                    slice == peer_pk.key
                })
                .return_const(fixture.default_peer_id)
                .once();

            fixture.tox.add_friend_norequest(&fixture.default_peer_pk)?;

            unsafe {
                tox_friend_status_callback(
                    std::ptr::null_mut(),
                    fixture.default_peer_id,
                    TOX_USER_STATUS_BUSY,
                    (&mut *fixture.tox.data as *mut ToxData)
                        as *mut std::os::raw::c_void,
                );
            }

            assert!(callback_called.load(Ordering::Relaxed));

            Ok(())
        }

        #[test]
        fn test_get_self_name() {
            let self_name = "TestName";

            let self_get_name_size_ctx = sys::tox_self_get_name_size_context();
            self_get_name_size_ctx.expect()
                .return_const(self_name.len() as u64);

            let self_get_name_ctx = sys::tox_self_get_name_context();
            self_get_name_ctx.expect()
                .returning_st(move |_, name_out| unsafe {
                    std::ptr::copy_nonoverlapping(self_name.as_ptr(), name_out, self_name.len())
                });

            let fixture = ToxFixture::new();

            assert_eq!(fixture.tox.self_name(), self_name);
        }

        #[test]
        fn test_friend_retrieval() {
            const NUM_FRIENDS: usize = 4;

            // Set up fake friends list with 3 items
            let self_get_friend_list_size_ctx = sys::tox_self_get_friend_list_size_context();
            self_get_friend_list_size_ctx.expect()
                .return_const(NUM_FRIENDS as u32);

            let self_get_friend_list_ctx = sys::tox_self_get_friend_list_context();
            self_get_friend_list_ctx.expect()
                .returning_st(|_, output_list| unsafe {
                    *output_list = 1;
                    *output_list.offset(1) = 2;
                    *output_list.offset(2) = 3;
                    *output_list.offset(3) = 4;
                });

            fn is_in_friend_list(id: &u32) -> bool {
                *id == 1u32 || *id == 2u32 || *id == 3u32 || *id == 4u32
            }
            // mocked friend PKs will only be 3 long, "pk1", "pk2", "pk3"
            let public_key_size_ctx = sys::tox_public_key_size_context();
            public_key_size_ctx.expect().return_const(3 as u32);
            let friend_get_public_key_ctx = sys::tox_friend_get_public_key_context();
            friend_get_public_key_ctx.expect()
                .withf_st(|_, id, _output, _error| is_in_friend_list(id))
                .returning_st(|_, id, output, _error| {
                    unsafe {
                        let key = format!("pk{}", id);
                        std::ptr::copy_nonoverlapping(key.as_ptr(), output, key.len())
                    }
                    true
                })
                .times(NUM_FRIENDS);

            let friend_get_name_size_ctx = sys::tox_friend_get_name_size_context();
            friend_get_name_size_ctx.expect().return_const(5 as u32);
            let friend_get_name_ctx = sys::tox_friend_get_name_context();
            friend_get_name_ctx.expect()
                .withf_st(|_, id, _output, _error| is_in_friend_list(id))
                .returning_st(|_, id, output, _error| {
                    unsafe {
                        let name = format!("name{}", id);
                        std::ptr::copy_nonoverlapping(name.as_ptr(), output, name.len())
                    }
                    true
                })
                .times(NUM_FRIENDS);

            let friend_get_status_ctx = sys::tox_friend_get_status_context();
            friend_get_status_ctx.expect()

                .withf_st(|_, id, _error| is_in_friend_list(id))
                .returning_st(|_, id, _error| match id {
                    2u32 => TOX_USER_STATUS_AWAY,
                    3u32 => TOX_USER_STATUS_BUSY,
                    _ => TOX_USER_STATUS_NONE,
                })
                .times(NUM_FRIENDS - 1); // Offline friend will not call this

            let friend_get_connection_status_ctx = sys::tox_friend_get_connection_status_context();
            friend_get_connection_status_ctx.expect()
                .withf_st(|_, id, _error| is_in_friend_list(id))
                .returning_st(|_, id, _error| {
                    if id == 4u32 {
                        TOX_CONNECTION_NONE
                    } else {
                        TOX_CONNECTION_UDP
                    }
                })
                .times(NUM_FRIENDS);

            let mut fixture = ToxFixture::new();

            let friends = fixture.tox.friends().unwrap();

            let friend_matches_id = |friend: &Friend, id: u32| {
                friend.name() == format!("name{}", id)
                    && friend.public_key().key == format!("pk{}", id).into_bytes()
            };

            assert_eq!(friends.len(), NUM_FRIENDS);

            let friend1 = friends
                .iter()
                .find(|item| friend_matches_id(item, 1))
                .unwrap();
            assert_eq!(friend1.public_key().as_bytes(), "pk1".as_bytes());
            assert_eq!(friend1.name(), "name1");
            assert_eq!(friend1.status(), Status::Online);

            let friend2 = friends
                .iter()
                .find(|item| friend_matches_id(item, 2))
                .unwrap();
            assert_eq!(friend2.public_key().as_bytes(), "pk2".as_bytes());
            assert_eq!(friend2.name(), "name2");
            assert_eq!(friend2.status(), Status::Away);

            let friend3 = friends
                .iter()
                .find(|item| friend_matches_id(item, 3))
                .unwrap();
            assert_eq!(friend3.public_key().as_bytes(), "pk3".as_bytes());
            assert_eq!(friend3.name(), "name3");
            assert_eq!(friend3.status(), Status::Busy);

            let friend4 = friends
                .iter()
                .find(|item| friend_matches_id(item, 4))
                .unwrap();
            assert_eq!(friend4.public_key().as_bytes(), "pk4".as_bytes());
            assert_eq!(friend4.name(), "name4");
            assert_eq!(friend4.status(), Status::Offline);
        }

        #[test]
        fn test_friend_retrieval_name_failure() {
            let friend_get_name_size_ctx = sys::tox_friend_get_name_size_context();
            friend_get_name_size_ctx.expect()
                .withf_st(|_, id, _err| *id == 0u32)
                .return_const(10 as u64)
                .once();

            let friend_get_name_ctx = sys::tox_friend_get_name_context();
            friend_get_name_ctx.expect()
                .withf_st(|_, id, _output, _err| *id == 0u32)
                .returning_st(|_, _id, _output, err| {
                    unsafe {
                        *err = TOX_ERR_FRIEND_QUERY_NULL;
                    }
                    return false;
                })
                .once();

            // Expect a second call where we fail on retrieval of the name size
            // instead of the name itself
            let friend_get_name_size_ctx = sys::tox_friend_get_name_size_context();
            friend_get_name_size_ctx.expect()

                .withf_st(|_, id, _err| *id == 0u32)
                .returning_st(|_, _id, err| {
                    unsafe {
                        *err = TOX_ERR_FRIEND_QUERY_NULL;
                    }
                    99348
                })
                .once();

            let fixture = ToxFixture::new();
            assert!(fixture.tox.name_from_id(0).is_err());
            assert!(fixture.tox.name_from_id(0).is_err());
        }

        #[test]
        fn test_friend_retrieval_pk_failure() {
            let friend_get_public_key_ctx = sys::tox_friend_get_public_key_context();
            friend_get_public_key_ctx.expect()
                .withf_st(|_, id, _output, _err| *id == 0u32)
                .returning_st(|_, _id, _output, _err| {
                    // NOTE: at the time of writing the caller passes in a null err
                    // pointer and relies on the return value
                    return false;
                });

            let fixture = ToxFixture::new();
            assert!(fixture.tox.public_key_from_id(0).is_err());
        }

        #[test]
        fn test_add_friend_norequest() -> Result<(), Box<dyn std::error::Error>> {
            let mut fixture = ToxFixture::new();

            let peer_pk = fixture.default_peer_pk.clone();
            let pk_len = fixture.pk_len;

            let friend_add_norequest_ctx = sys::tox_friend_add_norequest_context();
            friend_add_norequest_ctx
            .expect().withf_st(move |_, input_public_key, _err| {
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
            let mut fixture = ToxFixture::new();

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
            // Test toxcore failure triggers a failure for us
            let friend_add_norequest_ctx = sys::tox_friend_add_norequest_context();
            friend_add_norequest_ctx
                .expect()
                .returning_st(move |_, _, err| {
                    unsafe {
                        *err = TOX_ERR_FRIEND_ADD_NO_MESSAGE;
                    }
                    u32::MAX
                })
                .once();

            let mut fixture = ToxFixture::new();

            assert!(fixture
                .tox
                .add_friend_norequest(&fixture.default_peer_pk)
                .is_err());

            Ok(())
        }

        #[test]
        fn test_add_friend() -> Result<(), Box<dyn std::error::Error>> {
            let mut fixture = ToxFixture::new();

            let friend_add_ctx = sys::tox_friend_add_context();
            friend_add_ctx
                .expect()
                .times(1)
                .return_const_st(3u32);

            friend_add_ctx
                .expect()
                .times(1)
                .return_const_st(4u32);

            let friend_get_public_key_ctx = sys::tox_friend_get_public_key_context();
            friend_get_public_key_ctx
                .expect()
                .times(1)
                .returning_st(|_, _id, ptr, _err| {
                    unsafe { std::ptr::copy_nonoverlapping(b"testkey2".as_ptr(), ptr, 8) };
                    true
                });

            friend_get_public_key_ctx
                .expect()
                .times(1)
                .returning_st(|_, _id, ptr, _err| {
                    unsafe { std::ptr::copy_nonoverlapping(b"testkey3".as_ptr(), ptr, 8) };
                    true
                });

            let friend_name_size_ctx = sys::tox_friend_get_name_size_context();
            friend_name_size_ctx
                .expect()
                .times(2)
                .return_const_st(5u32);

            let friend_name_size_ctx = sys::tox_friend_get_name_context();
            friend_name_size_ctx
                .expect()
                .times(1)
                .returning_st(|_, _id, name, _err| {
                    unsafe { std::ptr::copy_nonoverlapping(b"test2".as_ptr(), name, 8) };
                    true
                });

            friend_name_size_ctx
                .expect()
                .times(1)
                .returning_st(|_, _id, name, _err| {
                    unsafe { std::ptr::copy_nonoverlapping(b"test3".as_ptr(), name, 8) };
                    true
                });

            let get_connection_status_ctx = sys::tox_friend_get_connection_status_context();
            get_connection_status_ctx
                .expect()
                .times(2)
                .return_const_st(TOX_CONNECTION_NONE);


            let _friend = fixture.tox.add_friend(ToxId::from_bytes(vec![0; 38]).unwrap(), "Message".into())?;
            let _friend2 = fixture.tox.add_friend(ToxId::from_bytes(vec![1; 38]).unwrap(), "Message".into())?;

            Ok(())
        }

        #[test]
        fn test_remove_friend() -> Result<(), Box<dyn std::error::Error>> {
            let mut fixture = ToxFixture::new();

            let default_peer_id = fixture.default_peer_id;

            let add_friend_norequest_ctx = sys::tox_friend_add_norequest_context();
            add_friend_norequest_ctx
                .expect()
                .returning_st(move |_, _pk, _err| {
                    default_peer_id
                });

            let remove_friend_ctx = sys::tox_friend_delete_context();
            remove_friend_ctx
                .expect()
                .times(1)
                .withf_st(move |_, id, _err| {
                    *id == default_peer_id
                })
                .return_const_st(true);

            let friend = fixture.tox.add_friend_norequest(&fixture.default_peer_pk)?;
            fixture.tox.remove_friend(&friend)?;

            Ok(())

        }
    }

    // FIXME: test friend name dispatch

    macro_rules! test_array_getter {
        ($name:ident, $value:expr) => {
            paste! {
                rusty_fork::rusty_fork_test! {
                #[test]
                fn [<test_self_ $name>]() -> Result<(), Box<dyn std::error::Error>> {
                    let key = $value.chars().map(|c| c as u8).collect::<Vec<u8>>();

                    let size_ctx = sys::[<tox_ $name _size_context>]();
                    size_ctx.expect()
                        .return_const(key.len() as u32)
                        .once();

                    let key_clone = key.clone();

                    let ctx = sys::[<tox_self_get_ $name _context>]();
                    ctx.expect()
                        .return_const(())
                        .returning_st(move |_, output_key| {
                            unsafe {
                                std::ptr::copy_nonoverlapping(key_clone.as_ptr(), output_key, key_clone.len());
                            }
                        });

                    let fixture = ToxFixture::new();

                    let retrieved_key = fixture.tox.[<self_ $name>]().key;
                    assert_eq!(retrieved_key, key);

                    Ok(())
                }
            }

            }
        }
    }

    test_array_getter!(public_key, "public_key");
    test_array_getter!(secret_key, "secret_key");
    test_array_getter!(address, "address");
}
