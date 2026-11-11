use crate::{
    error::Error,
    services::email::{EmailMessage, SendEmail},
};
use async_trait::async_trait;
use std::collections::VecDeque;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct MockSender {
    messages: Option<Mutex<VecDeque<EmailMessage>>>,
}

impl MockSender {
    pub fn recording() -> Self {
        Self {
            messages: Some(Mutex::new(VecDeque::new())),
        }
    }

    pub async fn take_messages(&self) -> Vec<EmailMessage> {
        match &self.messages {
            Some(messages) => messages.lock().await.drain(..).collect(),
            None => Vec::new(),
        }
    }
}

#[async_trait]
impl SendEmail for MockSender {
    async fn send(&self, message: EmailMessage) -> Result<(), Error> {
        println!(
            "\nMock email\nFrom: {}\nTo: {}\nSubject: {}\n{}\n",
            message.from, message.destination, message.subject, message.body
        );
        if let Some(inbox) = &self.messages {
            let mut inbox = inbox.lock().await;
            if inbox.len() == 100 {
                inbox.pop_front();
            }
            inbox.push_back(message);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(subject: &str) -> EmailMessage {
        EmailMessage {
            from: "notes@example.test".into(),
            destination: "user@example.test".into(),
            subject: subject.into(),
            body: "Your verification code is: ABCD1234".into(),
        }
    }

    #[actix_web::test]
    async fn default_sender_accepts_messages_without_recording_them() {
        let sender = MockSender::default();
        sender.send(message("Verify your email")).await.unwrap();
        assert!(sender.take_messages().await.is_empty());
    }

    #[actix_web::test]
    async fn recording_preserves_message_contents_and_order_and_drains_the_inbox() {
        let sender = MockSender::recording();
        assert!(sender.take_messages().await.is_empty());
        sender.send(message("Verify your email")).await.unwrap();
        sender.send(message("Reset your password")).await.unwrap();

        let messages = sender.take_messages().await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].subject, "Verify your email");
        assert_eq!(messages[1].subject, "Reset your password");
        for message in &messages {
            assert_eq!(message.from, "notes@example.test");
            assert_eq!(message.destination, "user@example.test");
            assert_eq!(message.body, "Your verification code is: ABCD1234");
        }
        assert!(sender.take_messages().await.is_empty());

        sender.send(message("New sign-in")).await.unwrap();
        let messages = sender.take_messages().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "New sign-in");
    }

    #[actix_web::test]
    async fn recording_retains_only_the_newest_hundred_messages_in_order() {
        let sender = MockSender::recording();
        for index in 0..103 {
            sender.send(message(&index.to_string())).await.unwrap();
        }

        let messages = sender.take_messages().await;
        let subjects: Vec<_> = messages
            .iter()
            .map(|message| message.subject.as_str())
            .collect();
        let expected: Vec<_> = (3..103).map(|index| index.to_string()).collect();
        assert_eq!(subjects, expected);
        assert!(sender.take_messages().await.is_empty());
    }
}
