//! Event types for the 3D printer system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::fmt;

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
    SafetyWarning,
    // PID tuning events
    PidTuneStarted,
    PidTuneProgress,
    PidTuneCycleComplete,
    PidTuneCompleted,
    PidTuneFailed,
    PidTuneCancelled,
    PidParamsApplied,
    PidTuneVerificationResult,
    Error,
    Warning,
    Info,
    Debug,
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventKind::StateChanged => write!(f, "StateChanged"),
            EventKind::StateTransitionRequested => write!(f, "StateTransitionRequested"),
            EventKind::StateTransitionFailed => write!(f, "StateTransitionFailed"),
            EventKind::PrintStarted => write!(f, "PrintStarted"),
            EventKind::PrintPaused => write!(f, "PrintPaused"),
            EventKind::PrintResumed => write!(f, "PrintResumed"),
            EventKind::PrintCompleted => write!(f, "PrintCompleted"),
            EventKind::PrintCancelled => write!(f, "PrintCancelled"),
            EventKind::PrintFailed => write!(f, "PrintFailed"),
            EventKind::TemperatureUpdate => write!(f, "TemperatureUpdate"),
            EventKind::PositionUpdate => write!(f, "PositionUpdate"),
            EventKind::LimitSwitchTriggered => write!(f, "LimitSwitchTriggered"),
            EventKind::MotorError => write!(f, "MotorError"),
            EventKind::MessageReceived => write!(f, "MessageReceived"),
            EventKind::MessageSent => write!(f, "MessageSent"),
            EventKind::ConnectionEstablished => write!(f, "ConnectionEstablished"),
            EventKind::ConnectionLost => write!(f, "ConnectionLost"),
            EventKind::DdsPublisherCreated => write!(f, "DdsPublisherCreated"),
            EventKind::DdsSubscriptionCreated => write!(f, "DdsSubscriptionCreated"),
            EventKind::DdsMessagePublished => write!(f, "DdsMessagePublished"),
            EventKind::DdsMessageReceived => write!(f, "DdsMessageReceived"),
            EventKind::SafetyWarning => write!(f, "SafetyWarning"),
            EventKind::PidTuneStarted => write!(f, "PidTuneStarted"),
            EventKind::PidTuneProgress => write!(f, "PidTuneProgress"),
            EventKind::PidTuneCycleComplete => write!(f, "PidTuneCycleComplete"),
            EventKind::PidTuneCompleted => write!(f, "PidTuneCompleted"),
            EventKind::PidTuneFailed => write!(f, "PidTuneFailed"),
            EventKind::PidTuneCancelled => write!(f, "PidTuneCancelled"),
            EventKind::PidParamsApplied => write!(f, "PidParamsApplied"),
            EventKind::PidTuneVerificationResult => write!(f, "PidTuneVerificationResult"),
            EventKind::Error => write!(f, "Error"),
            EventKind::Warning => write!(f, "Warning"),
            EventKind::Info => write!(f, "Info"),
            EventKind::Debug => write!(f, "Debug"),
        }
    }
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

impl fmt::Display for EventSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventSeverity::Debug => write!(f, "Debug"),
            EventSeverity::Info => write!(f, "Info"),
            EventSeverity::Warning => write!(f, "Warning"),
            EventSeverity::Error => write!(f, "Error"),
            EventSeverity::Critical => write!(f, "Critical"),
        }
    }
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

    pub fn publish_sync(&self, event: PrinterEvent) {
        for listener in &self.listeners {
            listener.on_event(&event);
        }
    }
}

#[async_trait::async_trait]
impl EventPublisher for SyncEventPublisher {
    async fn publish(&self, event: PrinterEvent) {
        self.publish_sync(event);
    }
}

impl Default for SyncEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}
