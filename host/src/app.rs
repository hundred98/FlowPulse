//! Application logic
//!
//! Contains the main application state and logic.
//! Integrates state management, message queue, and multi-channel access.

use std::sync::Arc;
use emb_public::{
    // Core client
    CoreSocketClient,
    
    // State management
    DeviceStateManager, DeviceStateConfig,
    StateMachine, StateMachineConfig,
    SafetyController, SafetyConfig,
    PrintController,
    
    // Message queue
    MessageQueue, MessageQueueConfig,
    CommandHandler, StatusHandler, ErrorHandler,
    
    // Event system
    SyncEventPublisher,
    
    // Multi-channel access
    ChannelManager, ChannelManagerConfig,
    WebSocketConfig, UnixSocketConfig, MqttConfig,
};

/// Application state containing all core components
pub struct AppState {
    /// Core socket client for communication with emb-core-server
    pub core_client: Arc<CoreSocketClient>,
    
    /// Device state manager for state synchronization
    pub device_state: Arc<DeviceStateManager>,
    
    /// State machine for printer state transitions
    pub state_machine: Arc<StateMachine>,
    
    /// Safety controller for safety checks
    pub safety_controller: Arc<SafetyController>,
    
    /// Print controller for print job management
    pub print_controller: Arc<PrintController>,
    
    /// Message queue for asynchronous command processing
    pub message_queue: Arc<MessageQueue>,
    
    /// Event publisher for event notifications
    pub event_publisher: Arc<SyncEventPublisher>,
    
    /// Channel manager for multi-channel access
    pub channel_manager: Arc<ChannelManager>,
}

impl AppState {
    /// Create a new application state with all components
    pub fn new(core_client: Arc<CoreSocketClient>) -> Self {
        // Create event publisher
        let event_publisher = Arc::new(SyncEventPublisher::new());
        
        // Create device state manager
        let device_state_config = DeviceStateConfig::default();
        let device_state = Arc::new(DeviceStateManager::new(
            core_client.clone(),
            event_publisher.clone(),
            device_state_config,
        ));
        
        // Create state machine
        let state_machine_config = StateMachineConfig::default();
        let state_machine = Arc::new(StateMachine::new(state_machine_config));
        
        // Create safety controller
        let safety_config = SafetyConfig::default();
        let safety_controller = Arc::new(SafetyController::new(
            safety_config,
            device_state.clone(),
            event_publisher.clone(),
        ));
        
        // Create print controller
        let print_controller = Arc::new(PrintController::new());
        
        // Create message queue
        let message_queue_config = MessageQueueConfig::default();
        let message_queue = Arc::new(MessageQueue::new(message_queue_config));
        
        // Create channel manager
        let channel_manager_config = ChannelManagerConfig {
            websocket: WebSocketConfig {
                bind_address: "127.0.0.1".to_string(),
                port: 8080,
                max_connections: 10,
                enable_auth: false,
                auth_token: None,
            },
            unix_socket: UnixSocketConfig {
                socket_path: "/tmp/flowpulse.sock".to_string(),
                max_connections: 5,
                buffer_size: 4096,
                enable_hmi_mode: false,
            },
            mqtt: MqttConfig::default(),
            enable_websocket: true,
            enable_unix_socket: true,
            enable_mqtt: false,
            status_broadcast_interval: 1,
        };
        
        let channel_manager = Arc::new(ChannelManager::new(
            channel_manager_config,
            message_queue.clone(),
            device_state.clone(),
            event_publisher.clone(),
        ));
        
        Self {
            core_client,
            device_state,
            state_machine,
            safety_controller,
            print_controller,
            message_queue,
            event_publisher,
            channel_manager,
        }
    }
    
    /// Initialize all components
    pub async fn initialize(&self) -> emb_public::EmbResult<()> {
        log::info!("Initializing application state...");
        
        // Register message handlers
        self.register_handlers().await?;
        
        log::info!("Application state initialized");
        Ok(())
    }
    
    /// Register all message handlers
    async fn register_handlers(&self) -> emb_public::EmbResult<()> {
        use emb_public::MessageType;
        
        // Command handler
        let command_handler = Arc::new(CommandHandler::new(
            self.device_state.clone(),
            self.state_machine.clone(),
            self.print_controller.clone(),
        ));
        
        self.message_queue.add_handler(MessageType::PrintStart, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::PrintPause, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::PrintResume, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::PrintStop, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::TemperatureSet, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::MoveCommand, command_handler.clone()).await;
        self.message_queue.add_handler(MessageType::HomeCommand, command_handler.clone()).await;
        
        // Status handler
        let status_handler = Arc::new(StatusHandler::new(
            self.device_state.clone(),
            self.state_machine.clone(),
            self.print_controller.clone(),
        ));
        
        self.message_queue.add_handler(MessageType::StateQuery, status_handler.clone()).await;
        self.message_queue.add_handler(MessageType::TemperatureGet, status_handler.clone()).await;
        self.message_queue.add_handler(MessageType::HardwareStatus, status_handler.clone()).await;
        
        // Error handler
        let error_handler = Arc::new(ErrorHandler::new(
            self.safety_controller.clone(),
            self.state_machine.clone(),
        ));
        
        self.message_queue.add_handler(MessageType::PrintError, error_handler.clone()).await;
        self.message_queue.add_handler(MessageType::HardwareError, error_handler.clone()).await;
        
        log::info!("Message handlers registered");
        Ok(())
    }
    
    /// Start all background services
    pub async fn start_services(&self) -> emb_public::EmbResult<()> {
        log::info!("Starting background services...");
        
        // Start device state synchronization loop
        let device_state_clone = self.device_state.clone();
        tokio::spawn(async move {
            device_state_clone.start_sync_loop().await;
        });
        
        // Start message queue processing
        let message_queue_clone = self.message_queue.clone();
        tokio::spawn(async move {
            if let Err(e) = message_queue_clone.start_processing().await {
                log::error!("Message queue processing error: {}", e);
            }
        });
        
        // Start multi-channel services
        self.channel_manager.start_all().await?;
        
        // Start status broadcast loop
        let channel_manager_clone = self.channel_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                
                if let Err(e) = channel_manager_clone.broadcast_status().await {
                    log::error!("Status broadcast error: {}", e);
                }
            }
        });
        
        log::info!("Background services started");
        Ok(())
    }
    
    /// Stop all services
    pub async fn stop_services(&self) -> emb_public::EmbResult<()> {
        log::info!("Stopping services...");
        
        self.channel_manager.stop_all().await?;
        self.message_queue.shutdown().await;
        
        log::info!("Services stopped");
        Ok(())
    }
}