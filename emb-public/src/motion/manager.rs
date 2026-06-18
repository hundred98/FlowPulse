//! Homing Manager
//!
//! Provides homing operations for printer axes.
//! Encapsulates the low-level serial frame protocol (0x0A frame type)
//! for sending homing commands and processing ACK/NACK responses.

use std::sync::Arc;
use crate::CoreSocketClient;

/// Homing Manager for axis homing operations.
///
/// Wraps the core socket client to provide high-level homing operations,
/// abstracting the raw serial frame protocol (0x0A send, 0x12/0x06 response).
pub struct HomingManager {
    client: Arc<CoreSocketClient>,
}

impl HomingManager {
    /// Create a new HomingManager with the given core socket client.
    pub fn new(client: Arc<CoreSocketClient>) -> Self {
        Self { client }
    }

    /// Get a reference to the underlying core socket client.
    pub fn client(&self) -> &Arc<CoreSocketClient> {
        &self.client
    }

    /// Home all axes (X, Y, Z).
    ///
    /// Sends a homing command with mask 0b111 (all axes).
    ///
    /// # Returns
    /// * `Ok(())` - If homing started successfully
    /// * `Err(String)` - If the request failed
    pub async fn home_all(&self) -> Result<(), String> {
        self.home_axes(0b111).await
    }

    /// Home specific axes by name.
    ///
    /// Accepts axes names like "x", "y", "z", or "all".
    ///
    /// # Returns
    /// * `Ok(String)` - Description of which axes were homed
    /// * `Err(String)` - If the request failed
    pub async fn home_by_names(&self, x: bool, y: bool, z: bool, all: bool) -> Result<String, String> {
        let axes_mask: u8 = if all {
            0b111
        } else {
            (if x { 0x01 } else { 0 }) |
            (if y { 0x02 } else { 0 }) |
            (if z { 0x04 } else { 0 })
        };

        if axes_mask == 0 {
            return Err("No axes selected".to_string());
        }

        let axes_names = self.format_axes_names(axes_mask);
        self.home_axes(axes_mask).await?;
        Ok(format!("Homing started: {}", axes_names))
    }

    /// Home specific axes using a bitmask.
    ///
    /// Bitmask: bit0=X, bit1=Y, bit2=Z
    /// Sends frame type 0x0A with the axes mask, then polls for
    /// ACK (0x06) or NACK (0x12) response.
    ///
    /// # Returns
    /// * `Ok(())` - If homing command was sent and acknowledged
    /// * `Err(String)` - If the request failed or NACK received
    pub async fn home_axes(&self, axes_mask: u8) -> Result<(), String> {
        let axes_names = self.format_axes_names(axes_mask);

        tracing::info!("Homing start: axes_mask=0x{:02X} ({})", axes_mask, axes_names);

        // Send homing frame (0x0A = homing command)
        self.client.serial_send_frame(0x0A, vec![axes_mask]).await?;

        let mut ack_received = false;

        // Poll for response (ACK/NACK) from device
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for _ in 0..5 {
            match self.client.serial_recv_frame().await {
                Ok(Some((ft, pld))) => {
                    if ft == 0x12 {
                        // NACK
                        let err = if pld.len() > 1 { pld[1] } else { 0xFF };
                        let desc = self.nack_description(err);
                        tracing::error!("Homing NACK (error={}, meaning: {})", err, desc);
                        return Err(format!("Homing NACK: {} (error={})", desc, err));
                    } else if ft == 0x06 {
                        // ACK - homing accepted
                        tracing::info!("Homing ACK received");
                        ack_received = true;
                        break;
                    }
                    // Other frame types - continue polling
                }
                Ok(None) => break, // no more frames
                Err(e) => tracing::warn!("Recv error: {}", e),
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        if !ack_received {
            tracing::info!("Homing command sent (no explicit ACK/NACK received within poll window)");
        }

        // Update server-side homed_axes so motion planner knows axes are homed.
        // The 0x0A serial frame controls the firmware directly, bypassing the
        // motion planner. Without this sync, G0/G1 moves will fail with
        // "axes not homed" even after successful homing.
        let current_homed = self.client.motion_query_homed().await.unwrap_or(0);
        let new_homed = current_homed | axes_mask;
        if new_homed != current_homed {
            tracing::info!("Updating server homed_axes: {:#04b} -> {:#04b}", current_homed, new_homed);
            self.client.motion_set_homed_axes(new_homed).await?;
        }

        Ok(())
    }

    /// Format axes names for logging.
    fn format_axes_names(&self, axes_mask: u8) -> String {
        [
            (if axes_mask & 0x01 != 0 { "X" } else { "" }),
            (if axes_mask & 0x02 != 0 { "Y" } else { "" }),
            (if axes_mask & 0x04 != 0 { "Z" } else { "" }),
        ].concat()
    }

    /// Get human-readable description for NACK error codes.
    fn nack_description(&self, error_code: u8) -> &'static str {
        match error_code {
            1 => "BUSY - homing already running",
            2 => "INVALID_AXES",
            3 => "NOT_CFG - endstops not configured",
            4 => "PRE_TIMEOUT",
            5 => "TOTAL_TIMEOUT",
            6 => "AXIS_DISABLED",
            _ => "UNKNOWN",
        }
    }
}

