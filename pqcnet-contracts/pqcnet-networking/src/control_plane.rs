use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pubsub::{
    ContentTopic, PubSubMessage, PubSubRouter, PublishReceipt, Subscription, Topic,
};
use crate::qs_dag::StateDiff;

const DEFAULT_DISCOVERY_TOPIC: &str = "/waku/2/pqcnet/discovery";
const DEFAULT_CONTROL_TOPIC: &str = "/waku/2/pqcnet/control";
const DEFAULT_DISCOVERY_CONTENT: &str = "/waku/2/pqcnet/discovery/node";
const DEFAULT_CONTROL_CONTENT: &str = "/waku/2/pqcnet/control/command";

#[derive(Clone, Debug)]
pub struct ControlPlaneConfig {
    pub discovery_topic: Topic,
    pub control_topic: Topic,
    pub discovery_content_topic: ContentTopic,
    pub control_content_topic: ContentTopic,
}

impl Default for ControlPlaneConfig {
    fn default() -> Self {
        Self {
            discovery_topic: DEFAULT_DISCOVERY_TOPIC.into(),
            control_topic: DEFAULT_CONTROL_TOPIC.into(),
            discovery_content_topic: DEFAULT_DISCOVERY_CONTENT.into(),
            control_content_topic: DEFAULT_CONTROL_CONTENT.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeAnnouncement {
    pub node_id: String,
    #[serde(default)]
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl NodeAnnouncement {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            endpoints: Vec::new(),
            capabilities: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControlCommand {
    Ping { nonce: u64 },
    StateSync { diff: StateDiff },
    Custom { name: String, payload: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    Discovery(NodeAnnouncement),
    Command(ControlCommand),
}

#[derive(Debug, Error)]
pub enum ControlPlaneError {
    #[error("serialization failure: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct ControlPlane {
    node_id: String,
    router: PubSubRouter,
    config: ControlPlaneConfig,
    discovery_sub: Subscription,
    control_sub: Subscription,
}

impl ControlPlane {
    pub fn new(
        node_id: impl Into<String>,
        router: PubSubRouter,
        config: ControlPlaneConfig,
    ) -> Self {
        let node_id = node_id.into();
        let discovery_sub =
            router.subscribe(&config.discovery_topic, format!("{}-discovery", node_id));
        let control_sub = router.subscribe(&config.control_topic, format!("{}-control", node_id));
        Self {
            node_id,
            router,
            config,
            discovery_sub,
            control_sub,
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn announce(
        &self,
        announcement: NodeAnnouncement,
    ) -> Result<PublishReceipt, ControlPlaneError> {
        self.publish(
            self.config.discovery_topic.clone(),
            self.config.discovery_content_topic.clone(),
            announcement,
        )
    }

    pub fn broadcast_command(
        &self,
        command: ControlCommand,
    ) -> Result<PublishReceipt, ControlPlaneError> {
        self.publish(
            self.config.control_topic.clone(),
            self.config.control_content_topic.clone(),
            command,
        )
    }

    pub fn poll_events(&self) -> Result<Vec<ControlEvent>, ControlPlaneError> {
        let mut events = Vec::new();
        for envelope in self.discovery_sub.drain() {
            let announcement: NodeAnnouncement = serde_json::from_slice(&envelope.payload)?;
            events.push(ControlEvent::Discovery(announcement));
        }
        for envelope in self.control_sub.drain() {
            let command: ControlCommand = serde_json::from_slice(&envelope.payload)?;
            events.push(ControlEvent::Command(command));
        }
        Ok(events)
    }

    fn publish<T>(
        &self,
        topic: Topic,
        content_topic: ContentTopic,
        value: T,
    ) -> Result<PublishReceipt, ControlPlaneError>
    where
        T: Serialize,
    {
        let payload = serde_json::to_vec(&value)?;
        Ok(self.router.publish(PubSubMessage {
            topic,
            content_topic,
            from: self.node_id.clone(),
            payload,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_messages_are_received() {
        let router = PubSubRouter::default();
        let plane_a = ControlPlane::new("node-a", router.clone(), ControlPlaneConfig::default());
        let plane_b = ControlPlane::new("node-b", router.clone(), ControlPlaneConfig::default());

        plane_a
            .announce(NodeAnnouncement::new("node-a"))
            .expect("announcement succeeds");

        let events = plane_b.poll_events().expect("events polled");
        assert!(matches!(
            events.first(),
            Some(ControlEvent::Discovery(NodeAnnouncement { node_id, .. })) if node_id == "node-a"
        ));
    }
}
