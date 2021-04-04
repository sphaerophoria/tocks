//! Wrapper trait/implementation for toxcore APIs. toxcore_sys APIs are wrapped
//! as traits to allow for mocking/testing

use mockall::automock;

use toxcore_sys::*;

#[automock]
pub trait ToxApi: Send + Sync {
    #[allow(clippy::new_ret_no_self)]
    unsafe fn new(
        &self,
        options: *const Tox_Options,
        error: *mut TOX_ERR_NEW,
    ) -> *mut toxcore_sys::Tox;
    unsafe fn kill(&self, tox: *mut toxcore_sys::Tox);
    unsafe fn iterate(&self, tox: *mut toxcore_sys::Tox, user_data: *mut ::std::os::raw::c_void);
    unsafe fn iteration_interval(&self, tox: *const toxcore_sys::Tox) -> u32;
    unsafe fn get_savedata_size(&self, tox: *const toxcore_sys::Tox) -> u64;
    unsafe fn get_savedata(&self, tox: *const toxcore_sys::Tox, savedata: *mut u8);
    unsafe fn public_key_size(&self) -> u32;
    unsafe fn self_get_public_key(&self, tox: *const toxcore_sys::Tox, public_key: *mut u8);
    unsafe fn secret_key_size(&self) -> u32;
    unsafe fn self_get_secret_key(&self, tox: *const toxcore_sys::Tox, secret_key: *mut u8);
    unsafe fn address_size(&self) -> u32;
    unsafe fn self_get_address(&self, tox: *const toxcore_sys::Tox, address: *mut u8);
    unsafe fn self_get_name_size(&self, tox: *const toxcore_sys::Tox) -> u64;
    unsafe fn self_get_name(&self, tox: *const toxcore_sys::Tox, name: *mut u8);
    unsafe fn self_set_name(
        &self,
        tox: *mut toxcore_sys::Tox,
        name: *const u8,
        length: u64,
        error: *mut TOX_ERR_SET_INFO,
    ) -> bool;
    unsafe fn self_get_friend_list_size(&self, tox: *const toxcore_sys::Tox) -> u64;
    unsafe fn self_get_friend_list(&self, tox: *const toxcore_sys::Tox, friend_list: *mut u32);
    unsafe fn friend_add_norequest(
        &self,
        tox: *mut toxcore_sys::Tox,
        public_key: *const u8,
        error: *mut TOX_ERR_FRIEND_ADD,
    ) -> u32;
    unsafe fn friend_get_public_key(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        public_key: *mut u8,
        error: *mut TOX_ERR_FRIEND_GET_PUBLIC_KEY,
    ) -> bool;
    unsafe fn friend_get_name_size(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> u64;
    unsafe fn friend_get_name(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        name: *mut u8,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> bool;
    unsafe fn max_message_length(&self) -> u32;
    unsafe fn friend_send_message(
        &self,
        tox: *mut toxcore_sys::Tox,
        friend_number: u32,
        type_: TOX_MESSAGE_TYPE,
        message: *const u8,
        length: size_t,
        error: *mut TOX_ERR_FRIEND_SEND_MESSAGE,
    ) -> u32;
    unsafe fn friend_get_status(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> TOX_USER_STATUS;
    unsafe fn friend_get_connection_status(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> TOX_CONNECTION;
    unsafe fn callback_friend_request(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_request_cb,
    );
    unsafe fn callback_friend_message(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_message_cb,
    );
    unsafe fn callback_friend_read_receipt(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_read_receipt_cb,
    );
    unsafe fn callback_friend_status(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_status_cb,
    );
    unsafe fn callback_friend_connection_status(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_connection_status_cb,
    );
    unsafe fn callback_friend_name(&self, tox: *mut toxcore_sys::Tox, callback: tox_friend_name_cb);
}

pub(crate) struct ToxApiImpl;

impl ToxApi for ToxApiImpl {
    unsafe fn new(
        &self,
        options: *const Tox_Options,
        err: *mut TOX_ERR_NEW,
    ) -> *mut toxcore_sys::Tox {
        tox_new(options, err)
    }
    unsafe fn kill(&self, tox: *mut toxcore_sys::Tox) {
        tox_kill(tox)
    }
    unsafe fn iterate(&self, tox: *mut toxcore_sys::Tox, user_data: *mut ::std::os::raw::c_void) {
        tox_iterate(tox, user_data)
    }

    unsafe fn iteration_interval(&self, tox: *const toxcore_sys::Tox) -> u32 {
        tox_iteration_interval(tox)
    }

    unsafe fn get_savedata_size(&self, tox: *const toxcore_sys::Tox) -> u64 {
        tox_get_savedata_size(tox)
    }

    unsafe fn get_savedata(&self, tox: *const toxcore_sys::Tox, savedata: *mut u8) {
        tox_get_savedata(tox, savedata)
    }

    unsafe fn public_key_size(&self) -> u32 {
        tox_public_key_size()
    }

    unsafe fn self_get_public_key(&self, tox: *const toxcore_sys::Tox, public_key: *mut u8) {
        tox_self_get_public_key(tox, public_key);
    }

    unsafe fn secret_key_size(&self) -> u32 {
        tox_secret_key_size()
    }

    unsafe fn self_get_secret_key(&self, tox: *const toxcore_sys::Tox, secret_key: *mut u8) {
        tox_self_get_secret_key(tox, secret_key);
    }

    unsafe fn address_size(&self) -> u32 {
        tox_address_size()
    }

    unsafe fn self_get_address(&self, tox: *const toxcore_sys::Tox, address: *mut u8) {
        tox_self_get_address(tox, address);
    }

    unsafe fn self_get_name_size(&self, tox: *const toxcore_sys::Tox) -> u64 {
        tox_self_get_name_size(tox)
    }

    unsafe fn self_get_name(&self, tox: *const toxcore_sys::Tox, name: *mut u8) {
        tox_self_get_name(tox, name)
    }

    unsafe fn self_set_name(
        &self,
        tox: *mut toxcore_sys::Tox,
        name: *const u8,
        length: u64,
        error: *mut TOX_ERR_SET_INFO,
    ) -> bool {
        tox_self_set_name(tox, name, length, error)
    }

    unsafe fn self_get_friend_list_size(&self, tox: *const toxcore_sys::Tox) -> u64 {
        tox_self_get_friend_list_size(tox)
    }

    unsafe fn self_get_friend_list(&self, tox: *const toxcore_sys::Tox, friend_list: *mut u32) {
        tox_self_get_friend_list(tox, friend_list)
    }

    unsafe fn friend_add_norequest(
        &self,
        tox: *mut toxcore_sys::Tox,
        public_key: *const u8,
        error: *mut TOX_ERR_FRIEND_ADD,
    ) -> u32 {
        tox_friend_add_norequest(tox, public_key, error)
    }

    unsafe fn friend_get_public_key(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        public_key: *mut u8,
        error: *mut TOX_ERR_FRIEND_GET_PUBLIC_KEY,
    ) -> bool {
        tox_friend_get_public_key(tox, friend_number, public_key, error)
    }

    unsafe fn friend_get_name_size(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> u64 {
        tox_friend_get_name_size(tox, friend_number, error)
    }

    unsafe fn friend_get_name(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        name: *mut u8,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> bool {
        tox_friend_get_name(tox, friend_number, name, error)
    }

    unsafe fn max_message_length(&self) -> u32 {
        tox_max_message_length()
    }

    unsafe fn friend_send_message(
        &self,
        tox: *mut toxcore_sys::Tox,
        friend_number: u32,
        type_: TOX_MESSAGE_TYPE,
        message: *const u8,
        length: size_t,
        error: *mut TOX_ERR_FRIEND_SEND_MESSAGE,
    ) -> u32 {
        tox_friend_send_message(tox, friend_number, type_, message, length, error)
    }

    unsafe fn friend_get_status(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> TOX_USER_STATUS {
        tox_friend_get_status(tox, friend_number, error)
    }

    unsafe fn friend_get_connection_status(
        &self,
        tox: *const toxcore_sys::Tox,
        friend_number: u32,
        error: *mut TOX_ERR_FRIEND_QUERY,
    ) -> TOX_CONNECTION {
        tox_friend_get_connection_status(tox, friend_number, error)
    }

    unsafe fn callback_friend_request(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_request_cb,
    ) {
        tox_callback_friend_request(tox, callback)
    }

    unsafe fn callback_friend_message(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_message_cb,
    ) {
        tox_callback_friend_message(tox, callback)
    }

    unsafe fn callback_friend_read_receipt(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_read_receipt_cb,
    ) {
        tox_callback_friend_read_receipt(tox, callback)
    }

    unsafe fn callback_friend_status(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_status_cb,
    ) {
        tox_callback_friend_status(tox, callback)
    }

    unsafe fn callback_friend_connection_status(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_connection_status_cb,
    ) {
        tox_callback_friend_connection_status(tox, callback)
    }

    unsafe fn callback_friend_name(
        &self,
        tox: *mut toxcore_sys::Tox,
        callback: tox_friend_name_cb,
    ) {
        tox_callback_friend_name(tox, callback)
    }
}

#[automock]
pub trait ToxOptionsApi {
    #[allow(clippy::new_ret_no_self)]
    unsafe fn new(&self, err: *mut TOX_ERR_OPTIONS_NEW) -> *mut Tox_Options;
    unsafe fn free(&self, options: *mut Tox_Options);
    unsafe fn set_ipv6_enabled(&self, options: *mut Tox_Options, ipv6_enabled: bool);
    unsafe fn set_udp_enabled(&self, options: *mut Tox_Options, udp_enabled: bool);
    unsafe fn set_local_discovery_enabled(
        &self,
        options: *mut Tox_Options,
        local_discovery_enabled: bool,
    );
    unsafe fn set_proxy_type(&self, options: *mut Tox_Options, type_: TOX_PROXY_TYPE);
    unsafe fn set_proxy_host(&self, options: *mut Tox_Options, host: *const ::std::os::raw::c_char);
    unsafe fn set_proxy_port(&self, options: *mut Tox_Options, port: u16);
    unsafe fn set_start_port(&self, options: *mut Tox_Options, start_port: u16);
    unsafe fn set_end_port(&self, options: *mut Tox_Options, end_port: u16);
    unsafe fn set_tcp_port(&self, options: *mut Tox_Options, tcp_port: u16);
    unsafe fn set_hole_punching_enabled(
        &self,
        options: *mut Tox_Options,
        hole_punching_enabled: bool,
    );
    unsafe fn set_savedata_type(&self, options: *mut Tox_Options, type_: TOX_SAVEDATA_TYPE);
    unsafe fn set_savedata_data(&self, options: *mut Tox_Options, data: *const u8, length: size_t);
    unsafe fn set_log_callback(&self, options: *mut Tox_Options, callback: tox_log_cb);
    unsafe fn set_experimental_thread_safety(&self, options: *mut Tox_Options, thread_safety: bool);
}

pub(crate) struct ToxOptionsSys;

impl ToxOptionsApi for ToxOptionsSys {
    unsafe fn new(&self, err: *mut TOX_ERR_OPTIONS_NEW) -> *mut Tox_Options {
        tox_options_new(err)
    }

    unsafe fn free(&self, options: *mut Tox_Options) {
        tox_options_free(options)
    }

    unsafe fn set_ipv6_enabled(&self, options: *mut Tox_Options, ipv6_enabled: bool) {
        tox_options_set_ipv6_enabled(options, ipv6_enabled)
    }
    unsafe fn set_udp_enabled(&self, options: *mut Tox_Options, udp_enabled: bool) {
        tox_options_set_udp_enabled(options, udp_enabled)
    }

    unsafe fn set_local_discovery_enabled(
        &self,
        options: *mut Tox_Options,
        local_discovery_enabled: bool,
    ) {
        tox_options_set_local_discovery_enabled(options, local_discovery_enabled)
    }

    unsafe fn set_proxy_type(&self, options: *mut Tox_Options, type_: TOX_PROXY_TYPE) {
        tox_options_set_proxy_type(options, type_)
    }

    unsafe fn set_proxy_host(
        &self,
        options: *mut Tox_Options,
        host: *const ::std::os::raw::c_char,
    ) {
        tox_options_set_proxy_host(options, host);
    }

    unsafe fn set_proxy_port(&self, options: *mut Tox_Options, port: u16) {
        tox_options_set_proxy_port(options, port)
    }

    unsafe fn set_start_port(&self, options: *mut Tox_Options, start_port: u16) {
        tox_options_set_start_port(options, start_port)
    }

    unsafe fn set_end_port(&self, options: *mut Tox_Options, end_port: u16) {
        tox_options_set_end_port(options, end_port)
    }

    unsafe fn set_tcp_port(&self, options: *mut Tox_Options, tcp_port: u16) {
        tox_options_set_tcp_port(options, tcp_port)
    }

    unsafe fn set_hole_punching_enabled(
        &self,
        options: *mut Tox_Options,
        hole_punching_enabled: bool,
    ) {
        tox_options_set_hole_punching_enabled(options, hole_punching_enabled)
    }

    unsafe fn set_savedata_type(&self, options: *mut Tox_Options, type_: TOX_SAVEDATA_TYPE) {
        tox_options_set_savedata_type(options, type_)
    }

    unsafe fn set_savedata_data(&self, options: *mut Tox_Options, data: *const u8, length: size_t) {
        tox_options_set_savedata_data(options, data, length)
    }

    unsafe fn set_log_callback(&self, options: *mut Tox_Options, callback: tox_log_cb) {
        tox_options_set_log_callback(options, callback)
    }

    unsafe fn set_experimental_thread_safety(
        &self,
        options: *mut Tox_Options,
        thread_safety: bool,
    ) {
        tox_options_set_experimental_thread_safety(options, thread_safety)
    }
}

#[automock]
pub trait ToxEncryptSaveApi: Send + Sync {
    unsafe fn pass_key_free(&self, key: *mut Tox_Pass_Key);
    unsafe fn pass_key_derive(
        &self,
        passphrase: *const u8,
        len: u64,
        err: *mut TOX_ERR_KEY_DERIVATION,
    ) -> *mut Tox_Pass_Key;
    unsafe fn pass_key_derive_with_salt(
        &self,
        passphrase: *const u8,
        len: u64,
        salt: *const u8,
        err: *mut TOX_ERR_KEY_DERIVATION,
    ) -> *mut Tox_Pass_Key;
    unsafe fn pass_key_encrypt(
        &self,
        key: *const Tox_Pass_Key,
        plaintext: *const u8,
        plaintext_len: u64,
        ciphertext: *mut u8,
        err: *mut TOX_ERR_ENCRYPTION,
    ) -> bool;
    unsafe fn pass_key_decrypt(
        &self,
        key: *const Tox_Pass_Key,
        ciphertext: *const u8,
        ciphertext_len: u64,
        plaintext: *mut u8,
        err: *mut TOX_ERR_DECRYPTION,
    ) -> bool;
    unsafe fn get_salt(
        &self,
        ciphertext: *const u8,
        salt: *mut u8,
        err: *mut TOX_ERR_GET_SALT,
    ) -> bool;
}

pub struct ToxEncryptSaveImpl;

impl ToxEncryptSaveApi for ToxEncryptSaveImpl {
    unsafe fn pass_key_free(&self, key: *mut Tox_Pass_Key) {
        tox_pass_key_free(key)
    }

    unsafe fn pass_key_derive(
        &self,
        passphrase: *const u8,
        len: u64,
        err: *mut TOX_ERR_KEY_DERIVATION,
    ) -> *mut Tox_Pass_Key {
        tox_pass_key_derive(passphrase, len, err)
    }

    unsafe fn pass_key_derive_with_salt(
        &self,
        passphrase: *const u8,
        len: u64,
        salt: *const u8,
        err: *mut TOX_ERR_KEY_DERIVATION,
    ) -> *mut Tox_Pass_Key {
        tox_pass_key_derive_with_salt(passphrase, len, salt, err)
    }

    unsafe fn pass_key_encrypt(
        &self,
        key: *const Tox_Pass_Key,
        plaintext: *const u8,
        plaintext_len: u64,
        ciphertext: *mut u8,
        err: *mut TOX_ERR_ENCRYPTION,
    ) -> bool {
        tox_pass_key_encrypt(key, plaintext, plaintext_len, ciphertext, err)
    }

    unsafe fn pass_key_decrypt(
        &self,
        key: *const Tox_Pass_Key,
        ciphertext: *const u8,
        ciphertext_len: u64,
        plaintext: *mut u8,
        err: *mut TOX_ERR_DECRYPTION,
    ) -> bool {
        tox_pass_key_decrypt(key, ciphertext, ciphertext_len, plaintext, err)
    }

    unsafe fn get_salt(
        &self,
        ciphertext: *const u8,
        salt: *mut u8,
        err: *mut TOX_ERR_GET_SALT,
    ) -> bool {
        tox_get_salt(ciphertext, salt, err)
    }
}
