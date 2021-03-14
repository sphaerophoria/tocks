use toxcore::{Message, Receipt};

use std::collections::HashMap;
use std::ops::Index;
use std::slice::SliceIndex;
use std::sync::{self, Arc, RwLock};

use tokio::sync::broadcast::{self, Receiver, Sender};

// FIXME: Replace with more advanced DB type
type History = Vec<ChatMessage>;
type SharedHistory = Arc<RwLock<History>>;

#[derive(Clone)]
pub enum ChatEvent {
    MessageAdded(usize),
    MessageUpdated(usize),
}

pub struct ChatViewGuard<'a> {
    history_guard: sync::RwLockReadGuard<'a, History>,
}

impl<'a> ChatViewGuard<'a> {
    pub fn len(&self) -> usize {
        self.history_guard.len()
    }
}

impl<'a, I> Index<I> for ChatViewGuard<'a>
where
    I: SliceIndex<[ChatMessage]>,
{
    type Output = <I as SliceIndex<[ChatMessage]>>::Output;

    fn index(&self, index: I) -> &Self::Output {
        self.history_guard.index(index)
    }
}

pub struct ChatView {
    history: SharedHistory,
    chat_event_rx: Receiver<ChatEvent>,
}

impl ChatView {
    fn new(history: SharedHistory, chat_event_rx: Receiver<ChatEvent>) -> ChatView {
        ChatView {
            history,
            chat_event_rx,
        }
    }

    pub fn lock(&self) -> ChatViewGuard<'_> {
        ChatViewGuard {
            history_guard: self.history.read().unwrap(),
        }
    }

    pub async fn chat_event(&mut self) -> Option<ChatEvent> {
        self.chat_event_rx.recv().await.ok()
    }
}

pub struct ChatMessage {
    pub from_self: bool,
    pub message: Message,
    pub complete: bool,
}

/// A chat room
///
/// This struct is meant to provide an up to date view of a conversation. This it
/// provides a mechanism to be notified on update, as well as a view into the
/// previous conversation history.
pub struct ChatRoom {
    history: Arc<RwLock<History>>,
    #[allow(dead_code)]
    receipts: HashMap<usize, Receipt>,
    chat_event_tx: Sender<ChatEvent>,
}

impl ChatRoom {
    pub fn new() -> ChatRoom {
        let history = Default::default();
        let receipts = HashMap::new();

        let (chat_event_tx, _) = broadcast::channel(100);

        ChatRoom {
            history,
            receipts,
            chat_event_tx,
        }
    }

    pub fn push_sent_message(&mut self, message: Message, receipt: Receipt) {
        let mut history = self.history.write().unwrap();

        history.push(ChatMessage {
            from_self: true,
            message: message,
            complete: false,
        });

        let message_idx = history.len() - 1;

        self.receipts.insert(message_idx, receipt);

        // If no one is listening that is not a problem for us. They'll catch up
        // through the ChatView APIs later
        let _ = self
            .chat_event_tx
            .send(ChatEvent::MessageAdded(message_idx));
    }

    pub fn push_received_message(&mut self, message: Message) {
        let mut history = self.history.write().unwrap();

        history.push(ChatMessage {
            from_self: false,
            message,
            complete: true,
        });

        // If no one is listening that is not a problem for us. They'll catch up
        // through the ChatView APIs later
        let _ = self
            .chat_event_tx
            .send(ChatEvent::MessageAdded(history.len() - 1));
    }

    pub fn view(&self) -> ChatView {
        ChatView::new(Arc::clone(&self.history), self.chat_event_tx.subscribe())
    }
}
