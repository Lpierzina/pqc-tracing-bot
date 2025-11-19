use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Identifier used for logical topics, mirroring Waku's `pubsubTopic`.
pub type Topic = String;
/// Identifier used for semantic scoping inside a topic, mirroring Waku's `contentTopic`.
pub type ContentTopic = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishReceipt {
    pub topic: Topic,
    pub sequence: u64,
    pub fanout: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PubSubMessage {
    pub topic: Topic,
    pub content_topic: ContentTopic,
    pub from: String,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PubSubEnvelope {
    pub topic: Topic,
    pub content_topic: ContentTopic,
    pub from: String,
    pub payload: Vec<u8>,
    pub sequence: u64,
    pub timestamp_ms: u128,
}

struct Subscriber {
    #[allow(dead_code)]
    id: String,
    sender: Sender<PubSubEnvelope>,
}

#[derive(Default, Clone)]
pub struct PubSubRouter {
    inner: Arc<Mutex<RouterInner>>,
}

#[derive(Default)]
struct RouterInner {
    next_sequence: u64,
    topics: HashMap<Topic, Vec<Subscriber>>,
}

impl PubSubRouter {
    /// Creates a new subscriber for the given topic. Messages are fanned out via
    /// a dedicated channel so each subscriber gets an independent view.
    pub fn subscribe(
        &self,
        topic: impl Into<Topic>,
        subscriber_id: impl Into<String>,
    ) -> Subscription {
        let topic = topic.into();
        let id = subscriber_id.into();
        let (tx, rx) = mpsc::channel();
        let mut guard = self.inner.lock().unwrap();
        guard
            .topics
            .entry(topic.clone())
            .or_default()
            .push(Subscriber {
                id: id.clone(),
                sender: tx,
            });
        Subscription {
            topic,
            subscriber_id: id,
            receiver: rx,
        }
    }

    /// Broadcasts a message to all subscribers of the target topic.
    pub fn publish(&self, message: PubSubMessage) -> PublishReceipt {
        let mut guard = self.inner.lock().unwrap();
        let sequence = guard.next_sequence;
        guard.next_sequence = guard.next_sequence.wrapping_add(1);
        let timestamp_ms = now_ms();
        let envelope = PubSubEnvelope {
            sequence,
            timestamp_ms,
            topic: message.topic.clone(),
            content_topic: message.content_topic.clone(),
            from: message.from,
            payload: message.payload,
        };
        let mut delivered = 0usize;
        if let Some(subscribers) = guard.topics.get_mut(&envelope.topic) {
            subscribers.retain(
                |subscriber| match subscriber.sender.send(envelope.clone()) {
                    Ok(_) => {
                        delivered += 1;
                        true
                    }
                    Err(_) => false,
                },
            );
        }
        PublishReceipt {
            topic: envelope.topic,
            sequence,
            fanout: delivered,
        }
    }
}

#[derive(Debug)]
pub struct Subscription {
    topic: Topic,
    subscriber_id: String,
    receiver: Receiver<PubSubEnvelope>,
}

impl Subscription {
    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn subscriber_id(&self) -> &str {
        &self.subscriber_id
    }

    /// Non-blocking drain of all currently buffered envelopes.
    pub fn drain(&self) -> Vec<PubSubEnvelope> {
        let mut messages = Vec::new();
        while let Ok(message) = self.receiver.try_recv() {
            messages.push(message);
        }
        messages
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanout_to_multiple_subscribers() {
        let router = PubSubRouter::default();
        let sub_a = router.subscribe("control", "node-a");
        let sub_b = router.subscribe("control", "node-b");
        let receipt = router.publish(PubSubMessage {
            topic: "control".into(),
            content_topic: "discovery".into(),
            from: "node-a".into(),
            payload: b"ping".to_vec(),
        });
        assert_eq!(receipt.fanout, 2);
        assert_eq!(sub_a.drain().len(), 1);
        assert_eq!(sub_b.drain().len(), 1);
    }
}
