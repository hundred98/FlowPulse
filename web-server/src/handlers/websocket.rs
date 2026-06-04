//! WebSocket Handler
//!
//! WebSocket connection handler for real-time communication with web frontend.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use log::{info, warn};

use crate::WebServerState;

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebServerState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
}

/// Handle WebSocket connection
async fn handle_websocket(socket: WebSocket, state: Arc<WebServerState>) {
    let (mut sender, mut receiver) = socket.split();
    
    info!("WebSocket client connected");
    
    // Send initial status
    let initial_status = serde_json::json!({
        "type": "connected",
        "message": "WebSocket connection established"
    });
    
    if let Ok(msg) = serde_json::to_string(&initial_status) {
        if sender.send(Message::Text(msg)).await.is_err() {
            warn!("Failed to send initial status");
            return;
        }
    }
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse and handle the message
                if let Err(e) = handle_text_message(&text, &state).await {
                    warn!("Error handling message: {}", e);
                }
            }
            Ok(Message::Binary(data)) => {
                // Handle binary message (if needed)
                info!("Received binary message: {} bytes", data.len());
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket client disconnected");
                break;
            }
            Err(e) => {
                warn!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

/// Handle text message from WebSocket
async fn handle_text_message(
    text: &str,
    _state: &Arc<WebServerState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse the message
    let msg: serde_json::Value = serde_json::from_str(text)?;
    
    // Handle different message types
    if let Some(msg_type) = msg.get("type").and_then(|t| t.as_str()) {
        match msg_type {
            "ping" => {
                // Respond with pong
                info!("Received ping, sending pong");
            }
            "command" => {
                // Handle command
                if let Some(action) = msg.get("action").and_then(|a| a.as_str()) {
                    info!("Received command: {}", action);
                    // TODO: Forward to message queue
                }
            }
            _ => {
                warn!("Unknown message type: {}", msg_type);
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_message_parsing() {
        let msg = r#"{"type":"ping"}"#;
        let parsed: serde_json::Value = serde_json::from_str(msg).unwrap();
        assert_eq!(parsed["type"], "ping");
    }
}
