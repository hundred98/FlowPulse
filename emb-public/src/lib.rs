//! FlowPulse emb-public library
//!
//! This library provides the public interface for FlowPulse 3D printer control system.

pub mod common;
pub mod core_client;
pub mod config;

// State management modules
pub mod state_machine;
pub mod message_queue;
pub mod print_control;
pub mod gateway;
pub mod state;  // New module (device_state, frontend_provider)
pub mod safety;  // New module (safety controller)
pub mod gcode;  // New module (gcode parser, reserved)

// Re-export common types
pub use common::{
    EmbError, EmbResult,
    PrinterEvent, EventKind, EventSeverity, EventListener,
    EventPublisher, SyncEventPublisher,
    WebSocketMessage, SharedState, PrinterStatus, TempStatus, PositionData,
};

// Re-export core client
pub use core_client::{CoreSocketClient, CoreClientConfig};

// Re-export config types
pub use config::{
    PrinterJsonConfig, PrinterParams, MotorParams, LimitSwitchParams,
    TemperatureParams, HeaterParams, FanParams, ProbeParams,
    load_config_from_file, parse_json_config,
    load_configs, build_motion_config_json, build_printer_config,
    LoadedConfigs, HardwareConfig, MotionFileConfig, PrinterFileConfig,
    ConfigFrameBuilder, create_config_frames, validate_config,
    configure_device,
};

// Re-export config submodules
pub use config::{config_adapter, config_protocol};

// Re-export state machine types
pub use state_machine::{
    PrinterState, TransitionReason, StateTransition, StateMachineConfig,
    StateMachine,
};

// Re-export message queue types
pub use message_queue::{
    Message, MessageType, MessagePriority, MessageStatus,
    MessageQueueConfig, QueueStats, MessageHandler, MessageQueue,
};

// Re-export print control types
pub use print_control::{
    PrintController, PrintJob, PrintState, TemperaturePreset,
};

// Re-export gateway types
pub use gateway::{
    CommunicationGateway, CommunicationChannel,
    ChannelType, ChannelStatus, ChannelConfig, ChannelStats, Direction,
    SerialChannelStats,
    WebSocketServer, WebSocketConfig, WebSocketConnection, WebSocketStatus,
    UnixSocketServer, UnixSocketConfig, UnixSocketConnection, UnixSocketStatus, UnixSocketClientType,
    MqttClient, MqttConfig, MqttStatus, MqttTopicType,
    ChannelManager, ChannelManagerConfig, ChannelManagerStatus,
};

// Re-export state types
pub use state::{
    DeviceStateManager, DeviceStateConfig, Position, MotionStatus, FlowStatus,
    DeviceStateSnapshot, FrontendDataProvider, UnixSocketProvider,
    EmbeddedDataProvider, WebDataProvider,
};

// Re-export safety types
pub use safety::{
    SafetyController, SafetyConfig, TemperatureLimit, MotionLimit,
    SafetyCheckResult,
};

// Re-export message queue handlers
pub use message_queue::{
    CommandHandler, StatusHandler, ErrorHandler,
};