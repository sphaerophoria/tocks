use crate::{error::*, tox::ToxEventCallback, Event};
use crate::{sys, tox::Tox, ProxyType, SaveData};

use paste::paste;

use toxcore_sys::*;

use std::{
    ffi::{CStr, CString, NulError},
    pin::Pin,
};

macro_rules! impl_builder_option {
    ($field_name: ident, $tag: ident, $type:ty) => {
        impl_builder_option!($field_name, $field_name, $tag, $type);
    };
    ($field_name: ident, $exposed_name: ident, $tag: ident, $type:ty) => {
        pub fn $exposed_name(self, $tag: $type) -> Self {
            unsafe {
                paste! {
                    sys::[<tox_options_set_ $field_name>](self.options, $tag);
                    self
                }
            }
        }
    };
}

macro_rules! impl_bool_builder_option {
    ($field: ident) => {
        paste! {
            impl_builder_option!([<$field _ enabled>], $field, $field, bool);
        }
    };
}

pub struct ToxBuilder {
    options: *mut Tox_Options,
    event_callback: Option<ToxEventCallback>,
    savedata: SaveData,
    log: bool,
}

impl ToxBuilder {
    pub(crate) fn new() -> Result<ToxBuilder, ToxBuilderCreationError> {
        let mut err = TOX_ERR_OPTIONS_NEW_OK;

        let options = unsafe { sys::tox_options_new(&mut err as *mut TOX_ERR_OPTIONS_NEW) };
        if err != TOX_ERR_OPTIONS_NEW_OK {
            return Err(ToxBuilderCreationError);
        }

        Ok(ToxBuilder {
            options,
            event_callback: None,
            savedata: SaveData::None,
            log: false,
        })
    }

    impl_bool_builder_option!(ipv6);
    impl_bool_builder_option!(udp);
    impl_bool_builder_option!(local_discovery);
    impl_builder_option!(proxy_port, port, u16);
    impl_builder_option!(start_port, port, u16);
    impl_builder_option!(end_port, port, u16);
    impl_builder_option!(tcp_port, port, u16);
    impl_bool_builder_option!(hole_punching);
    impl_builder_option!(experimental_thread_safety, enabled, bool);

    pub fn proxy_type(self, t: ProxyType) -> Self {
        let c_type = match t {
            ProxyType::None => TOX_PROXY_TYPE_NONE,
            ProxyType::Http => TOX_PROXY_TYPE_HTTP,
            ProxyType::Socks5 => TOX_PROXY_TYPE_SOCKS5,
        };

        unsafe {
            sys::tox_options_set_proxy_type(self.options, c_type);
        }
        self
    }

    pub fn proxy_host(self, host: &str) -> Result<Self, NulError> {
        let cstr = CString::new(host)?;
        unsafe {
            sys::tox_options_set_proxy_host(self.options, cstr.as_ptr());
        }
        Ok(self)
    }

    pub fn savedata(mut self, data: SaveData) -> Self {
        self.savedata = data;
        self
    }

    pub fn log(mut self, enable: bool) -> Self {
        self.log = enable;
        self
    }

    pub fn event_callback<F: FnMut(Event) + 'static>(mut self, callback: F) -> Self {
        self.event_callback = Some(Box::new(callback));
        self
    }

    fn map_err_new(err: TOX_ERR_NEW) -> ToxCreationError {
        match err {
            TOX_ERR_NEW_NULL => return ToxCreationError::Null,
            TOX_ERR_NEW_MALLOC => return ToxCreationError::Malloc,
            TOX_ERR_NEW_PORT_ALLOC => return ToxCreationError::PortAlloc,
            TOX_ERR_NEW_PROXY_BAD_TYPE => return ToxCreationError::BadProxyType,
            TOX_ERR_NEW_PROXY_BAD_HOST => return ToxCreationError::BadProxyHost,
            TOX_ERR_NEW_PROXY_BAD_PORT => return ToxCreationError::BadProxyPort,
            TOX_ERR_NEW_PROXY_NOT_FOUND => return ToxCreationError::ProxyNotFound,
            TOX_ERR_NEW_LOAD_ENCRYPTED => return ToxCreationError::LoadEncrypted,
            TOX_ERR_NEW_LOAD_BAD_FORMAT => return ToxCreationError::BadLoadFormat,
            _ => return ToxCreationError::Unknown,
        }
    }

    fn map_err_toxav_new(err: TOXAV_ERR_NEW) -> ToxCreationError {
        match err {
            TOXAV_ERR_NEW_NULL => return ToxCreationError::Null,
            TOXAV_ERR_NEW_MALLOC => return ToxCreationError::Malloc,
            TOXAV_ERR_NEW_MULTIPLE => return ToxCreationError::Multiple,
            _ => return ToxCreationError::Unknown,
        }
    }

    /// Create the [`Tox`] instance
    pub fn build(mut self) -> Result<Tox, ToxBuildError> {
        if self.log {
            unsafe {
                sys::tox_options_set_log_callback(self.options, Some(tox_log_callback));
            }
        }

        let data = Pin::new(&self.savedata);

        match &*data {
            SaveData::ToxSave(data) => unsafe {
                sys::tox_options_set_savedata_type(self.options, TOX_SAVEDATA_TYPE_TOX_SAVE);
                sys::tox_options_set_savedata_data(self.options, data.as_ptr(), data.len() as u64);
            },
            SaveData::SecretKey(data) => unsafe {
                sys::tox_options_set_savedata_type(self.options, TOX_SAVEDATA_TYPE_SECRET_KEY);
                sys::tox_options_set_savedata_data(self.options, data.as_ptr(), data.len() as u64);
            },
            SaveData::None => {}
        }

        let mut err = TOX_ERR_NEW_OK;
        let sys_tox = unsafe { sys::tox_new(self.options, &mut err) };

        if err != TOX_ERR_NEW_OK {
            return Err(From::from(Self::map_err_new(err)));
        }

        let mut event_callback = None;
        std::mem::swap(&mut event_callback, &mut self.event_callback);

        let mut err = TOXAV_ERR_NEW_OK;
        let av = unsafe { sys::toxav_new(sys_tox, &mut err) };

        if err != TOXAV_ERR_NEW_OK {
            unsafe {
                sys::tox_kill(sys_tox);
            }
            return Err(From::from(Self::map_err_toxav_new(err)));
        }

        let ret = Tox::new(sys_tox, av, event_callback);

        Ok(ret)
    }
}

impl Drop for ToxBuilder {
    fn drop(&mut self) {
        unsafe {
            sys::tox_options_free(self.options);
        }
    }
}

/// Converts from tox log levels to [`log::Level`]
fn convert_tox_log_level(level: TOX_LOG_LEVEL) -> Result<log::Level, ()> {
    use log::Level;

    match level {
        TOX_LOG_LEVEL_TRACE => Ok(Level::Trace),
        TOX_LOG_LEVEL_DEBUG => Ok(Level::Debug),
        TOX_LOG_LEVEL_INFO => Ok(Level::Info),
        TOX_LOG_LEVEL_WARNING => Ok(Level::Warn),
        TOX_LOG_LEVEL_ERROR => Ok(Level::Error),
        _ => Err(()),
    }
}

/// Callback function provided to toxcore for logging
///
/// Adapts the toxcore log into a log compatible with the log crate
///
/// The user_data field here is a different pointer than the ones used in other
/// other callbacks
pub(crate) unsafe extern "C" fn tox_log_callback(
    _tox: *mut toxcore_sys::Tox,
    level: TOX_LOG_LEVEL,
    file: *const std::os::raw::c_char,
    line: u32,
    func: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
    _user_data: *mut std::os::raw::c_void,
) {
    let level = match convert_tox_log_level(level) {
        Ok(level) => level,
        Err(_) => return,
    };

    let file_string = CStr::from_ptr(file).to_string_lossy();
    let message_string = CStr::from_ptr(message).to_string_lossy().to_string();
    let func_string = CStr::from_ptr(func).to_string_lossy().to_string();

    let metadata = log::Metadata::builder().level(level).target("tox").build();

    log::logger().log(
        &log::Record::builder()
            .level(level)
            .target("tox")
            .metadata(metadata)
            .file(Some(file_string.as_ref()))
            .line(Some(line))
            .args(format_args!("{}: {}", func_string, message_string))
            .build(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    struct ToxApiFixture {
        _callback_friend_request_ctx: sys::__tox_callback_friend_request::Context,
        _callback_friend_message_ctx: sys::__tox_callback_friend_message::Context,
        _callback_friend_read_receipt_ctx: sys::__tox_callback_friend_read_receipt::Context,
        _callback_friend_status_ctx: sys::__tox_callback_friend_status::Context,
        _callback_friend_connection_status_ctx:
            sys::__tox_callback_friend_connection_status::Context,
        _callback_friend_name_ctx: sys::__tox_callback_friend_name::Context,
        _kill_ctx: sys::__tox_kill::Context,
        _av_kill_ctx: sys::__toxav_kill::Context,
        _new_ctx: sys::__tox_new::Context,
        _av_new_ctx: sys::__toxav_new::Context,
    }

    fn generate_tox_api_mock() -> ToxApiFixture {
        let callback_friend_request_ctx = sys::tox_callback_friend_request_context();
        callback_friend_request_ctx.expect().return_const(());

        let callback_friend_message_ctx = sys::tox_callback_friend_message_context();
        callback_friend_message_ctx.expect().return_const(());

        let callback_friend_read_receipt_ctx = sys::tox_callback_friend_read_receipt_context();
        callback_friend_read_receipt_ctx.expect().return_const(());

        let callback_friend_status_ctx = sys::tox_callback_friend_status_context();
        callback_friend_status_ctx.expect().return_const(());

        let callback_friend_connection_status_ctx =
            sys::tox_callback_friend_connection_status_context();
        callback_friend_connection_status_ctx
            .expect()
            .return_const(());

        let callback_friend_name_ctx = sys::tox_callback_friend_name_context();
        callback_friend_name_ctx.expect().return_const(());

        let kill_ctx = sys::tox_kill_context();
        kill_ctx.expect().return_const(());

        let av_kill_ctx = sys::toxav_kill_context();
        av_kill_ctx.expect().return_const(());

        let new_ctx = sys::tox_new_context();
        new_ctx.expect().returning_st(|_, _| std::ptr::null_mut());

        let av_new_ctx = sys::toxav_new_context();
        av_new_ctx
            .expect()
            .returning_st(|_, _| std::ptr::null_mut());

        ToxApiFixture {
            _callback_friend_request_ctx: callback_friend_request_ctx,
            _callback_friend_message_ctx: callback_friend_message_ctx,
            _callback_friend_read_receipt_ctx: callback_friend_read_receipt_ctx,
            _callback_friend_status_ctx: callback_friend_status_ctx,
            _callback_friend_connection_status_ctx: callback_friend_connection_status_ctx,
            _callback_friend_name_ctx: callback_friend_name_ctx,
            _kill_ctx: kill_ctx,
            _av_kill_ctx: av_kill_ctx,
            _new_ctx: new_ctx,
            _av_new_ctx: av_new_ctx,
        }
    }
    struct BuilderFixture {
        builder: ToxBuilder,
        _options_new_ctx: sys::__tox_options_new::Context,
        _options_free_ctx: sys::__tox_options_free::Context,
    }

    impl BuilderFixture {
        fn new() -> Result<BuilderFixture, Box<dyn std::error::Error>> {
            let options_new_ctx = sys::tox_options_new_context();
            options_new_ctx
                .expect()
                .returning_st(|_| 0xdeadbeef as *mut Tox_Options);

            let options_free_ctx = sys::tox_options_free_context();
            options_free_ctx.expect().return_const(());

            Ok(BuilderFixture {
                builder: ToxBuilder::new()?,
                _options_new_ctx: options_new_ctx,
                _options_free_ctx: options_free_ctx,
            })
        }
    }

    macro_rules! test_builder_options {
        ($test_name:ident, $rust_name:ident, $rust_val:expr, $mock_name:ident, $c_val:expr) => {
            paste! {
                rusty_fork::rusty_fork_test! {
                #[test]
                fn [<test_options_ $test_name>]() -> Result<(), Box<dyn std::error::Error>>
                {
                    let ctx = sys::[<tox_options_set_ $mock_name _context>]();
                    ctx.expect()
                        .withf_st(|_, v| *v == $c_val)
                        .return_const(())
                        .once();

                    let fixture = BuilderFixture::new()?;

                    fixture.builder
                        .$rust_name($rust_val);

                    Ok(())
                }
                }
            }
        };
        ($test_name:ident, $rust_name:ident, $c_name:ident, $val: expr) => {
            test_builder_options!($test_name, $rust_name, $val, $c_name, $val);
        };
    }

    macro_rules! test_bool_option {
        ($name:ident) => {
            paste! {
                test_builder_options!([<$name _enable>], $name, [<$name _enabled>], true);
                test_builder_options!([<$name _disable>], $name, [<$name _enabled>], false);
            }
        };
    }

    rusty_fork::rusty_fork_test! {
        #[test]
        fn test_builder_creation_failure() {
            let options_new_ctx = sys::tox_options_new_context();
            options_new_ctx.expect().returning_st(|err| {
                unsafe { *err = TOX_ERR_OPTIONS_NEW_MALLOC };
                std::ptr::null_mut()
            });

            assert!(ToxBuilder::new().is_err());
        }

        #[test]
        fn test_av_creation_error() -> Result<(), Box<dyn std::error::Error>> {

            let fixture = BuilderFixture::new()?;
            let tox_fixture = generate_tox_api_mock();

            tox_fixture._kill_ctx.checkpoint();
            tox_fixture._av_new_ctx.checkpoint();

            tox_fixture._av_new_ctx.expect()
                .once()
                .returning_st(|_, err| {
                    unsafe { *err = TOXAV_ERR_NEW_MALLOC };
                    std::ptr::null_mut()
                });

            tox_fixture._kill_ctx.expect()
                .once()
                .return_const_st(());

            assert!(fixture.builder.build().is_err());


            Ok(())
        }
    }

    test_bool_option!(ipv6);
    test_bool_option!(udp);
    test_bool_option!(local_discovery);
    test_builder_options!(
        proxy_type_none,
        proxy_type,
        ProxyType::None,
        proxy_type,
        TOX_PROXY_TYPE_NONE
    );
    test_builder_options!(
        proxy_type_http,
        proxy_type,
        ProxyType::Http,
        proxy_type,
        TOX_PROXY_TYPE_HTTP
    );
    test_builder_options!(
        proxy_type_socks,
        proxy_type,
        ProxyType::Socks5,
        proxy_type,
        TOX_PROXY_TYPE_SOCKS5
    );
    test_builder_options!(proxy_port, proxy_port, 1337, proxy_port, 1337);
    test_builder_options!(start_port, start_port, 1337, start_port, 1337);
    test_builder_options!(end_port, end_port, 1337, end_port, 1337);
    test_builder_options!(tcp_port, tcp_port, 1337, tcp_port, 1337);
    test_bool_option!(hole_punching);
    test_builder_options!(
        experimental_thread_safety,
        experimental_thread_safety,
        true,
        experimental_thread_safety,
        true
    );

    rusty_fork::rusty_fork_test! {

        #[test]
        fn test_proxy_host_success() -> Result<(), Box<dyn std::error::Error>> {
            let set_proxy_host_ctx = sys::tox_options_set_proxy_host_context();
            set_proxy_host_ctx.expect()
                .withf_st(|_, v| unsafe { CStr::from_ptr(*v).to_string_lossy() == "test" })
                .return_const(())
                .once();

            BuilderFixture::new()?.builder.proxy_host("test")?;

            Ok(())
        }

        #[test]
        fn test_proxy_host_failure() -> Result<(), Box<dyn std::error::Error>> {
            assert!(BuilderFixture::new()?
                .builder
                .proxy_host("\0 \0 \0")
                .is_err());

            Ok(())
        }

        #[test]
        fn test_savedata_tox_save() -> Result<(), Box<dyn std::error::Error>> {
            let savedata = "test".to_string().into_bytes();

            let set_savedata_type_ctx = sys::tox_options_set_savedata_type_context();
            set_savedata_type_ctx.expect()
                .withf_st(|_, v| *v == TOX_SAVEDATA_TYPE_TOX_SAVE)
                .return_const(())
                .once();

            let savedata_clone = savedata.clone();

            let set_savedata_data_ctx = sys::tox_options_set_savedata_data_context();
            set_savedata_data_ctx.expect()
                .withf_st(move |_, data, len| unsafe {
                    std::slice::from_raw_parts(*data, *len as usize) == &savedata_clone
                })
                .return_const(())
                .once();

            let fixture = BuilderFixture::new()?;

            let _tox_mock = generate_tox_api_mock();

            fixture.builder.savedata(SaveData::ToxSave(savedata)).build().unwrap();

            Ok(())
        }

        #[test]
        fn test_savedata_secret_key() -> Result<(), Box<dyn std::error::Error>> {
            let set_savedata_type_ctx = sys::tox_options_set_savedata_type_context();
    set_savedata_type_ctx.expect()
                .withf_st(|_, v| *v == TOX_SAVEDATA_TYPE_SECRET_KEY)
                .return_const(())
                .once();

            let savedata = "key".chars().map(|c| c as u8).collect::<Vec<u8>>();

            let savedata_clone = savedata.clone();

            let set_savedata_data_ctx = sys::tox_options_set_savedata_data_context();
    set_savedata_data_ctx.expect()
                .withf_st(move |_, data, len| unsafe {
                    std::slice::from_raw_parts(*data, *len as usize) == &savedata_clone
                })
                .return_const(())
                .once();

            let _tox_mock = generate_tox_api_mock();
            let fixture = BuilderFixture::new()?;

            fixture.builder.savedata(SaveData::SecretKey(savedata)).build().unwrap();

            Ok(())
        }

        #[test]
        fn test_logger_enabled() -> Result<(), Box<dyn std::error::Error>> {
            let set_log_callback_ctx = sys::tox_options_set_log_callback_context();
            set_log_callback_ctx.expect()
                .withf_st(|_, cb| *cb == Some(tox_log_callback))
                .return_const(())
                .once();

            {
                let _tox_mock = generate_tox_api_mock();
                let fixture = BuilderFixture::new()?;
                fixture.builder.log(true).build()?;
            }

            {
                let _tox_mock = generate_tox_api_mock();
                // The second iteration should fail due to the newly injected mock
                let fixture = BuilderFixture::new()?;
                fixture.builder.log(false).build()?;
            }

            Ok(())
        }

        #[test]
        fn test_convert_log_level() -> Result<(), ()> {
            use log::Level;
            assert_eq!(convert_tox_log_level(TOX_LOG_LEVEL_ERROR)?, Level::Error);
            assert_eq!(convert_tox_log_level(TOX_LOG_LEVEL_WARNING)?, Level::Warn);
            assert_eq!(convert_tox_log_level(TOX_LOG_LEVEL_INFO)?, Level::Info);
            assert_eq!(convert_tox_log_level(TOX_LOG_LEVEL_DEBUG)?, Level::Debug);
            assert_eq!(convert_tox_log_level(TOX_LOG_LEVEL_TRACE)?, Level::Trace);

            Ok(())
        }
        }
}
