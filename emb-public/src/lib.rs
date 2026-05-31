pub mod common;
pub mod gcode;
pub mod temperature;
pub mod print_control;
pub mod gateway;
pub mod dds;
pub mod message_queue;
pub mod state_machine;
pub mod shared_memory;
pub mod core_client;
pub mod config_adapter;
pub mod printer_config;
pub mod config_protocol;

pub use common::{
    EmbError, EmbResult, PrinterEvent, EventKind, EventSeverity,
    EventListener, EventPublisher, SyncEventPublisher,
    init_debug, is_debug_enabled,
};
pub use temperature::{TemperatureController, TemperatureState, PidParams};
pub use print_control::{PrintController, PrintJob, PrintState};
pub use gateway::CommunicationGateway;
pub use dds::{
    DdsManager, DdsDomain, Publisher, Subscription, DdsMessage, TopicConfig,
    QosPolicy, SubscriptionFilter, PublisherStats, SubscriptionStats, DomainStats, ManagerStats,
};
pub use message_queue::{
    MessageQueue, Message, MessagePriority, MessageType, MessageHandler, MessageQueueConfig,
    MessageStatus, QueueStats,
};
pub use state_machine::{
    StateMachine, PrinterState, StateTransition, TransitionReason, StateMachineConfig,
};
pub use core_client::{CoreSocketClient, CoreClientConfig};
pub use shared_memory::{
    SharedMemoryManager, SharedMemoryHandle, MemoryRegion, SharedMemoryConfig,
    SharedMemoryPermissions, SharedMemoryError, SharedMemoryStats,
    SharedMemoryServer, PrinterCmd, PrinterState as ShmPrinterState, 
    PrinterStatus as ShmPrinterStatus, PrinterCommand as ShmPrinterCommand,
    SharedMemory as ShmSharedMemory, SHM_NAME, SHM_MAGIC, SHM_SIZE,
};
pub use printer_config::{
    PrinterJsonConfig, PrinterParams, MotorParams, LimitSwitchParams,
    TemperatureParams, HeaterParams, FanParams, ProbeParams,
    load_config_from_file, parse_json_config,
};
pub use config_protocol::{
    ConfigFrameBuilder, create_config_frames, validate_config,
};