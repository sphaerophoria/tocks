#![allow(clippy::mutex_atomic)]
#![allow(non_snake_case)]

use anyhow::{anyhow, Context, Result};
use futures::FutureExt;
use lazy_static::lazy_static;
use log::*;
use openal_sys as oal;
use thiserror::Error;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[cfg_attr(test, mockall::automock)]
mod oal_func_impl {

    use std::ffi::c_void;

    use openal_sys as oal;

    extern "C" {
        #![cfg_attr(test, allow(unused))]

        pub fn alGenSources(n: i32, sources: *mut u32);
        pub fn alDeleteSources(n: i32, sources: *const u32);
        pub fn alBufferData(buffer: u32, format: i32, data: *const c_void, size: i32, from: i32);

        pub fn alGenBuffers(n: i32, buffers: *mut u32);
        pub fn alDeleteBuffers(n: i32, buffers: *const u32);
        pub fn alSourceQueueBuffers(source: u32, nb: i32, buffers: *const u32);
        pub fn alSourceUnqueueBuffers(source: u32, nb: i32, buffers: *mut u32);

        pub fn alSourcei(source: u32, param: i32, value: i32);
        pub fn alSourcePlay(source: u32);
        pub fn alGetSourcei(source: u32, param: i32, value: *mut i32);

        pub fn alGetError() -> i32;

        pub fn alcCreateContext(
            device: *mut oal::ALCdevice,
            attrlist: *const i32,
        ) -> *mut oal::ALCcontext;
        pub fn alcMakeContextCurrent(context: *mut oal::ALCcontext) -> bool;
        pub fn alcDestroyContext(context: *mut oal::ALCcontext);
        pub fn alcGetString(device: *mut oal::ALCdevice, param: i32) -> *const i8;

        pub fn alcOpenDevice(devicename: *const i8) -> *mut oal::ALCdevice;
        pub fn alcCloseDevice(device: *mut oal::ALCdevice) -> bool;

        // In progress API to migrate a device. This is not yet exposed in the
        // public header. See https://github.com/kcat/openal-soft/issues/533
        pub fn alcReopenDeviceSOFT(
            device: *mut oal::ALCdevice,
            deviceName: *const i8,
            attribs: *const i32,
        );
    }
}

#[mockall_double::double]
use oal_func_impl as oal_func;

use std::{
    collections::VecDeque,
    ffi::{c_void, CStr, CString},
    ptr::NonNull,
    sync::Mutex,
    time::Duration,
};

lazy_static! {
    // openal seems to be quite stateful. We have to guarantee that another
    // instance of our class will not call OAL functions again. This guard
    // ensures that only one instance of AudioManager can be constructed
    static ref SINGLE_INSTANCE_GUARD: Mutex<bool> = Mutex::new(false);
}

#[derive(Error, Debug)]
pub enum OalError {
    #[error("Invalid name")]
    InvalidName,
    #[error("Invalid enum")]
    InvalidEnum,
    #[error("Invalid value")]
    InvalidValue,
    #[error("Invalid operation")]
    InvalidOperation,
    #[error("Out of memory")]
    OutOfMemory,
    #[error("Unknown error")]
    Unknown,
}

impl From<u32> for OalError {
    fn from(err: u32) -> OalError {
        match err {
            oal::AL_INVALID_NAME => OalError::InvalidName,
            oal::AL_INVALID_ENUM => OalError::InvalidEnum,
            oal::AL_INVALID_VALUE => OalError::InvalidValue,
            oal::AL_INVALID_OPERATION => OalError::InvalidOperation,
            oal::AL_OUT_OF_MEMORY => OalError::OutOfMemory,
            _ => OalError::Unknown,
        }
    }
}

/// RAII wrapper around OpenAL source + pre-allocated buffers. This struct will
/// allocate a source and a user-provided number of buffers on start. As frames
/// are added it will which buffers are currently in queue, and which are free to
/// use. On destruction all requested OpenAL resources will be freed
struct OalSource {
    source: u32,
    // We allow for a user configurable number of incoming frames. Note that
    // these frames are not correlated with the amount of time remaining to play
    // since each buffer can have an arbitrary amount of data in it. The purpose
    // here is for the user to choose how frequently they need to populate data
    // relative to the amount of data within the frames they are pushing. More
    // buffers allows for more flexibility in terms of how frequently data needs
    // to be fed in
    available_buffers: Vec<u32>,
    processing_buffers: VecDeque<u32>,
}

impl OalSource {
    fn new(num_buffers: usize, looping: bool) -> Result<OalSource> {
        unsafe {
            let mut source = 0u32;

            oal_func::alGenSources(1, &mut source);
            oal_result().context("Failed to generate source")?;

            debug!("Allocated OpenAL source {}", source);

            let mut buffers = Vec::with_capacity(num_buffers);
            oal_func::alGenBuffers(num_buffers as i32, buffers.as_mut_ptr());
            oal_result().context("Failed to generate buffers")?;
            buffers.set_len(num_buffers);

            debug!("Got buffers for source {}: {:?}", source, buffers);

            oal_func::alSourcei(source, oal::AL_LOOPING as i32, looping as i32);

            Ok(OalSource {
                source,
                available_buffers: buffers,
                processing_buffers: Default::default(),
            })
        }
    }

    fn push_frame(&mut self, frame: AudioFrame) -> Result<()> {
        self.reclaim_processed_buffers()
            .context("Failed to reclaim processed buffers")?;

        if let Some(bufid) = self.available_buffers.last() {
            unsafe {
                let (data_ptr, data_len) = match &frame.data {
                    AudioData::Mono8(data) => (data.as_ptr() as *const c_void, data.len()),
                    AudioData::Mono16(data) => (data.as_ptr() as *const c_void, data.len() * 2),
                    AudioData::Stereo8(data) => (data.as_ptr() as *const c_void, data.len()),
                    AudioData::Stereo16(data) => (data.as_ptr() as *const c_void, data.len() * 2),
                };

                oal_func::alBufferData(
                    *bufid,
                    frame.data.get_oal_format(),
                    data_ptr,
                    data_len as i32,
                    frame.sample_rate,
                );

                oal_result().context("Failed to populate buffer data")?;

                debug!("Queuing buffer {} onto source {}", bufid, self.source);

                oal_func::alSourceQueueBuffers(self.source, 1, bufid);
                oal_result().context("Failed to queue buffer on source")?;
            }
        } else {
            return Err(anyhow!("Cannot queue any more samples onto source"));
        }

        let bufid = self.available_buffers.pop().expect("No available buffers");
        self.processing_buffers.push_back(bufid);

        unsafe {
            if !self.playing()? {
                debug!("Starting audio source {}", self.source);
                oal_func::alSourcePlay(self.source);
                oal_result().context("Failed to play source")?;
            }
        }

        Ok(())
    }

    fn playing(&self) -> Result<bool> {
        unsafe {
            let mut source_state = oal::AL_STOPPED as i32;
            oal_func::alGetSourcei(self.source, oal::AL_SOURCE_STATE as i32, &mut source_state);
            oal_result().context("Failed to get source state")?;

            Ok(source_state == oal::AL_PLAYING as i32)
        }
    }

    fn repeating(&self) -> Result<bool> {
        unsafe {
            let mut repeating = 0;
            oal_func::alGetSourcei(self.source, oal::AL_LOOPING as i32, &mut repeating);
            oal_result().context("Failed to get looping state")?;

            Ok(repeating != 0)
        }
    }

    fn reclaim_processed_buffers(&mut self) -> Result<()> {
        unsafe {
            let mut num_processed_buffers = 0;
            oal_func::alGetSourcei(
                self.source,
                oal::AL_BUFFERS_PROCESSED as i32,
                &mut num_processed_buffers,
            );
            oal_result().context("Failed to get number of processed buffers")?;

            if num_processed_buffers == 0 {
                return Ok(());
            }

            for _ in 0..num_processed_buffers {
                let bufid = self
                    .processing_buffers
                    .pop_front()
                    .expect("Processing buffer does not exist");

                debug!("Reclaimed buffer {}", bufid);

                self.available_buffers.push(bufid);
            }

            let start_reclaim_index = self.available_buffers.len() - num_processed_buffers as usize;
            oal_func::alSourceUnqueueBuffers(
                self.source,
                num_processed_buffers,
                self.available_buffers[start_reclaim_index..].as_mut_ptr(),
            );
            oal_result().context("Failed to unqueue processed buffers")?;
        }

        Ok(())
    }
}

impl Drop for OalSource {
    fn drop(&mut self) {
        info!("Dropping audio source {}", self.source);
        unsafe {
            oal_func::alDeleteSources(1, &self.source);
            oal_func::alDeleteBuffers(
                self.processing_buffers.len() as i32,
                self.processing_buffers.make_contiguous().as_ptr(),
            );
            oal_func::alDeleteBuffers(
                self.available_buffers.len() as i32,
                self.available_buffers.as_ptr(),
            );

            if let Err(e) = oal_result() {
                error!("Failed to drop OpenAL source {}: {}", self.source, e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AudioDevice {
    Default,
    Named(String),
}

impl ToString for AudioDevice {
    fn to_string(&self) -> String {
        match self {
            AudioDevice::Default => "Default".to_string(),
            AudioDevice::Named(s) => s.clone(),
        }
    }
}

pub enum FormattedAudio {
    Mp3(Vec<u8>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AudioData {
    Mono8(Vec<i8>),
    Mono16(Vec<i16>),
    Stereo8(Vec<i8>),
    Stereo16(Vec<i16>),
}

impl AudioData {
    fn get_oal_format(&self) -> i32 {
        let format = match *self {
            AudioData::Mono8(_) => oal::AL_FORMAT_MONO8,
            AudioData::Mono16(_) => oal::AL_FORMAT_MONO16,
            AudioData::Stereo8(_) => oal::AL_FORMAT_STEREO8,
            AudioData::Stereo16(_) => oal::AL_FORMAT_STEREO16,
        };

        format as i32
    }
}

#[derive(Debug)]
pub struct AudioFrame {
    pub data: AudioData,
    pub sample_rate: i32,
}

type Streams = Vec<(UnboundedReceiver<AudioFrame>, OalSource)>;

/// Wrapper around openal for our purposes.
pub struct AudioManager {
    device_handle: NonNull<oal::ALCdevice>,
    alc_context: NonNull<oal::ALCcontext>,
    streams: Streams,
    // finishing_streams are streams that we no longer are receiving audio data
    // for, but still have queued audio to play on the oal source. We need to
    // poll these at some interval and drop them when the queued data is complete
    finishing_streams: Vec<OalSource>,
}

pub struct RepeatingAudioHandle {
    // Just hold a sender that we don't push anything into. This allows us to
    // re-use all the logic around handling cleanup of audio channels
    _handle: UnboundedSender<AudioFrame>,
}

impl AudioManager {
    pub fn new() -> Result<AudioManager> {
        unsafe {
            let mut audio_manager_constructed = SINGLE_INSTANCE_GUARD.lock().unwrap();
            if *audio_manager_constructed {
                return Err(anyhow!("AudioManager already constructed once"));
            }

            *audio_manager_constructed = true;

            // Clear OpenAL error state
            oal_func::alGetError();

            // FIXME: Read device handle from storage
            let device_handle = NonNull::new(oal_func::alcOpenDevice(std::ptr::null()))
                .context("OpenAL returned null device pointer")?;

            let alc_context = oal_func::alcCreateContext(device_handle.as_ptr(), std::ptr::null());
            oal_func::alcMakeContextCurrent(alc_context);

            oal_result().context("Failed to create audio context")?;

            let alc_context = NonNull::new(alc_context).context("OpenAL returned null context")?;

            let audio_manager = AudioManager {
                device_handle,
                alc_context,
                streams: Vec::new(),
                finishing_streams: Vec::new(),
            };

            Ok(audio_manager)
        }
    }

    pub fn output_devices(&mut self) -> Result<Vec<AudioDevice>> {
        unsafe {
            let mut ret = vec![AudioDevice::Default];

            let mut devices =
                oal_func::alcGetString(std::ptr::null_mut(), oal::ALC_ALL_DEVICES_SPECIFIER as i32);
            while *devices != 0 {
                let device_cstr = CStr::from_ptr(devices);
                let device_str = device_cstr
                    .to_str()
                    .context("Audio device was not a valid Utf8 string")?;
                devices = devices.add(device_cstr.to_bytes_with_nul().len());
                ret.push(AudioDevice::Named(device_str.to_string()));
            }

            Ok(ret)
        }
    }

    pub fn set_output_device(&mut self, device: AudioDevice) -> Result<()> {
        unsafe {
            match device {
                AudioDevice::Default => {
                    oal_func::alcReopenDeviceSOFT(
                        self.device_handle.as_ptr(),
                        std::ptr::null(),
                        std::ptr::null(),
                    );
                }
                AudioDevice::Named(name) => {
                    let name_cstr = CString::new(name).context("Device name invalid")?;
                    oal_func::alcReopenDeviceSOFT(
                        self.device_handle.as_ptr(),
                        name_cstr.as_ptr(),
                        std::ptr::null(),
                    )
                }
            }
        }

        oal_result().context("Failed to switch output device")?;

        Ok(())
    }

    #[allow(unused)]
    pub fn create_playback_channel(
        &mut self,
        frame_depth: usize,
    ) -> Result<UnboundedSender<AudioFrame>> {
        self.create_playback_channel_priv(frame_depth, false)
    }

    pub fn play_formatted_audio(&mut self, container: FormattedAudio) {
        let _ = self.play_formatted_audio_priv(container, false);
    }

    pub fn play_repeating_formatted_audio(
        &mut self,
        container: FormattedAudio,
    ) -> RepeatingAudioHandle {
        let handle = self.play_formatted_audio_priv(container, true);

        RepeatingAudioHandle { _handle: handle }
    }

    pub async fn run(&mut self) {
        loop {
            futures::select! {
                (frame, index) = Self::incoming_audio_data(&mut self.streams).fuse() => {
                    self.handle_incoming_audio_frame(frame, index);
                },
                _ = Self::service_finishing_streams_timer(&self.finishing_streams).fuse() => {
                    self.cleanup_finished_streams();
                }
            };
        }
    }

    async fn incoming_audio_data(streams: &mut Streams) -> (Option<AudioFrame>, usize) {
        // If there's no data we just wait forever to avoid infinite looping
        // from the parent function. This is required because select_all falls
        // over on an empty iterator
        if streams.is_empty() {
            futures::future::pending::<()>().await;
        }

        let futures = streams
            .iter_mut()
            .enumerate()
            .map(|(index, (channel, _source))| {
                async move { (channel.recv().await, index) }.boxed_local()
            });

        let (res, _, _) = futures::future::select_all(futures).await;

        res
    }

    async fn service_finishing_streams_timer(finishing_streams: &[OalSource]) {
        // We never need to wake up if there are no streams to service
        if finishing_streams.is_empty() {
            futures::future::pending::<()>().await;
        }

        tokio::time::sleep(Duration::from_millis(100)).await
    }

    fn create_playback_channel_priv(
        &mut self,
        frame_depth: usize,
        looping: bool,
    ) -> Result<UnboundedSender<AudioFrame>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let oal_source =
            OalSource::new(frame_depth, looping).context("Failed to allocate OpenAL source")?;

        self.streams.push((rx, oal_source));

        Ok(tx)
    }

    fn play_formatted_audio_priv(
        &mut self,
        container: FormattedAudio,
        looping: bool,
    ) -> UnboundedSender<AudioFrame> {
        let notification_handle = self.create_playback_channel_priv(50, looping).unwrap();

        match container {
            FormattedAudio::Mp3(data) => Self::decode_mp3_into_channel(data, &notification_handle),
        }

        notification_handle
    }

    fn handle_incoming_audio_frame(&mut self, frame: Option<AudioFrame>, index: usize) {
        match frame {
            Some(frame) => {
                if let Err(e) = self.streams[index].1.push_frame(frame) {
                    error!("Failed to push frame to OpenAL source: {:?}", e);
                }
            }
            None => {
                debug!(
                    "Stream closed, queuing stream {} to be finished",
                    self.streams[index].1.source
                );
                let (_, oal_source) = self.streams.remove(index);
                self.finishing_streams.push(oal_source);
            }
        }
    }

    fn cleanup_finished_streams(&mut self) {
        if !self.finishing_streams.is_empty() {
            let mut finishing_streams = Vec::new();
            std::mem::swap(&mut finishing_streams, &mut self.finishing_streams);
            let (finishing_streams, _) = finishing_streams
                .into_iter()
                .partition(|item| item.playing().unwrap() && !item.repeating().unwrap());

            self.finishing_streams = finishing_streams;
        }

        // FIXME: close device if all streams are finished
    }

    fn decode_mp3_into_channel(data: Vec<u8>, channel: &UnboundedSender<AudioFrame>) {
        let mut mp3_decoder = minimp3::Decoder::new(&data[..]);

        while let Ok(frame) = mp3_decoder.next_frame() {
            let data = match frame.channels {
                1 => AudioData::Mono16(frame.data),
                2 => AudioData::Stereo16(frame.data),
                _ => continue,
            };

            channel
                .send(AudioFrame {
                    data,
                    sample_rate: frame.sample_rate,
                })
                .expect("Failed to send notification data to audio thread");
        }
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        let mut audio_manager_constructed = SINGLE_INSTANCE_GUARD.lock().unwrap();

        unsafe {
            oal_func::alcMakeContextCurrent(std::ptr::null_mut());
            oal_func::alcDestroyContext(self.alc_context.as_ptr());
            oal_func::alcCloseDevice(self.device_handle.as_ptr());
        }

        *audio_manager_constructed = false;
    }
}

fn oal_result() -> Result<()> {
    unsafe {
        let err = oal_func::alGetError() as u32;
        if err == oal::AL_NO_ERROR {
            return Ok(());
        }

        Err(OalError::from(err).into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use rusty_fork::rusty_fork_test;
    use std::sync::{Arc, Mutex};

    struct AudioManagerFixture {
        audio_manager: AudioManager,
        #[allow(unused)]
        al_get_error_ctx: oal_func::__alGetError::Context,
        #[allow(unused)]
        alc_open_device_ctx: oal_func::__alcOpenDevice::Context,
        #[allow(unused)]
        alc_create_context_ctx: oal_func::__alcCreateContext::Context,
        #[allow(unused)]
        alc_make_context_current_ctx: oal_func::__alcMakeContextCurrent::Context,
        #[allow(unused)]
        alc_destroy_context_ctx: oal_func::__alcDestroyContext::Context,
        #[allow(unused)]
        alc_close_device_ctx: oal_func::__alcCloseDevice::Context,
    }

    fn create_audio_manager() -> AudioManagerFixture {
        let al_get_error_ctx = oal_func::alGetError_context();
        al_get_error_ctx.expect().return_const_st(0);

        const DEVICE_ADDR: u64 = 0x12345678;

        let alc_open_device_ctx = oal_func::alcOpenDevice_context();
        alc_open_device_ctx
            .expect()
            .return_const_st(DEVICE_ADDR as *mut oal::ALCdevice);

        const CONTEXT_ADDR: u64 = 0xdeadbeef;

        let alc_create_context_ctx = oal_func::alcCreateContext_context();
        alc_create_context_ctx
            .expect()
            .return_const_st(CONTEXT_ADDR as *mut oal::ALCcontext);

        let alc_make_context_current_ctx = oal_func::alcMakeContextCurrent_context();
        alc_make_context_current_ctx
            .expect()
            .withf_st(|addr| (*addr as u64) == CONTEXT_ADDR || *addr == std::ptr::null_mut())
            .returning(|_| true);

        let alc_destroy_context_ctx = oal_func::alcDestroyContext_context();
        alc_destroy_context_ctx
            .expect()
            .withf_st(|addr| (*addr as u64) == CONTEXT_ADDR)
            .return_const_st(());

        let alc_close_device_ctx = oal_func::alcCloseDevice_context();
        alc_close_device_ctx
            .expect()
            .withf_st(|addr| (*addr as u64) == DEVICE_ADDR)
            .return_const_st(true);

        let audio_manager = AudioManager::new().unwrap();

        AudioManagerFixture {
            al_get_error_ctx,
            alc_open_device_ctx,
            alc_create_context_ctx,
            alc_make_context_current_ctx,
            alc_destroy_context_ctx,
            alc_close_device_ctx,
            audio_manager,
        }
    }

    rusty_fork_test! {
        // FIXME: Lots more tests could be added but for the time being I don't
        // feel like it
        #[test]
        fn test_single_instance_allowed() {
            let _fixture = create_audio_manager();
            assert!(AudioManager::new().is_err())
        }

        #[test]
        fn test_playback_channel() {
            let al_delete_sources_ctx = oal_func::alDeleteSources_context();
            al_delete_sources_ctx.expect().return_const_st(());

            let al_delete_buffers_ctx = oal_func::alDeleteBuffers_context();
            al_delete_buffers_ctx.expect().return_const_st(());

            let mut fixture = create_audio_manager();

            let al_gen_sources_ctx = oal_func::alGenSources_context();
            al_gen_sources_ctx.expect().return_const_st(());

            let al_gen_buffers_ctx = oal_func::alGenBuffers_context();
            al_gen_buffers_ctx.expect().return_const_st(());

            let al_source_queue_buffers_ctx = oal_func::alSourceQueueBuffers_context();
            al_source_queue_buffers_ctx.expect().return_const_st(());

            let al_sourcei_ctx = oal_func::alSourcei_context();
            al_sourcei_ctx.expect()
                .withf_st(|_source, key, _value| *key == oal::AL_LOOPING as i32)
                .return_const_st(());

            let playback_channel = fixture.audio_manager.create_playback_channel(50).unwrap();
            let mut sent_buf = Vec::new();

            for i in 0..3000 {
                sent_buf.push(i);
            }

            playback_channel.send(AudioFrame{
                data: AudioData::Mono16(sent_buf.clone()),
                sample_rate: 44100
            }).unwrap();


            let fut = async {
                // Run event loop for 100ms
                futures::select! {
                    _ = fixture.audio_manager.run().fuse() => (),
                    _ = tokio::time::sleep(Duration::from_millis(100)).fuse() => (),
                }
            };

            let al_get_sourcei_ctx = oal_func::alGetSourcei_context();

            // Never finish processing, our audio should be short enough that we
            // never have to reclaim
            al_get_sourcei_ctx.expect().withf_st(|_source, param, _value| *param == oal::AL_BUFFERS_PROCESSED as i32)
                .returning_st(|_source, _param, value| unsafe {*value = 0i32; });

            al_get_sourcei_ctx.expect().withf_st(|_source, param, _value| *param == oal::AL_SOURCE_STATE as i32)
                .returning_st(|_source, _param, value| unsafe {*value = oal::AL_PLAYING as i32; });

            al_get_sourcei_ctx.expect().withf_st(|_source, param, _value| *param == oal::AL_LOOPING as i32)
                .returning_st(|_source, _param, value| unsafe {*value = 0; });

            let buf_data: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
            let buf_data_clone = Arc::clone(&buf_data);

            let al_buffer_data_ctx = oal_func::alBufferData_context();
            al_buffer_data_ctx.expect()
                .withf_st(|_, format, _, _, _| {
                    *format == oal::AL_FORMAT_MONO16 as i32
                })
                .returning_st(move |_, _, data, data_len, _| {
                    unsafe {
                        let mut locked = buf_data.lock().unwrap();
                        locked.extend_from_slice(std::slice::from_raw_parts(data as *const i16, data_len as usize / 2));
                    }
                });

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(fut);

            let buf_data = buf_data_clone;

            assert!(*buf_data.lock().unwrap() == sent_buf);
        }
    }
}
