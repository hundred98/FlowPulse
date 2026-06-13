//! GPIO Manager
//!
//! Provides high-level GPIO operations including pin control, querying,
//! and standardized event subscription via broadcast channel.

use std::sync::Arc;
use serde::Serialize;
use tokio::sync::broadcast;
use crate::CoreSocketClient;

/// GPIO event published via the broadcast channel.
///
/// Emitted whenever a GPIO report is received from the device.
#[derive(Debug, Clone, Serialize)]
pub struct GpioEvent {
    /// GPIO pin name (e.g., "fan0", "probe")
    pub name: String,
    /// GPIO pin value (e.g., 1.0 for HIGH, 0.0 for LOW)
    pub value: f32,
}

/// GPIO Manager for pin control and event monitoring.
///
/// Wraps the core socket client to provide:
/// - Setting and querying GPIO pin values
/// - Subscribing to GPIO report events
/// - Standardized event broadcast channel
pub struct GpioManager {
    client: Arc<CoreSocketClient>,
    /// Broadcast sender for GPIO report events
    event_tx: broadcast::Sender<GpioEvent>,
}

impl GpioManager {
    /// Create a new GpioManager with the given core socket client.
    ///
    /// Initializes an internal broadcast channel for GPIO events.
    /// Call `subscribe()` to enable report forwarding from the device.
    pub fn new(client: Arc<CoreSocketClient>) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { client, event_tx }
    }

    /// Get a reference to the underlying core socket client.
    pub fn client(&self) -> &Arc<CoreSocketClient> {
        &self.client
    }

    /// Subscribe to GPIO report events.
    ///
    /// Sets up an internal callback that forwards GPIO reports from the
    /// core server to the broadcast channel, then sends the subscribe
    /// request to enable report pushing.
    ///
    /// # Returns
    /// * `Ok(())` if subscription succeeded
    /// * `Err(String)` if the request failed
    pub async fn subscribe(&self) -> Result<(), String> {
        self.setup_callback().await;

        // Send subscribe request to server
        self.client.gpio_subscribe_report(true).await?;

        log::info!("GPIO report subscribed (broadcast channel active)");
        Ok(())
    }

    /// Set up the internal callback without sending the subscribe request.
    ///
    /// Useful when the subscribe request is handled externally
    /// (e.g., by `setup::initialize_device`), but the broadcast channel
    /// and callback still need to be initialized.
    pub async fn setup_callback(&self) {
        let tx = self.event_tx.clone();
        self.client.set_gpio_report_callback(move |name, value| {
            let _ = tx.send(GpioEvent { name, value });
        }).await;
    }

    /// Get a receiver for GPIO report events.
    ///
    /// Each call creates a new receiver subscribed to the same broadcast channel.
    pub fn event_receiver(&self) -> broadcast::Receiver<GpioEvent> {
        self.event_tx.subscribe()
    }

    /// Set a GPIO pin value.
    ///
    /// # Arguments
    /// * `name` - GPIO pin name
    /// * `value` - Pin value (e.g., 1.0 for HIGH, 0.0 for LOW)
    ///
    /// # Returns
    /// * `Ok(())` if the pin was set successfully
    /// * `Err(String)` if the request failed
    pub async fn set_pin(&self, name: &str, value: f32) -> Result<(), String> {
        self.client.gpio_set(name, value).await
    }

    /// Query a GPIO pin value.
    ///
    /// # Arguments
    /// * `name` - GPIO pin name
    ///
    /// # Returns
    /// * `Ok(f32)` - The current pin value
    /// * `Err(String)` if the query failed
    pub async fn query_pin(&self, name: &str) -> Result<f32, String> {
        self.client.gpio_query(name).await
    }

    /// Unsubscribe from GPIO report events.
    ///
    /// # Returns
    /// * `Ok(())` if unsubscription succeeded
    /// * `Err(String)` if the request failed
    pub async fn unsubscribe(&self) -> Result<(), String> {
        // Clear the callback
        self.client.clear_gpio_report_callback().await;

        // Send unsubscribe request
        self.client.gpio_subscribe_report(false).await?;

        log::info!("GPIO report unsubscribed");
        Ok(())
    }
}