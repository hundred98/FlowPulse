//! Message queue integration example
//!
//! This example demonstrates how to integrate the message queue with
//! state management, state machine, and print controller.

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
    
    // Message queue
    MessageQueue, MessageQueueConfig,
    Message, MessageType, MessagePriority,
    CommandHandler, StatusHandler, ErrorHandler,
    
    // Event system
    SyncEventPublisher,
    
    // Result type
    EmbResult,
};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> EmbResult<()> {
    // Initialize logging
    env_logger::init();
    
    log::info!("Starting message queue integration example");
    
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
    
    // 4. Start background tasks
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
    
    log::info!("Background tasks started");
    
    // 5. Send test messages
    log::info!("Sending test messages...");
    
    // Test: Query state
    let state_query = Message::new(
        MessageType::StateQuery,
        "test".to_string(),
        serde_json::json!({}),
    );
    message_queue.enqueue(state_query).await?;
    log::info!("State query message sent");
    
    // Test: Set temperature
    let temp_set = Message::new(
        MessageType::TemperatureSet,
        "test".to_string(),
        serde_json::json!({
            "heater": "hotend",
            "temperature": 200.0,
        }),
    ).with_priority(MessagePriority::High);
    message_queue.enqueue(temp_set).await?;
    log::info!("Temperature set message sent");
    
    // Test: Get temperature
    let temp_get = Message::new(
        MessageType::TemperatureGet,
        "test".to_string(),
        serde_json::json!({
            "heater": "hotend",
        }),
    );
    message_queue.enqueue(temp_get).await?;
    log::info!("Temperature get message sent");
    
    // Test: Hardware status
    let hw_status = Message::new(
        MessageType::HardwareStatus,
        "test".to_string(),
        serde_json::json!({}),
    );
    message_queue.enqueue(hw_status).await?;
    log::info!("Hardware status message sent");
    
    // 6. Wait for processing
    log::info!("Waiting for message processing...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // 7. Check queue statistics
    let stats = message_queue.get_stats().await;
    log::info!("Queue statistics:");
    log::info!("  Total processed: {}", stats.total_processed);
    log::info!("  Pending: {}", stats.pending_count);
    log::info!("  Completed: {}", stats.completed_count);
    log::info!("  Failed: {}", stats.failed_count);
    log::info!("  Average processing time: {} ms", stats.avg_processing_time_ms);
    
    // 8. Shutdown
    log::info!("Shutting down...");
    message_queue.shutdown().await;
    
    log::info!("Message queue integration example completed");
    
    Ok(())
}