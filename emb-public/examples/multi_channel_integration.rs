//! Multi-channel integration example
//!
//! This example demonstrates how to use the multi-channel access system
//! with WebSocket, UnixSocket, and MQTT.

use emb_public::{
    // Core components
    CoreSocketClient, CoreClientConfig,

    // State management
    DeviceStateManager, DeviceStateConfig,

    // State machine
    StateMachine, StateMachineConfig,

    // Safety controller
    SafetyController, SafetyConfig,

    // Print controller
    PrintController,

    // Temperature management
    TemperatureManager,
    temperature::TemperatureManagerConfig,

    // Message queue
    MessageQueue, MessageQueueConfig,
    Message, MessageType, MessagePriority,
    CommandHandler, StatusHandler, ErrorHandler,

    // Event system
    SyncEventPublisher,

    // Gateway channels
    ChannelManager, ChannelManagerConfig,
    WebSocketConfig, UnixSocketConfig, MqttConfig,

    // Result type
    EmbResult,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> EmbResult<()> {
    // Initialize logging
    env_logger::init();
    
    log::info!("Starting multi-channel integration example");
    
    // 1. Create core components
    log::info!("Creating core components...");
    
    // Core socket client
    let core_client_config = CoreClientConfig::default();
    let core_client = Arc::new(CoreSocketClient::new(core_client_config));
    
    // Event publisher
    let event_publisher = Arc::new(SyncEventPublisher::new());
    
    // Device state manager
    let device_state_config = DeviceStateConfig::default();
    let device_state = Arc::new(DeviceStateManager::new(
        core_client.clone(),
        event_publisher.clone(),
        device_state_config,
    ));
    
    // State machine
    let state_machine_config = StateMachineConfig::default();
    let state_machine = Arc::new(StateMachine::new(state_machine_config));
    
    // Safety controller
    let safety_config = SafetyConfig::default();
    let safety_controller = Arc::new(SafetyController::new(
        safety_config,
        device_state.clone(),
        event_publisher.clone(),
    ));
    
    // Print controller
    let print_controller = Arc::new(PrintController::new());

    // Temperature manager
    let temperature_manager = Arc::new(TemperatureManager::new(
        core_client.clone(),
        event_publisher.clone(),
        TemperatureManagerConfig::default(),
        None,
    ));

    log::info!("Core components created");
    
    // 2. Create message queue
    log::info!("Creating message queue...");
    
    let message_queue_config = MessageQueueConfig::default();
    let message_queue = Arc::new(MessageQueue::new(message_queue_config));
    
    log::info!("Message queue created");
    
    // 3. Register message handlers
    log::info!("Registering message handlers...");
    
    // Command handler
    let command_handler = Arc::new(CommandHandler::new(
        device_state.clone(),
        state_machine.clone(),
        print_controller.clone(),
        temperature_manager.clone(),
    ));
    message_queue.add_handler(MessageType::PrintStart, command_handler.clone()).await;
    message_queue.add_handler(MessageType::PrintPause, command_handler.clone()).await;
    message_queue.add_handler(MessageType::PrintResume, command_handler.clone()).await;
    message_queue.add_handler(MessageType::PrintStop, command_handler.clone()).await;
    message_queue.add_handler(MessageType::TemperatureSet, command_handler.clone()).await;
    message_queue.add_handler(MessageType::MoveCommand, command_handler.clone()).await;
    message_queue.add_handler(MessageType::HomeCommand, command_handler.clone()).await;

    // Status handler
    let status_handler = Arc::new(StatusHandler::new(
        device_state.clone(),
        state_machine.clone(),
        print_controller.clone(),
        temperature_manager.clone(),
    ));
    message_queue.add_handler(MessageType::StateQuery, status_handler.clone()).await;
    message_queue.add_handler(MessageType::TemperatureGet, status_handler.clone()).await;
    message_queue.add_handler(MessageType::HardwareStatus, status_handler.clone()).await;
    
    // Error handler
    let error_handler = Arc::new(ErrorHandler::new(
        safety_controller.clone(),
        state_machine.clone(),
    ));
    message_queue.add_handler(MessageType::PrintError, error_handler.clone()).await;
    message_queue.add_handler(MessageType::HardwareError, error_handler.clone()).await;
    
    log::info!("Message handlers registered");
    
    // 4. Create channel manager
    log::info!("Creating channel manager...");
    
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
        mqtt: MqttConfig {
            broker_address: "mqtt.example.com".to_string(),
            port: 1883,
            client_id: "flowpulse_client".to_string(),
            username: None,
            password: None,
            topic_prefix: "flowpulse".to_string(),
            enable_tls: false,
            keep_alive: 60,
            clean_session: true,
            qos: 1,
        },
        enable_websocket: true,
        enable_unix_socket: true,
        enable_mqtt: false,  // MQTT disabled by default
        status_broadcast_interval: 1,
    };
    
    let channel_manager = Arc::new(ChannelManager::new(
        channel_manager_config,
        message_queue.clone(),
        device_state.clone(),
        temperature_manager.clone(),
        event_publisher.clone(),
    ));
    
    log::info!("Channel manager created");
    
    // 5. Start all channels
    log::info!("Starting all channels...");
    
    channel_manager.start_all().await?;
    
    log::info!("All channels started");
    
    // 6. Start background tasks
    log::info!("Starting background tasks...");
    
    // Start device state synchronization loop
    let device_state_clone = device_state.clone();
    tokio::spawn(async move {
        device_state_clone.start_sync_loop().await;
    });
    
    // Start message queue processing
    let message_queue_clone = message_queue.clone();
    tokio::spawn(async move {
        if let Err(e) = message_queue_clone.start_processing().await {
            log::error!("Message queue processing error: {}", e);
        }
    });
    
    // Start status broadcast loop
    let channel_manager_clone = channel_manager.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            
            if let Err(e) = channel_manager_clone.broadcast_status().await {
                log::error!("Status broadcast error: {}", e);
            }
        }
    });
    
    log::info!("Background tasks started");
    
    // 7. Send test messages through different channels
    log::info!("Sending test messages...");
    
    // Test: Send message through WebSocket
    let ws_msg = Message::new(
        MessageType::StateQuery,
        "websocket".to_string(),
        serde_json::json!({}),
    ).with_destination("websocket".to_string());
    message_queue.enqueue(ws_msg).await?;
    log::info!("WebSocket message sent");
    
    // Test: Send message through UnixSocket
    let unix_msg = Message::new(
        MessageType::TemperatureGet,
        "unix_socket".to_string(),
        serde_json::json!({
            "heater": "hotend",
        }),
    ).with_destination("unix_socket".to_string());
    message_queue.enqueue(unix_msg).await?;
    log::info!("UnixSocket message sent");
    
    // 8. Wait for processing
    log::info!("Waiting for message processing...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    // 9. Check channel manager status
    let status = channel_manager.get_status().await;
    log::info!("Channel manager status:");
    log::info!("  WebSocket running: {}", status.websocket_running);
    log::info!("  UnixSocket running: {}", status.unix_socket_running);
    log::info!("  MQTT connected: {}", status.mqtt_connected);
    log::info!("  Total messages routed: {}", status.total_messages_routed);
    
    // 10. Shutdown
    log::info!("Shutting down...");
    
    channel_manager.stop_all().await?;
    message_queue.shutdown().await;
    
    log::info!("Multi-channel integration example completed");
    
    Ok(())
}