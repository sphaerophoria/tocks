use crate::{
    audio::{AudioData, AudioFrame},
    ChatHandle,
};

use toxcore::av::{
    ActiveCall, AudioFrame as CoreFrame, CallEvent as CoreCallEvent, CallState as CoreCallState,
    IncomingCall,
};

use anyhow::{bail, Context, Result};
use futures::prelude::*;
use serde::{Deserialize, Serialize};

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum CallState {
    Incoming,
    Outgoing,
    Active,
    Idle,
}

pub enum CallEvent {
    AudioReceived(ChatHandle, AudioFrame),
    CallAccepted(ChatHandle),
    CallEnded(ChatHandle),
}

impl TryFrom<(ChatHandle, CoreCallEvent)> for CallEvent {
    type Error = ();

    fn try_from(event: (ChatHandle, CoreCallEvent)) -> Result<CallEvent, Self::Error> {
        match event.1 {
            CoreCallEvent::AudioReceived(core_frame) => {
                let audio_buf = Arc::try_unwrap(core_frame.data).unwrap();
                let audio_data = match core_frame.channels {
                    1 => AudioData::Mono16(audio_buf),
                    2 => AudioData::Stereo16(audio_buf),
                    _ => panic!("Unsupported channel number"),
                };

                Ok(CallEvent::AudioReceived(
                    event.0,
                    AudioFrame {
                        data: audio_data,
                        sample_rate: core_frame.sample_rate as i32,
                    },
                ))
            }
            CoreCallEvent::CallStateChanged(CoreCallState::Finished) => {
                Ok(CallEvent::CallEnded(event.0))
            }
            CoreCallEvent::CallStateChanged(CoreCallState::Active) => {
                Ok(CallEvent::CallAccepted(event.0))
            }
            _ => Err(()),
        }
    }
}

impl TryFrom<AudioFrame> for CoreFrame {
    type Error = anyhow::Error;
    fn try_from(frame: AudioFrame) -> Result<CoreFrame> {
        let frame = match frame.data {
            AudioData::Mono16(buf) => CoreFrame {
                data: Arc::new(buf),
                channels: 1,
                sample_rate: frame.sample_rate as u32,
            },
            AudioData::Stereo16(buf) => CoreFrame {
                data: Arc::new(buf),
                channels: 2,
                sample_rate: frame.sample_rate as u32,
            },
            _ => bail!("Unsupported data format"),
        };

        Ok(frame)
    }
}

pub struct CallManager {
    incoming_calls: HashMap<ChatHandle, IncomingCall>,
    active_calls: HashMap<ChatHandle, ActiveCall>,
}

impl CallManager {
    pub fn new() -> CallManager {
        CallManager {
            incoming_calls: Default::default(),
            active_calls: Default::default(),
        }
    }

    pub fn call_state(&self, chat: &ChatHandle) -> CallState {
        if self.incoming_calls.contains_key(chat) {
            CallState::Incoming
        } else if let Some(call) = self.active_calls.get(chat) {
            match call.call_state() {
                CoreCallState::Active => CallState::Active,
                CoreCallState::Finished => CallState::Idle,
                CoreCallState::WaitingForPeerAnswer => CallState::Outgoing,
                CoreCallState::WaitingForSelfAnswer => CallState::Incoming,
            }
        } else {
            CallState::Idle
        }
    }

    pub fn incoming_call(&mut self, chat: ChatHandle, handle: IncomingCall) {
        self.incoming_calls.insert(chat, handle);
    }

    pub fn accept_call(&mut self, chat: &ChatHandle) -> Result<()> {
        let incoming_call = self
            .incoming_calls
            .remove(chat)
            .context("Incoming call handle not available")?;

        let chat = *chat;
        let active_call = incoming_call.accept().context("Failed to accept call")?;

        self.active_calls.insert(chat, active_call);

        Ok(())
    }

    pub fn outgoing_call(&mut self, chat: ChatHandle, call: ActiveCall) {
        self.active_calls.insert(chat, call);
    }

    pub fn drop_call(&mut self, chat: &ChatHandle) {
        self.incoming_calls.remove(chat);
        self.active_calls.remove(chat);
    }

    pub fn send_audio_frame(&mut self, frame: AudioFrame) -> Result<()> {
        let core_frame: CoreFrame = frame
            .try_into()
            .context("Failed to convert audio frame to core audio frame")?;

        self.active_calls
            .iter_mut()
            .try_for_each(|(_, call)| {
                call.send_audio_frame(core_frame.clone())
                    .map_err(anyhow::Error::from)
            })
            .context("Failed to send audio to one or more friends")
    }

    pub async fn run(&mut self) -> CallEvent {
        futures::select! {
            event = Self::wait_for_active_call_event(&mut self.active_calls).fuse() => {
                let (handle, event) = event;
                let event = event.unwrap();
                self.handle_call_event(&handle, &event);
                (handle, event).try_into().unwrap()
            }
            hungup_handle = Self::wait_for_incoming_hangups(&mut self.incoming_calls).fuse() => {
                self.incoming_calls.remove(&hungup_handle);
                CallEvent::CallEnded(hungup_handle)
            }
        }
    }

    async fn wait_for_incoming_hangups(
        incoming_calls: &mut HashMap<ChatHandle, IncomingCall>,
    ) -> ChatHandle {
        if incoming_calls.is_empty() {
            futures::future::pending::<()>().await;
        }

        let iter = incoming_calls
            .iter_mut()
            .map(|(chat, call)| call.wait_hangup().map(move |_| *chat));
        futures::future::select_all(iter).await.0
    }

    async fn wait_for_active_call_event(
        active_calls: &mut HashMap<ChatHandle, ActiveCall>,
    ) -> (ChatHandle, Option<CoreCallEvent>) {
        if active_calls.is_empty() {
            futures::future::pending::<()>().await;
        }

        let iter = active_calls
            .iter_mut()
            .map(|(chat, call)| call.next().map(move |v| (*chat, v)));
        futures::future::select_all(iter).await.0
    }

    fn handle_call_event(&mut self, chat: &ChatHandle, event: &CoreCallEvent) {
        if let CoreCallEvent::CallStateChanged(CoreCallState::Finished) = event {
            self.active_calls.remove(chat);
        }
    }
}
