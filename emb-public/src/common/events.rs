//! Event types for the 3D printer system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Event kinds that can occur in the printer system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    StateChanged,
    StateTransitionRequested,
    StateTransitionFailed,
    PrintStarted,
    PrintPaused,
    PrintResumed,
    PrintCompleted,
    PrintCancelled,
    PrintFailed,
    TemperatureUpdate,
    PositionUpdate,
    LimitSwitchTriggered,
    MotorError,
    MessageReceived,
    MessageSent,
    ConnectionEstablished,
    ConnectionLost,
    DdsPublisherCreated,
    DdsSubscriptionCreated,
    DdsMessagePublished,
    DdsMessageReceived,
    Error,
    Warning,
    Info,
    Debug,
}

/// Event severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// A printer event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterEvent {
    pub id: Uuid,
    pub kind: EventKind,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub severity: EventSeverity,
}

impl PrinterEvent {
    pub fn new(kind: EventKind, source: String, message: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            timestamp: Utc::now(),
            source,
            message,
            data: None,
            severity: EventSeverity::Info,
        }
    }

    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn state_change<T: serde::Serialize + std::fmt::Debug>(
        from_state: T,
        to_state: T,
    ) -> Self {
        Self::new(
            EventKind::StateChanged,
            "state_machine".to_string(),
            format!("State changed from {:?} to {:?}", from_state, to_state),
        )
        .with_data(serde_json::json!({
            "from": from_state,
            "to": to_state
        }))
    }

    pub fn error(source: String, message: String) -> Self {
        Self::new(EventKind::Error, source, message)
            .with_severity(EventSeverity::Error)
    }

    pub fn warning(source: String, message: String) -> Self {
        Self::new(EventKind::Warning, source, message)
            .with_severity(EventSeverity::Warning)
    }

    pub fn info(source: String, message: String) -> Self {
        Self::new(EventKind::Info, source, message)
            .with_severity(EventSeverity::Info)
    }
}

/// Event listener trait
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &PrinterEvent);
}

/// Event publisher trait for async publishing
#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: PrinterEvent);
}

/// Event publisher for distributing events (synchronous version)
pub struct SyncEventPublisher {
    listeners: Vec<Box<dyn EventListener>>,
}

impl SyncEventPublisher {
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    pub fn add_listener(&mut self, listener: Box<dyn EventListener>) {
        self.listeners.push(listener);
    }

    pub fn publish(&self, event: PrinterEvent) {
        for listener in &self.listeners {
            listener.on_event(&event);
        }
    }
}

impl Default for SyncEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}
