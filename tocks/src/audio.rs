#![allow(clippy::mutex_atomic)]

use anyhow::{anyhow, Context, Result};
use futures::FutureExt;
use lazy_static::lazy_static;
use log::*;
use openal_sys as oal;
use thiserror::Error;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::{collections::VecDeque, ffi::c_void, ptr::NonNull, sync::Mutex, time::Duration};

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
    fn new(num_buffers: usize) -> Result<OalSource> {
        unsafe {
            let mut source = 0u32;

            oal::alGenSources(1, &mut source);
            oal_result().context("Failed to generate source")?;

            debug!("Allocated OpenAL source {}", source);

            let mut buffers = Vec::with_capacity(num_buffers);
            oal::alGenBuffers(num_buffers as i32, buffers.as_mut_ptr());
            oal_result().context("Failed to generate buffers")?;
            buffers.set_len(num_buffers);

            debug!("Got buffers for source {}: {:?}", source, buffers);

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

                oal::alBufferData(
                    *bufid,
                    frame.data.get_oal_format(),
                    data_ptr,
                    data_len as i32,
                    frame.sample_rate,
                );

                oal_result().context("Failed to populate buffer data")?;

                debug!("Queuing buffer {} onto source {}", bufid, self.source);

                oal::alSourceQueueBuffers(self.source, 1, bufid);
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
                oal::alSourcePlay(self.source);
                oal_result().context("Failed to play source")?;
            }
        }

        Ok(())
    }

    fn playing(&self) -> Result<bool> {
        unsafe {
            let mut source_state = oal::AL_STOPPED as i32;
            oal::alGetSourcei(self.source, oal::AL_SOURCE_STATE as i32, &mut source_state);
            oal_result().context("Failed to get source state")?;

            Ok(source_state == oal::AL_PLAYING as i32)
        }
    }

    fn reclaim_processed_buffers(&mut self) -> Result<()> {
        unsafe {
            let mut num_processed_buffers = 0;
            oal::alGetSourcei(
                self.source,
                oal::AL_BUFFERS_PROCESSED as i32,
                &mut num_processed_buffers,
            );
            oal_result().context("Failed to get number of processed buffers")?;

            for _ in 0..num_processed_buffers {
                let bufid = self
                    .processing_buffers
                    .pop_front()
                    .expect("Processing buffer does not exist");

                debug!("Reclaimed buffer {}", bufid);

                self.available_buffers.push(bufid);
            }

            let start_reclaim_index = self.available_buffers.len() - num_processed_buffers as usize;
            oal::alSourceUnqueueBuffers(
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
            oal::alDeleteSources(1, &self.source);
            oal::alDeleteBuffers(
                self.processing_buffers.len() as i32,
                self.processing_buffers.make_contiguous().as_ptr(),
            );
            oal::alDeleteBuffers(
                self.available_buffers.len() as i32,
                self.available_buffers.as_ptr(),
            );

            if let Err(e) = oal_result() {
                error!("Failed to drop OpenAL source {}: {}", self.source, e);
            }
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

impl AudioManager {
    pub fn new() -> Result<AudioManager> {
        unsafe {
            let mut audio_manager_constructed = SINGLE_INSTANCE_GUARD.lock().unwrap();
            if *audio_manager_constructed {
                return Err(anyhow!("AudioManager already constructed once"));
            }

            *audio_manager_constructed = true;

            // Clear OpenAL error state
            oal::alGetError();

            // FIXME: Read device handle from storage
            let device_handle = NonNull::new(oal::alcOpenDevice(std::ptr::null()))
                .context("OpenAL returned null device pointer")?;

            let alc_context = oal::alcCreateContext(device_handle.as_ptr(), std::ptr::null());
            oal::alcMakeContextCurrent(alc_context);

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

    pub fn create_playback_channel(
        &mut self,
        frame_depth: usize,
    ) -> Result<UnboundedSender<AudioFrame>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let oal_source = OalSource::new(frame_depth).context("Failed to allocate OpenAL source")?;

        self.streams.push((rx, oal_source));

        Ok(tx)
    }

    pub fn play_formatted_audio(&mut self, container: FormattedAudio) {
        match container {
            FormattedAudio::Mp3(data) => {
                let notification_handle =
                    self.create_playback_channel(50).unwrap();

                let mut mp3_decoder = minimp3::Decoder::new(&data[..]);

                while let Ok(frame) = mp3_decoder.next_frame() {
                    let data = match frame.channels {
                        1 => AudioData::Mono16(frame.data),
                        2 => AudioData::Stereo16(frame.data),
                        _ => continue,
                    };

                    notification_handle
                        .send(AudioFrame {
                            data,
                            sample_rate: frame.sample_rate,
                        })
                        .expect("Failed to send notification data to audio thread");
                }
            }
        }
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

        tokio::time::sleep(Duration::from_millis(300)).await
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
                .partition(|item| item.playing().unwrap());

            self.finishing_streams = finishing_streams;
        }

        // FIXME: close device if all streams are finished
    }
}

impl Drop for AudioManager {
    fn drop(&mut self) {
        let mut audio_manager_constructed = SINGLE_INSTANCE_GUARD.lock().unwrap();

        unsafe {
            oal::alcMakeContextCurrent(std::ptr::null_mut());
            oal::alcDestroyContext(self.alc_context.as_ptr());
            oal::alcCloseDevice(self.device_handle.as_ptr());
        }

        *audio_manager_constructed = false;
    }
}

fn oal_result() -> Result<()> {
    unsafe {
        let err = oal::alGetError() as u32;
        if err == oal::AL_NO_ERROR {
            return Ok(());
        }

        Err(OalError::from(err).into())
    }
}
