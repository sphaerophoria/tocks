use crate::{
    call_state_to_qtring,
};

use chrono::{DateTime, Utc};
use tocks::{CallState, ChatLogEntry, ChatMessageId};
use toxcore::Message;

use ::log::{debug, error};
use qmetaobject::*;


use std::collections::HashMap;

#[derive(QObject)]
#[allow(non_snake_case)]
pub struct ChatModel {
    base: qt_base_class!(trait QAbstractItemModel),
    callState: qt_property!(QString; NOTIFY callStateChanged),
    callStateChanged: qt_signal!(),
    lastReadTime: qt_property!(QDateTime; NOTIFY lastReadTimeChanged),
    lastReadTimeChanged: qt_signal!(),
    // Cache the last message time for easy access from QML
    lastMessageTime: qt_property!(QDateTime; NOTIFY lastMessageTimeChanged),
    lastMessageTimeChanged: qt_signal!(),

    load_message_callback: Box<dyn Fn(Option<ChatMessageId>)>,
    chat_log: Vec<ChatLogEntry>,
}

impl ChatModel {
    const MESSAGE_ROLE: i32 = USER_ROLE;
    const SENDER_ID_ROLE: i32 = USER_ROLE + 1;
    const COMPLETE_ROLE: i32 = USER_ROLE + 2;

    pub fn new<F: Fn(Option<ChatMessageId>) + 'static>(load_message_callback: F) -> ChatModel {
        ChatModel {
            base: Default::default(),
            callState: Default::default(),
            callStateChanged: Default::default(),
            lastReadTime: Default::default(),
            lastReadTimeChanged: Default::default(),
            lastMessageTime: Default::default(),
            lastMessageTimeChanged: Default::default(),

            load_message_callback: Box::new(load_message_callback),
            chat_log: Default::default(),
        }
    }

    pub fn set_call_state(&mut self, state: &CallState) {
        self.callState = call_state_to_qtring(state);
        self.callStateChanged();
    }

    pub fn set_last_read_time(&mut self, timestamp: DateTime<Utc>) {
        self.lastReadTime = qdatetime_from_datetime(&timestamp);
        self.lastReadTimeChanged();
    }


    pub fn push_messages(&mut self, mut entries: Vec<ChatLogEntry>) {
        if entries.is_empty() {
            return;
        }

        let cmp_fn = |item: &ChatLogEntry| {
            item.id().cmp(&entries.first().unwrap().id())
        };

        let entries_insert_start = self.chat_log.binary_search_by(cmp_fn);
        let entries_insert_end = self.chat_log.binary_search_by(cmp_fn);

        let insert_idx = if let (Err(start_idx), Err(end_idx)) = (entries_insert_start, entries_insert_end) {
            assert!(start_idx == end_idx);
            start_idx
        } else if let (Ok(_), Ok(_)) = (entries_insert_start, entries_insert_end) {
            return
        } else if let Ok(_) = entries_insert_end {
            let overlap_start = entries.binary_search_by(|item| {
                item.id().cmp(&self.chat_log.first().unwrap().id())
            });

            let overlap_start = overlap_start.expect("Failed to find overlap start");
            entries.truncate(overlap_start);
            0
        } else {
            let cmp_fn = |item: &ChatLogEntry| {
                item.id().cmp(&self.chat_log.last().unwrap().id())
            };
            let overlap_end = entries.binary_search_by(cmp_fn);
            let overlap_end = overlap_end.expect("Failed to find overlap end");
            entries = entries.drain(overlap_end + 1..).collect();
            self.chat_log.len()
        };

        // Reverse direction for model indexes
        let (model_start_idx, model_end_idx) = if insert_idx == 0 {
            (self.chat_log.len(), self.chat_log.len() + entries.len() - 1)
        } else {
            (0, entries.len() - 1)
        };

        debug!("Splicing {} messages at {}, qml {}-{}",
            entries.len(), insert_idx, model_start_idx, model_end_idx);

        (self as &dyn QAbstractItemModel).begin_insert_rows(QModelIndex::default(), model_start_idx as i32, model_end_idx as i32);

        self.chat_log.splice(insert_idx..insert_idx, entries.into_iter());

        (self as &dyn QAbstractItemModel).end_insert_rows();

        self.lastMessageTime = qdatetime_from_datetime(self.chat_log.last().unwrap().timestamp());
        self.lastMessageTimeChanged();
    }

    pub fn push_message(&mut self, entry: ChatLogEntry) {
        self.push_messages(vec![entry]);
    }

    pub fn resolve_message(&mut self, id: ChatMessageId) {
        let idx = match self.chat_log.binary_search_by(|item| item.id().cmp(&id)) {
            Ok(idx) => idx,
            Err(_) => {
                error!("Chatlog item {} not found", id);
                return;
            }
        };

        self.chat_log[idx].set_complete(true);

        let qidx = (self as &dyn QAbstractItemModel).create_index(
            self.reversed_index(idx),
            0,
            0,
        );
        (self as &dyn QAbstractItemModel).data_changed(qidx, qidx);
    }

    pub fn reversed_index(&self, idx: usize) -> i32 {
        (self.chat_log.len() - idx as usize - 1) as i32
    }
}

impl QAbstractItemModel for ChatModel {
    fn index(&self, row: i32, _column: i32, _parent: QModelIndex) -> QModelIndex {
        (self as &dyn QAbstractItemModel).create_index(row, 0, 0)
    }

    fn parent(&self, _index: QModelIndex) -> QModelIndex {
        QModelIndex::default()
    }

    fn row_count(&self, _parent: QModelIndex) -> i32 {
        debug!("Returning row count {}", self.chat_log.len());
        self.chat_log.len() as i32
    }

    fn column_count(&self, _parent: QModelIndex) -> i32 {
        1
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        debug!("Returning line, {}", index.row());

        let entry = self.chat_log.get(self.reversed_index(index.row() as usize) as usize);

        if entry.is_none() {
            return QVariant::default();
        }

        let entry = entry.unwrap();

        match role {
            Self::MESSAGE_ROLE => {
                let message = entry.message();

                if let Message::Normal(message) = message {
                    QString::from(message.as_ref()).to_qvariant()
                } else {
                    QVariant::default()
                }
            }
            Self::SENDER_ID_ROLE => entry.sender().id().to_qvariant(),
            Self::COMPLETE_ROLE => entry.complete().to_qvariant(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut ret = HashMap::new();

        ret.insert(Self::MESSAGE_ROLE, "message".into());
        ret.insert(Self::SENDER_ID_ROLE, "senderId".into());
        ret.insert(Self::COMPLETE_ROLE, "complete".into());

        ret
    }

    fn can_fetch_more(&self, _parent: QModelIndex) -> bool {
        true
    }

    fn fetch_more(&mut self, _parent: QModelIndex) {
        (self.load_message_callback)(self.chat_log.first().map(|item| *item.id()));
    }
}

fn qdatetime_from_datetime(timestamp: &DateTime<Utc>) -> QDateTime {
    let naive_utc = timestamp.naive_local();
    let date = naive_utc.date();
    let time = naive_utc.time();
    QDateTime::from_date_time_local_timezone(date.into(), time.into())
}
