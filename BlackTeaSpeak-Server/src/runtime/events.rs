use super::*;
use crate::query::{CommandRequest, QueryResponse};

impl BaselineRuntime {
    pub fn subscribe_events(&self, callback: EventCallback) {
        self.event_subscribers.lock().unwrap().push(callback);
    }

    pub fn broadcast_event(&self, server_id: u32, notification: &crate::transport::TransportNotification) {
        let subs = self.event_subscribers.lock().unwrap();
        for sub in subs.iter() {
            sub(self, server_id, notification);
        }
    }

    pub fn record_text_message(
        &mut self,
        target: &TextMessageTarget,
        sender_database_id: u64,
        sender_unique_id: String,
        sender_name: String,
    ) -> u64 {
        let timestamp = self.next_conversation_timestamp();
        let Some(conversation_id) = (match target.target_mode {
            2 => target.channel_id,
            3 => Some(0),
            _ => None,
        }) else {
            return timestamp;
        };

        self.store
            .conversation_messages
            .entry(target.server_id)
            .or_default()
            .push(ConversationMessage {
                conversation_id,
                timestamp,
                sender_database_id,
                sender_unique_id,
                sender_name,
                message: target.message.clone(),
            });
        timestamp
    }

    pub(crate) fn record_private_message(
        &mut self,
        server_id: u32,
        sender: ConversationParticipant,
        target: ConversationParticipant,
        message: String,
    ) -> u64 {
        let timestamp = self.next_conversation_timestamp();
        self.store
            .private_messages
            .entry(server_id)
            .or_default()
            .push(PrivateConversationMessage {
                timestamp,
                sender_database_id: sender.database_id,
                sender_unique_id: sender.unique_identifier,
                sender_name: sender.nickname,
                target_database_id: target.database_id,
                target_unique_id: target.unique_identifier,
                target_name: target.nickname,
                message,
            });
        timestamp
    }

}
