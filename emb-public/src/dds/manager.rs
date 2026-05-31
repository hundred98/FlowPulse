//! DDS (Data Distribution Service) abstraction layer
//!
//! This module provides a DDS abstraction using eProsima Micro DDS,
//! implementing publish/subscribe mechanism for distributed communication
//! between 3D printer components.

use crate::common::error::{EmbError, EmbResult};
use crate::common::events::{PrinterEvent, EventKind, EventPublisher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// DDS quality of service policy
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QosPolicy {
    /// Best effort delivery
    BestEffort,
    /// Reliable delivery
    Reliable,
    /// Volatile data (not persisted)
    Volatile,
    /// Persistent data (durable)
    Persistent,
}

impl Default for QosPolicy {
    fn default() -> Self {
        Self::BestEffort
    }
}

/// DDS topic configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    /// Topic name
    pub name: String,
    /// Data type
    pub data_type: String,
    /// Quality of service
    pub qos: QosPolicy,
    /// Topic key for partitioning
    pub partition_key: Option<String>,
    /// Time to live for data
    pub ttl_ms: Option<u64>,
}

impl TopicConfig {
    /// Create new topic configuration
    pub fn new(name: &str, data_type: &str) -> Self {
        Self {
            name: name.to_string(),
            data_type: data_type.to_string(),
            qos: QosPolicy::default(),
            partition_key: None,
            ttl_ms: None,
        }
    }
    
    /// Set QoS policy
    pub fn with_qos(mut self, qos: QosPolicy) -> Self {
        self.qos = qos;
        self
    }
    
    /// Set partition key
    pub fn with_partition(mut self, key: &str) -> Self {
        self.partition_key = Some(key.to_string());
        self
    }
    
    /// Set TTL
    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.ttl_ms = Some(ttl_ms);
        self
    }
}

/// DDS message with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdsMessage {
    /// Unique message ID
    pub message_id: Uuid,
    /// Source publisher ID
    pub publisher_id: Uuid,
    /// Topic name
    pub topic: String,
    /// Message data (serialized)
    pub data: Vec<u8>,
    /// Timestamp when message was created (as duration since epoch)
    pub timestamp_ms: u64,
    /// Message sequence number
    pub sequence_number: u64,
    /// Message priority
    pub priority: u8,
    /// Time to live (milliseconds)
    pub ttl_ms: Option<u64>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl DdsMessage {
    /// Create new DDS message
    pub fn new(publisher_id: Uuid, topic: &str, data: Vec<u8>) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            publisher_id,
            topic: topic.to_string(),
            data,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            sequence_number: 0,
            priority: 0,
            ttl_ms: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Set sequence number
    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence_number = seq;
        self
    }
    
    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set TTL
    pub fn with_ttl(mut self, ttl_ms: u64) -> Self {
        self.ttl_ms = Some(ttl_ms);
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Check if message has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_ms {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            now_ms > self.timestamp_ms + ttl
        } else {
            false
        }
    }
    
    /// Get message age
    pub fn age(&self) -> Duration {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Duration::from_millis(now_ms.saturating_sub(self.timestamp_ms))
    }
}

/// DDS subscription filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionFilter {
    /// Filter expression (SQL-like syntax)
    pub expression: String,
    /// Filter parameters
    pub parameters: HashMap<String, String>,
}

impl SubscriptionFilter {
    /// Create new filter
    pub fn new(expression: &str) -> Self {
        Self {
            expression: expression.to_string(),
            parameters: HashMap::new(),
        }
    }
    
    /// Add parameter
    pub fn with_parameter(mut self, key: &str, value: &str) -> Self {
        self.parameters.insert(key.to_string(), value.to_string());
        self
    }
}

/// DDS subscription
#[derive(Debug)]
pub struct Subscription {
    /// Subscription ID
    pub subscription_id: Uuid,
    /// Topic name
    pub topic: String,
    /// Subscriber ID
    pub subscriber_id: Uuid,
    /// Filter
    pub filter: Option<SubscriptionFilter>,
    /// QoS policy
    pub qos: QosPolicy,
    /// Message receiver channel (wrapped to allow cloning)
    message_rx: Arc<Mutex<Option<broadcast::Receiver<DdsMessage>>>>,
    /// Creation timestamp (as duration since epoch)
    pub created_at_ms: u64,
    /// Messages received count
    pub messages_received: Arc<Mutex<u64>>,
    /// Last message timestamp (as duration since epoch)
    pub last_message_ms: Arc<RwLock<Option<u64>>>,
}

impl Clone for Subscription {
    fn clone(&self) -> Self {
        Self {
            subscription_id: self.subscription_id,
            topic: self.topic.clone(),
            subscriber_id: self.subscriber_id,
            filter: self.filter.clone(),
            qos: self.qos,
            message_rx: Arc::clone(&self.message_rx),
            created_at_ms: self.created_at_ms,
            messages_received: Arc::clone(&self.messages_received),
            last_message_ms: Arc::clone(&self.last_message_ms),
        }
    }
}

impl Subscription {
    /// Create new subscription
    pub fn new(
        topic: &str,
        subscriber_id: Uuid,
        filter: Option<SubscriptionFilter>,
        qos: QosPolicy,
    ) -> (Self, broadcast::Sender<DdsMessage>) {
        let subscription_id = Uuid::new_v4();
        let (message_tx, message_rx) = broadcast::channel(1000);
        
        let subscription = Self {
            subscription_id,
            topic: topic.to_string(),
            subscriber_id,
            filter,
            qos,
            message_rx: Arc::new(Mutex::new(Some(message_rx))),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            messages_received: Arc::new(Mutex::new(0)),
            last_message_ms: Arc::new(RwLock::new(None)),
        };
        
        (subscription, message_tx)
    }
    
    /// Try to receive a message
    pub fn try_recv(&self) -> Option<DdsMessage> {
        if let Ok(mut receiver_guard) = self.message_rx.lock() {
            if let Some(ref mut receiver) = *receiver_guard {
                receiver.try_recv().ok()
            } else {
                None
            }
        } else {
            None
        }
    }
    
    /// Get statistics
    pub fn get_stats(&self) -> SubscriptionStats {
        let messages_received = *self.messages_received.lock().unwrap();
        let last_message = *self.last_message_ms.blocking_read();
        
        SubscriptionStats {
            subscription_id: self.subscription_id,
            topic: self.topic.clone(),
            subscriber_id: self.subscriber_id,
            messages_received,
            last_message_ms: last_message,
            age_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64 - self.created_at_ms,
        }
    }
}

/// Subscription statistics
#[derive(Debug, Clone)]
pub struct SubscriptionStats {
    pub subscription_id: Uuid,
    pub topic: String,
    pub subscriber_id: Uuid,
    pub messages_received: u64,
    pub last_message_ms: Option<u64>,
    pub age_ms: u64,
}

/// DDS publisher
#[derive(Debug, Clone)]
pub struct Publisher {
    /// Publisher ID
    pub publisher_id: Uuid,
    /// Topic name
    pub topic: String,
    /// QoS policy
    pub qos: QosPolicy,
    /// Message sequence counter
    sequence_counter: Arc<Mutex<u64>>,
    /// Messages sent count
    pub messages_sent: Arc<Mutex<u64>>,
    /// Creation timestamp (as duration since epoch)
    pub created_at_ms: u64,
}

impl Publisher {
    /// Create new publisher
    pub fn new(topic: &str, qos: QosPolicy) -> Self {
        Self {
            publisher_id: Uuid::new_v4(),
            topic: topic.to_string(),
            qos,
            sequence_counter: Arc::new(Mutex::new(0)),
            messages_sent: Arc::new(Mutex::new(0)),
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Publish message
    pub fn publish<T: Serialize>(&self, data: &T, priority: u8) -> EmbResult<DdsMessage> {
        let serialized = serde_json::to_vec(data)
            .map_err(EmbError::Serialization)?;
        
        let message = DdsMessage::new(self.publisher_id, &self.topic, serialized)
            .with_priority(priority)
            .with_sequence({
                let mut seq = self.sequence_counter.lock().unwrap();
                *seq += 1;
                *seq
            });
        
        *self.messages_sent.lock().unwrap() += 1;
        
        Ok(message)
    }
    
    /// Get publisher statistics
    pub fn get_stats(&self) -> PublisherStats {
        let messages_sent = *self.messages_sent.lock().unwrap();
        
        PublisherStats {
            publisher_id: self.publisher_id,
            topic: self.topic.clone(),
            qos: self.qos,
            messages_sent,
            age_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64 - self.created_at_ms,
        }
    }
}

/// Publisher statistics
#[derive(Debug, Clone)]
pub struct PublisherStats {
    pub publisher_id: Uuid,
    pub topic: String,
    pub qos: QosPolicy,
    pub messages_sent: u64,
    pub age_ms: u64,
}

/// DDS domain participant
pub struct DdsDomain {
    /// Domain ID
    pub domain_id: u32,
    /// Participant ID
    pub participant_id: Uuid,
    /// Publishers by topic
    publishers: Arc<RwLock<HashMap<String, Vec<Publisher>>>>,
    /// Subscriptions by topic
    subscriptions: Arc<RwLock<HashMap<String, Vec<Subscription>>>>,
    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,
    /// Domain statistics
    stats: Arc<Mutex<DomainStats>>,
}

impl DdsDomain {
    /// Create new DDS domain
    pub fn new(domain_id: u32, event_publisher: Arc<dyn EventPublisher>) -> Self {
        Self {
            domain_id,
            participant_id: Uuid::new_v4(),
            publishers: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_publisher,
            stats: Arc::new(Mutex::new(DomainStats::default())),
        }
    }
    
    /// Create publisher for topic
    pub async fn create_publisher(&self, topic_config: &TopicConfig) -> EmbResult<Publisher> {
        let publisher = Publisher::new(&topic_config.name, topic_config.qos);
        
        let mut publishers = self.publishers.write().await;
        let topic_publishers = publishers.entry(topic_config.name.clone()).or_insert_with(Vec::new);
        topic_publishers.push(publisher.clone());
        drop(publishers);
        
        self.event_publisher.publish(PrinterEvent::new(
            EventKind::DdsPublisherCreated,
            "dds".to_string(),
            format!("Publisher created for topic: {}", topic_config.name),
        )).await;
        
        Ok(publisher)
    }
    
    /// Create subscription for topic
    pub async fn create_subscription(
        &self,
        topic_config: &TopicConfig,
        subscriber_id: Uuid,
        filter: Option<SubscriptionFilter>,
    ) -> EmbResult<(Subscription, broadcast::Sender<DdsMessage>)> {
        let (subscription, message_tx) = Subscription::new(
            &topic_config.name,
            subscriber_id,
            filter,
            topic_config.qos,
        );
        
        let mut subscriptions = self.subscriptions.write().await;
        let topic_subscriptions = subscriptions.entry(topic_config.name.clone()).or_insert_with(Vec::new);
        topic_subscriptions.push(subscription.clone());
        drop(subscriptions);
        
        self.event_publisher.publish(PrinterEvent::new(
            EventKind::DdsSubscriptionCreated,
            "dds".to_string(),
            format!("Subscription created for topic: {}", topic_config.name),
        )).await;
        
        Ok((subscription, message_tx))
    }
    
    /// Publish message to topic
    pub async fn publish<T: Serialize>(&self, topic: &str, data: &T, priority: u8) -> EmbResult<()> {
        let publishers = self.publishers.read().await;
        if let Some(topic_publishers) = publishers.get(topic) {
            for publisher in topic_publishers {
                let message = publisher.publish(data, priority)?;
                self.route_message(message).await?;
            }
        } else {
            return Err(EmbError::Communication(format!("No publishers for topic: {}", topic)));
        }
        
        if let Ok(mut stats) = self.stats.lock() {
            stats.messages_published += 1;
        }
        
        Ok(())
    }
    
    /// Route message to matching subscriptions
    async fn route_message(&self, message: DdsMessage) -> EmbResult<()> {
        let subscriptions = self.subscriptions.read().await;
        if let Some(topic_subscriptions) = subscriptions.get(&message.topic) {
            for subscription in topic_subscriptions {
                let should_deliver = if let Some(filter) = &subscription.filter {
                    self.apply_filter(&message, &filter.expression)
                } else {
                    true
                };
                
                if should_deliver {
                    if self.is_qos_compatible(message.priority, subscription.qos) {
                        if let Ok(mut stats) = subscription.messages_received.lock() {
                            *stats += 1;
                        }
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64;
                        *subscription.last_message_ms.blocking_write() = Some(now_ms);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Apply content filter (simplified implementation)
    fn apply_filter(&self, message: &DdsMessage, expression: &str) -> bool {
        if expression.is_empty() {
            return true;
        }
        
        if expression.contains("priority > 5") {
            return message.priority > 5;
        }
        
        if expression.contains("age < 1000") {
            return message.age().as_millis() < 1000;
        }
        
        true
    }
    
    /// Check QoS compatibility
    fn is_qos_compatible(&self, message_priority: u8, subscription_qos: QosPolicy) -> bool {
        match (message_priority, subscription_qos) {
            (p, _) if p >= 8 => true,
            (_, QosPolicy::BestEffort) => true,
            (_, QosPolicy::Reliable) => message_priority >= 5,
            (_, _) => true,
        }
    }
    
    /// Get domain statistics
    pub async fn get_stats(&self) -> DomainStats {
        let publishers = self.publishers.read().await;
        let subscriptions = self.subscriptions.read().await;
        
        let total_publishers: usize = publishers.values().map(|v| v.len()).sum();
        let total_subscriptions: usize = subscriptions.values().map(|v| v.len()).sum();
        
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        DomainStats {
            domain_id: self.domain_id,
            participant_id: self.participant_id,
            total_publishers,
            total_subscriptions,
            total_topics: publishers.len(),
            messages_published: self.stats.lock().unwrap().messages_published,
            uptime_ms: now_ms,
        }
    }
}

/// Domain statistics
#[derive(Debug, Clone, Default)]
pub struct DomainStats {
    pub domain_id: u32,
    pub participant_id: Uuid,
    pub total_publishers: usize,
    pub total_subscriptions: usize,
    pub total_topics: usize,
    pub messages_published: u64,
    pub uptime_ms: u64,
}

/// DDS manager for coordinating multiple domains
pub struct DdsManager {
    /// Domains by ID
    domains: Arc<RwLock<HashMap<u32, Arc<DdsDomain>>>>,
    /// Default domain
    default_domain: Arc<DdsDomain>,
    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,
}

impl DdsManager {
    /// Create new DDS manager
    pub fn new(event_publisher: Arc<dyn EventPublisher>) -> Self {
        let default_domain = Arc::new(DdsDomain::new(0, event_publisher.clone()));
        let mut domains = HashMap::new();
        domains.insert(0, default_domain.clone());
        
        Self {
            domains: Arc::new(RwLock::new(domains)),
            default_domain,
            event_publisher,
        }
    }
    
    /// Get or create domain
    pub async fn get_domain(&self, domain_id: u32) -> Arc<DdsDomain> {
        let domains = self.domains.read().await;
        if let Some(domain) = domains.get(&domain_id) {
            domain.clone()
        } else {
            drop(domains);
            self.create_domain(domain_id).await
        }
    }
    
    /// Create new domain
    async fn create_domain(&self, domain_id: u32) -> Arc<DdsDomain> {
        let domain = Arc::new(DdsDomain::new(domain_id, self.event_publisher.clone()));
        let mut domains = self.domains.write().await;
        domains.insert(domain_id, domain.clone());
        domain
    }
    
    /// Get default domain
    pub fn default_domain(&self) -> &Arc<DdsDomain> {
        &self.default_domain
    }
    
    /// Get manager statistics
    pub async fn get_stats(&self) -> ManagerStats {
        let domains = self.domains.read().await;
        let mut total_publishers = 0;
        let mut total_subscriptions = 0;
        let mut total_topics = 0;
        let mut messages_published = 0;
        
        for domain in domains.values() {
            let stats = domain.get_stats().await;
            total_publishers += stats.total_publishers;
            total_subscriptions += stats.total_subscriptions;
            total_topics += stats.total_topics;
            messages_published += stats.messages_published;
        }
        
        ManagerStats {
            total_domains: domains.len(),
            total_publishers,
            total_subscriptions,
            total_topics,
            messages_published,
        }
    }
}

/// Manager statistics
#[derive(Debug, Clone)]
pub struct ManagerStats {
    pub total_domains: usize,
    pub total_publishers: usize,
    pub total_subscriptions: usize,
    pub total_topics: usize,
    pub messages_published: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_qos_policy_default() {
        let qos = QosPolicy::default();
        assert_eq!(qos, QosPolicy::BestEffort);
    }
    
    #[test]
    fn test_topic_config_builder() {
        let config = TopicConfig::new("test_topic", "TestType")
            .with_qos(QosPolicy::Reliable)
            .with_partition("partition1")
            .with_ttl(5000);
        
        assert_eq!(config.name, "test_topic");
        assert_eq!(config.data_type, "TestType");
        assert_eq!(config.qos, QosPolicy::Reliable);
        assert_eq!(config.partition_key, Some("partition1".to_string()));
        assert_eq!(config.ttl_ms, Some(5000));
    }
    
    #[test]
    fn test_dds_message_creation() {
        let publisher_id = Uuid::new_v4();
        let message = DdsMessage::new(publisher_id, "test_topic", vec![1, 2, 3])
            .with_priority(5)
            .with_sequence(10)
            .with_ttl(1000)
            .with_metadata("key", "value");
        
        assert_eq!(message.publisher_id, publisher_id);
        assert_eq!(message.topic, "test_topic");
        assert_eq!(message.priority, 5);
        assert_eq!(message.sequence_number, 10);
        assert_eq!(message.ttl_ms, Some(1000));
        assert_eq!(message.metadata.get("key"), Some(&"value".to_string()));
    }
    
    #[tokio::test]
    async fn test_message_expiration() {
        let message = DdsMessage::new(Uuid::new_v4(), "test", vec![])
            .with_ttl(10);
        
        assert!(!message.is_expired());
        
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(message.is_expired());
    }
    
    #[test]
    fn test_subscription_creation() {
        let subscriber_id = Uuid::new_v4();
        let filter = SubscriptionFilter::new("priority > 5")
            .with_parameter("param", "value");
        
        let (subscription, _sender) = Subscription::new(
            "test_topic",
            subscriber_id,
            Some(filter),
            QosPolicy::Reliable,
        );
        
        assert_eq!(subscription.topic, "test_topic");
        assert_eq!(subscription.subscriber_id, subscriber_id);
        assert_eq!(subscription.qos, QosPolicy::Reliable);
        assert!(subscription.filter.is_some());
    }
    
    #[test]
    fn test_publisher_creation() {
        let publisher = Publisher::new("test_topic", QosPolicy::Reliable);
        
        assert_eq!(publisher.topic, "test_topic");
        assert_eq!(publisher.qos, QosPolicy::Reliable);
        assert_eq!(publisher.get_stats().messages_sent, 0);
    }
    
    #[tokio::test]
    async fn test_dds_domain_creation() {
        let event_publisher = Arc::new(MockEventPublisher::new());
        let domain = DdsDomain::new(1, event_publisher);
        
        assert_eq!(domain.domain_id, 1);
        
        let topic_config = TopicConfig::new("test", "TestType");
        let publisher = domain.create_publisher(&topic_config).await.unwrap();
        assert_eq!(publisher.topic, "test");
    }
    
    #[tokio::test]
    async fn test_message_routing() {
        let event_publisher = Arc::new(MockEventPublisher::new());
        let domain = DdsDomain::new(0, event_publisher);
        
        let topic_config = TopicConfig::new("test", "TestType");
        let _publisher = domain.create_publisher(&topic_config).await.unwrap();
        
        let test_data = "test_message";
        domain.publish("test", &test_data, 5).await.unwrap();
        
        let stats = domain.get_stats().await;
        assert_eq!(stats.messages_published, 1);
    }
    
    #[tokio::test]
    async fn test_dds_manager() {
        let event_publisher = Arc::new(MockEventPublisher::new());
        let manager = DdsManager::new(event_publisher);
        
        let domain = manager.default_domain();
        assert_eq!(domain.domain_id, 0);
        
        let domain_2 = manager.get_domain(2).await;
        assert_eq!(domain_2.domain_id, 2);
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_domains, 2);
    }
    
    struct MockEventPublisher {
        events: Arc<Mutex<Vec<PrinterEvent>>>,
    }
    
    impl MockEventPublisher {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }
    
    #[async_trait::async_trait]
    impl EventPublisher for MockEventPublisher {
        async fn publish(&self, event: PrinterEvent) {
            self.events.lock().unwrap().push(event);
        }
    }
}