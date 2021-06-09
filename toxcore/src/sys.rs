//! Wrapper trait/implementation for toxcore APIs. toxcore_sys APIs are wrapped
//! as traits to allow for mocking/testing

#[cfg_attr(test, mockall::automock)]
mod api_impl {
    extern "C" {
        #![cfg_attr(test, allow(unused))]
        pub fn tox_new(
            options: *const toxcore_sys::Tox_Options,
            error: *mut toxcore_sys::TOX_ERR_NEW,
        ) -> *mut toxcore_sys::Tox;
        pub fn tox_kill(tox: *mut toxcore_sys::Tox);
        pub fn tox_iterate(tox: *mut toxcore_sys::Tox, user_data: *mut ::std::os::raw::c_void);
        pub fn tox_iteration_interval(tox: *const toxcore_sys::Tox) -> u32;
        pub fn tox_get_savedata_size(tox: *const toxcore_sys::Tox) -> u64;
        pub fn tox_get_savedata(tox: *const toxcore_sys::Tox, savedata: *mut u8);
        pub fn tox_public_key_size() -> u32;
        pub fn tox_self_get_public_key(tox: *const toxcore_sys::Tox, public_key: *mut u8);
        pub fn tox_secret_key_size() -> u32;
        pub fn tox_self_get_secret_key(tox: *const toxcore_sys::Tox, secret_key: *mut u8);
        pub fn tox_address_size() -> u32;
        pub fn tox_self_get_address(tox: *const toxcore_sys::Tox, address: *mut u8);
        pub fn tox_self_get_name_size(tox: *const toxcore_sys::Tox) -> u64;
        pub fn tox_self_get_name(tox: *const toxcore_sys::Tox, name: *mut u8);
        pub fn tox_self_set_name(
            tox: *mut toxcore_sys::Tox,
            name: *const u8,
            length: u64,
            error: *mut toxcore_sys::TOX_ERR_SET_INFO,
        ) -> bool;
        pub fn tox_self_get_friend_list_size(tox: *const toxcore_sys::Tox) -> u64;
        pub fn tox_self_get_friend_list(tox: *const toxcore_sys::Tox, friend_list: *mut u32);
        pub fn tox_friend_add_norequest(
            tox: *mut toxcore_sys::Tox,
            public_key: *const u8,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_ADD,
        ) -> u32;
        pub fn tox_friend_get_public_key(
            tox: *const toxcore_sys::Tox,
            friend_number: u32,
            public_key: *mut u8,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_GET_PUBLIC_KEY,
        ) -> bool;
        pub fn tox_friend_get_name_size(
            tox: *const toxcore_sys::Tox,
            friend_number: u32,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_QUERY,
        ) -> u64;
        pub fn tox_friend_get_name(
            tox: *const toxcore_sys::Tox,
            friend_number: u32,
            name: *mut u8,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_QUERY,
        ) -> bool;
        pub fn tox_max_message_length() -> u32;
        pub fn tox_friend_send_message(
            tox: *mut toxcore_sys::Tox,
            friend_number: u32,
            type_: toxcore_sys::TOX_MESSAGE_TYPE,
            message: *const u8,
            length: u64,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_SEND_MESSAGE,
        ) -> u32;
        pub fn tox_friend_get_status(
            tox: *const toxcore_sys::Tox,
            friend_number: u32,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_QUERY,
        ) -> toxcore_sys::TOX_USER_STATUS;
        pub fn tox_friend_get_connection_status(
            tox: *const toxcore_sys::Tox,
            friend_number: u32,
            error: *mut toxcore_sys::TOX_ERR_FRIEND_QUERY,
        ) -> toxcore_sys::TOX_CONNECTION;
        pub fn tox_callback_friend_request(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_request_cb,
        );
        pub fn tox_callback_friend_message(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_message_cb,
        );
        pub fn tox_callback_friend_read_receipt(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_read_receipt_cb,
        );
        pub fn tox_callback_friend_status(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_status_cb,
        );
        pub fn tox_callback_friend_connection_status(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_connection_status_cb,
        );
        pub fn tox_callback_friend_name(
            tox: *mut toxcore_sys::Tox,
            callback: toxcore_sys::tox_friend_name_cb,
        );

        pub fn tox_options_new(
            err: *mut toxcore_sys::TOX_ERR_OPTIONS_NEW,
        ) -> *mut toxcore_sys::Tox_Options;
        pub fn tox_options_free(options: *mut toxcore_sys::Tox_Options);
        pub fn tox_options_set_ipv6_enabled(
            options: *mut toxcore_sys::Tox_Options,
            ipv6_enabled: bool,
        );
        pub fn tox_options_set_udp_enabled(
            options: *mut toxcore_sys::Tox_Options,
            udp_enabled: bool,
        );
        pub fn tox_options_set_local_discovery_enabled(
            options: *mut toxcore_sys::Tox_Options,
            local_discovery_enabled: bool,
        );
        pub fn tox_options_set_proxy_type(
            options: *mut toxcore_sys::Tox_Options,
            type_: toxcore_sys::TOX_PROXY_TYPE,
        );
        pub fn tox_options_set_proxy_host(
            options: *mut toxcore_sys::Tox_Options,
            host: *const ::std::os::raw::c_char,
        );
        pub fn tox_options_set_proxy_port(options: *mut toxcore_sys::Tox_Options, port: u16);
        pub fn tox_options_set_start_port(options: *mut toxcore_sys::Tox_Options, start_port: u16);
        pub fn tox_options_set_end_port(options: *mut toxcore_sys::Tox_Options, end_port: u16);
        pub fn tox_options_set_tcp_port(options: *mut toxcore_sys::Tox_Options, tcp_port: u16);
        pub fn tox_options_set_hole_punching_enabled(
            options: *mut toxcore_sys::Tox_Options,
            hole_punching_enabled: bool,
        );
        pub fn tox_options_set_savedata_type(
            options: *mut toxcore_sys::Tox_Options,
            type_: toxcore_sys::TOX_SAVEDATA_TYPE,
        );
        pub fn tox_options_set_savedata_data(
            options: *mut toxcore_sys::Tox_Options,
            data: *const u8,
            length: u64,
        );
        pub fn tox_options_set_log_callback(
            options: *mut toxcore_sys::Tox_Options,
            callback: toxcore_sys::tox_log_cb,
        );
        pub fn tox_options_set_experimental_thread_safety(
            options: *mut toxcore_sys::Tox_Options,
            thread_safety: bool,
        );
        pub fn tox_pass_key_free(key: *mut toxcore_sys::Tox_Pass_Key);
        pub fn tox_pass_key_derive(
            passphrase: *const u8,
            len: u64,
            err: *mut toxcore_sys::TOX_ERR_KEY_DERIVATION,
        ) -> *mut toxcore_sys::Tox_Pass_Key;
        pub fn tox_pass_key_derive_with_salt(
            passphrase: *const u8,
            len: u64,
            salt: *const u8,
            err: *mut toxcore_sys::TOX_ERR_KEY_DERIVATION,
        ) -> *mut toxcore_sys::Tox_Pass_Key;
        pub fn tox_pass_key_encrypt(
            key: *const toxcore_sys::Tox_Pass_Key,
            plaintext: *const u8,
            plaintext_len: u64,
            ciphertext: *mut u8,
            err: *mut toxcore_sys::TOX_ERR_ENCRYPTION,
        ) -> bool;
        pub fn tox_pass_key_decrypt(
            key: *const toxcore_sys::Tox_Pass_Key,
            ciphertext: *const u8,
            ciphertext_len: u64,
            plaintext: *mut u8,
            err: *mut toxcore_sys::TOX_ERR_DECRYPTION,
        ) -> bool;
        pub fn tox_get_salt(
            ciphertext: *const u8,
            salt: *mut u8,
            err: *mut toxcore_sys::TOX_ERR_GET_SALT,
        ) -> bool;
    }
}

#[mockall_double::double]
use api_impl as api;

pub use api::*;
