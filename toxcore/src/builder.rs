use crate::error::*;
use crate::{
    sys::{ToxApi, ToxApiImpl, ToxOptionsApi, ToxOptionsSys},
    tox::{Tox, ToxImpl},
    ProxyType, SaveData,
};

use paste::paste;

use toxcore_sys::*;

use std::ffi::{CStr, CString, NulError};

macro_rules! impl_builder_option {
    ($field_name: ident, $tag: ident, $type:ty) => {
        impl_builder_option!($field_name, $field_name, $tag, $type);
    };
    ($field_name: ident, $exposed_name: ident, $tag: ident, $type:ty) => {
        pub fn $exposed_name(&mut self, $tag: $type) {
            unsafe {
                paste! {
                    self.api.[<set_ $field_name>](self.options, $tag);
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

/// Helper for constructing a [`Tox`] instance
pub struct ToxBuilder {
    inner: ToxBuilderImpl<ToxOptionsSys>,
}

impl ToxBuilder {
    pub fn new() -> Result<ToxBuilder, ToxBuilderCreationError> {
        Ok(ToxBuilder {
            inner: ToxBuilderImpl::new(ToxOptionsSys)?,
        })
    }

    pub fn ipv6(mut self, enable: bool) -> Self {
        self.inner.ipv6(enable);
        self
    }

    pub fn udp(mut self, enable: bool) -> Self {
        self.inner.udp(enable);
        self
    }

    pub fn local_discovery(mut self, enable: bool) -> Self {
        self.inner.local_discovery(enable);
        self
    }

    pub fn proxy_port(mut self, port: u16) -> Self {
        self.inner.proxy_port(port);
        self
    }

    pub fn start_port(mut self, port: u16) -> Self {
        self.inner.start_port(port);
        self
    }

    pub fn end_port(mut self, port: u16) -> Self {
        self.inner.end_port(port);
        self
    }

    pub fn tcp_port(mut self, port: u16) -> Self {
        self.inner.tcp_port(port);
        self
    }

    pub fn hole_punching(mut self, enable: bool) -> Self {
        self.inner.hole_punching(enable);
        self
    }

    pub fn experimental_thread_safety(mut self, enable: bool) -> Self {
        self.inner.experimental_thread_safety(enable);
        self
    }

    pub fn proxy_type(mut self, t: ProxyType) -> Self {
        self.inner.proxy_type(t);
        self
    }

    pub fn proxy_host(mut self, host: &str) -> Result<Self, NulError> {
        self.inner.proxy_host(host)?;
        Ok(self)
    }

    pub fn savedata(mut self, data: SaveData) -> Self {
        self.inner.savedata(data);
        self
    }

    pub fn log(mut self, enable: bool) -> Self {
        self.inner.log(enable);
        self
    }

    pub fn build(self) -> Result<Tox, ToxCreationError> {
        Ok(Tox::new(self.inner.build(ToxApiImpl)?))
    }
}

/// Generic implementation of [`ToxBuilder`]. Abstracted this way to allow for
/// testing/mocking without exposing generics to API consumers. Note that this
/// isn't quite as useful as it is for [`Tox`] but it does allow us to
/// test some of the conversion logic
struct ToxBuilderImpl<Api: ToxOptionsApi> {
    api: Api,
    options: *mut Tox_Options,
    log: bool,
}

impl<Api: ToxOptionsApi> ToxBuilderImpl<Api> {
    pub(crate) fn new(api: Api) -> Result<ToxBuilderImpl<Api>, ToxBuilderCreationError> {
        let mut err = TOX_ERR_OPTIONS_NEW_OK;

        let options = unsafe { api.new(&mut err as *mut TOX_ERR_OPTIONS_NEW) };
        if err != TOX_ERR_OPTIONS_NEW_OK {
            return Err(ToxBuilderCreationError);
        }

        Ok(ToxBuilderImpl {
            api,
            options,
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

    pub fn proxy_type(&mut self, t: ProxyType) {
        let c_type = match t {
            ProxyType::None => TOX_PROXY_TYPE_NONE,
            ProxyType::Http => TOX_PROXY_TYPE_HTTP,
            ProxyType::Socks5 => TOX_PROXY_TYPE_SOCKS5,
        };

        unsafe {
            self.api.set_proxy_type(self.options, c_type);
        }
    }

    pub fn proxy_host(&mut self, host: &str) -> Result<(), NulError> {
        let cstr = CString::new(host)?;
        unsafe {
            self.api.set_proxy_host(self.options, cstr.as_ptr());
        }
        Ok(())
    }

    pub fn savedata(&mut self, data: SaveData) {
        match data {
            SaveData::ToxSave(data) => unsafe {
                self.api
                    .set_savedata_type(self.options, TOX_SAVEDATA_TYPE_TOX_SAVE);
                self.api
                    .set_savedata_data(self.options, data.as_ptr(), data.len() as u64);
            },
            SaveData::SecretKey(data) => unsafe {
                self.api
                    .set_savedata_type(self.options, TOX_SAVEDATA_TYPE_SECRET_KEY);
                self.api
                    .set_savedata_data(self.options, data.as_ptr(), data.len() as u64);
            },
        }
    }

    pub fn log(&mut self, enable: bool) {
        self.log = enable;
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

    /// Create the [`Tox`] instance
    pub fn build<ToxApiImpl: ToxApi>(
        self,
        tox_api: ToxApiImpl,
    ) -> Result<ToxImpl<ToxApiImpl>, ToxCreationError> {
        if self.log {
            unsafe {
                self.api
                    .set_log_callback(self.options, Some(tox_log_callback));
            }
        }

        let mut err = TOX_ERR_NEW_OK;
        let sys_tox = unsafe { tox_api.new(self.options, &mut err) };

        if err != TOX_ERR_NEW_OK {
            return Err(Self::map_err_new(err));
        }

        let ret = ToxImpl::new(tox_api, sys_tox);

        Ok(ret)
    }
}

impl<Api: ToxOptionsApi> Drop for ToxBuilderImpl<Api> {
    fn drop(&mut self) {
        unsafe {
            self.api.free(self.options);
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
    use crate::sys::{MockToxApi, MockToxOptionsApi};
    use std::ffi::CStr;

    fn generate_tox_api_mock() -> MockToxApi {
        let mut mock = MockToxApi::default();

        mock.expect_callback_friend_request().return_const(());

        mock.expect_callback_friend_message().return_const(());

        mock.expect_kill().return_const(());
        mock.expect_new().returning_st(|_, _| std::ptr::null_mut());

        mock
    }
    struct BuilderFixture {
        builder: ToxBuilderImpl<MockToxOptionsApi>,
    }

    impl BuilderFixture {
        fn new(mut mock: MockToxOptionsApi) -> Result<BuilderFixture, Box<dyn std::error::Error>> {
            mock.expect_new()
                .returning_st(|_| 0xdeadbeef as *mut Tox_Options);

            mock.expect_free().return_const(());

            Ok(BuilderFixture {
                builder: ToxBuilderImpl::new(mock)?,
            })
        }
    }

    macro_rules! test_builder_options {
        ($test_name:ident, $rust_name:ident, $rust_val:expr, $mock_name:ident, $c_val:expr) => {
            paste! {
                #[test]
                fn [<test_options_ $test_name>]() -> Result<(), Box<dyn std::error::Error>>
                {
                    let mut mock = MockToxOptionsApi::default();

                    mock.[<expect_set_ $mock_name>]()
                        .withf_st(|_, v| *v == $c_val)
                        .return_const(())
                        .once();

                    let mut fixture = BuilderFixture::new(mock)?;

                    fixture.builder
                        .$rust_name($rust_val);

                    Ok(())
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

    #[test]
    fn test_builder_creation_failure() {
        let mut mock = MockToxOptionsApi::default();
        mock.expect_new().returning_st(|err| {
            unsafe { *err = TOX_ERR_OPTIONS_NEW_MALLOC };
            std::ptr::null_mut()
        });

        assert!(ToxBuilderImpl::new(mock).is_err());
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

    #[test]
    fn test_proxy_host_success() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockToxOptionsApi::default();

        mock.expect_set_proxy_host()
            .withf_st(|_, v| unsafe { CStr::from_ptr(*v).to_string_lossy() == "test" })
            .return_const(())
            .once();

        BuilderFixture::new(mock)?.builder.proxy_host("test")?;

        Ok(())
    }

    #[test]
    fn test_proxy_host_failure() -> Result<(), Box<dyn std::error::Error>> {
        let mock = MockToxOptionsApi::default();
        assert!(BuilderFixture::new(mock)?
            .builder
            .proxy_host("\0 \0 \0")
            .is_err());

        Ok(())
    }

    #[test]
    fn test_savedata_tox_save() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockToxOptionsApi::default();
        let savedata = "test".to_string().into_bytes();

        mock.expect_set_savedata_type()
            .withf_st(|_, v| *v == TOX_SAVEDATA_TYPE_TOX_SAVE)
            .return_const(())
            .once();

        let savedata_clone = savedata.clone();

        mock.expect_set_savedata_data()
            .withf_st(move |_, data, len| unsafe {
                std::slice::from_raw_parts(*data, *len as usize) == &savedata_clone
            })
            .return_const(())
            .once();

        BuilderFixture::new(mock)?
            .builder
            .savedata(SaveData::ToxSave(&savedata));

        Ok(())
    }

    #[test]
    fn test_savedata_secret_key() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockToxOptionsApi::default();

        mock.expect_set_savedata_type()
            .withf_st(|_, v| *v == TOX_SAVEDATA_TYPE_SECRET_KEY)
            .return_const(())
            .once();

        let savedata = "key".chars().map(|c| c as u8).collect::<Vec<u8>>();

        let savedata_clone = savedata.clone();

        mock.expect_set_savedata_data()
            .withf_st(move |_, data, len| unsafe {
                std::slice::from_raw_parts(*data, *len as usize) == &savedata_clone
            })
            .return_const(())
            .once();

        BuilderFixture::new(mock)?
            .builder
            .savedata(SaveData::SecretKey(&savedata));

        Ok(())
    }

    #[test]
    fn test_logger_enabled() -> Result<(), Box<dyn std::error::Error>> {
        let mut mock = MockToxOptionsApi::default();

        mock.expect_set_log_callback()
            .withf_st(|_, cb| *cb == Some(tox_log_callback))
            .return_const(())
            .once();

        let tox_mock = generate_tox_api_mock();
        let mut fixture = BuilderFixture::new(mock)?;
        fixture.builder.log(true);
        fixture.builder.build(tox_mock)?;

        let tox_mock = generate_tox_api_mock();
        let mock = MockToxOptionsApi::default();
        // The second iteration should fail due to the newly injected mock
        let mut fixture = BuilderFixture::new(mock)?;
        fixture.builder.log(false);
        fixture.builder.build(tox_mock)?;

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
