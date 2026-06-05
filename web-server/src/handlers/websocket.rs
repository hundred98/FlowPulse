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
use tokio::sync::broadcast;

use crate::WebServerState;
use emb_public::common::WebSocketMessage;

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
    
    // Subscribe to temperature updates (if WebDataProvider)
    let temp_rx = {
        // Try to get broadcast receiver from WebDataProvider
        // For now, we'll create a dummy channel
        let (_tx, rx) = broadcast::channel(16);
        rx
    };
    
    // Split the receiver for async use
    let mut temp_rx = temp_rx;
    
    // Handle incoming messages and temperature updates
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_text_message(&text, &state).await {
                            warn!("Error handling message: {}", e);
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        info!("Received binary message: {} bytes", data.len());
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket client disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            
            // Handle temperature updates
            temp_msg = temp_rx.recv() => {
                match temp_msg {
                    Ok(WebSocketMessage::Temperature { hotend_current, hotend_target, bed_current, bed_target }) => {
                        let msg = serde_json::json!({
                            "type": "temperature",
                            "data": {
                                "hotend_current": hotend_current,
                                "hotend_target": hotend_target,
                                "bed_current": bed_current,
                                "bed_target": bed_target
                            }
                        });
                        
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                warn!("Failed to send temperature update");
                                break;
                            }
                        }
                    }
                    Ok(WebSocketMessage::Position { x, y, z, e }) => {
                        let msg = serde_json::json!({
                            "type": "position",
                            "data": { "x": x, "y": y, "z": z, "e": e }
                        });
                        
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                warn!("Failed to send position update");
                                break;
                            }
                        }
                    }
                    Ok(WebSocketMessage::State { from, to }) => {
                        let msg = serde_json::json!({
                            "type": "state",
                            "data": { "from": from, "to": to }
                        });
                        
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                warn!("Failed to send state update");
                                break;
                            }
                        }
                    }
                    Ok(other) => {
                        // Handle other message types
                        if let Ok(json) = serde_json::to_string(&other) {
                            if sender.send(Message::Text(json)).await.is_err() {
                                warn!("Failed to send update");
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        warn!("WebSocket client lagged behind, continuing");
                    }
                }
            }
        }
    }
}

/// Handle text message from WebSocket
async fn handle_text_message(
    text: &str,
    _state: &Arc<WebServerState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg: serde_json::Value = serde_json::from_str(text)?;
    
    if let Some(msg_type) = msg.get("type").and_then(|t| t.as_str()) {
        match msg_type {
            "ping" => {
                info!("Received ping, sending pong");
            }
            "command" => {
                if let Some(action) = msg.get("action").and_then(|a| a.as_str()) {
                    info!("Received command: {}", action);
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
