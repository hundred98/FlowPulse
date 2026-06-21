//! Power-loss resume checkpoint manager.
//!
//! Manages `config/resume.json`:
//! - `config` section: user-editable settings (enabled, interval, z_hop, retract)
//! - `checkpoint` section: runtime save/restore data (file, line, position, temps, modes)
//!
//! All I/O is synchronous file operations; call `save()` via `tokio::task::spawn_blocking`
//! or from an async context where brief blocking is acceptable.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::common::EmbResult;

/// ── config section (user-editable) ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeConfig {
    /// Master switch
    pub enabled: bool,
    /// Save checkpoint every N valid commands
    pub checkpoint_interval: u32,
    /// Z-hop height (mm) during resume move
    pub z_hop: f32,
    /// Retract distance (mm) before resume, compensating for overlapped lines
    pub retract_distance: f32,
}

impl Default for ResumeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkpoint_interval: 10,
            z_hop: 5.0,
            retract_distance: 1.0,
        }
    }
}

/// ── checkpoint section (auto-saved at runtime) ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    /// Whether a valid checkpoint exists
    pub has: bool,
    /// G-code file path
    pub file_path: String,
    /// File line number (1-based) last processed
    pub line: u32,
    /// Z axis height (mm) at checkpoint
    pub z_pos: f32,
    /// Hotend target temperature (°C)
    pub hotend_temp: f32,
    /// Bed target temperature (°C)
    pub bed_temp: f32,
    /// M220 S value (feed rate, 0-100)
    pub feed_rate: u16,
    /// M221 S value (flow rate, 0-100)
    pub flow_rate: u16,
    /// G90 (true) / G91 (false)
    pub is_absolute: bool,
    /// M82 (false) / M83 (true)
    pub e_is_relative: bool,
    /// M106 S value (fan speed, 0-255)
    pub fan_speed: u8,
}

impl Default for CheckpointData {
    fn default() -> Self {
        Self {
            has: false,
            file_path: String::new(),
            line: 0,
            z_pos: 0.0,
            hotend_temp: 0.0,
            bed_temp: 0.0,
            feed_rate: 100,
            flow_rate: 100,
            is_absolute: true,
            e_is_relative: true,
            fan_speed: 0,
        }
    }
}

/// Combined resume file (top-level JSON structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResumeFile {
    #[serde(default)]
    #[allow(dead_code)]
    version: String,
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<String>,
    config: ResumeConfig,
    checkpoint: CheckpointData,
}

// ──────────────────────────────────────────────
// CheckpointManager
// ──────────────────────────────────────────────

/// Manages power-loss resume checkpoint I/O.
///
/// # Usage
/// ```ignore
/// let mgr = CheckpointManager::new("config/resume.json");
/// let cfg = mgr.config();
/// if mgr.has_checkpoint() { /* ask user */ }
/// mgr.save(ctx).unwrap();
/// mgr.clear().unwrap();
/// ```
pub struct CheckpointManager {
    path: String,
    config: ResumeConfig,
    checkpoint: CheckpointData,
}

impl CheckpointManager {
    /// Create a new manager, loading existing data from disk if present.
    pub fn new(path: &str) -> Self {
        let (config, checkpoint) = if Path::new(path).exists() {
            match Self::load_from_disk(path) {
                Ok(data) => (data.config, data.checkpoint),
                Err(e) => {
                    tracing::warn!("Failed to load {}: {}, using defaults", path, e);
                    (ResumeConfig::default(), CheckpointData::default())
                }
            }
        } else {
            // Create file with defaults and empty checkpoint
            let default = ResumeFile {
                version: String::new(),
                description: None,
                config: ResumeConfig::default(),
                checkpoint: CheckpointData::default(),
            };
            if let Err(e) = write_json(path, &default) {
                tracing::warn!("Failed to create {}: {}", path, e);
            }
            (ResumeConfig::default(), CheckpointData::default())
        };

        Self { path: path.to_string(), config, checkpoint }
    }

    /// Return the current config (read-only snapshot).
    pub fn config(&self) -> ResumeConfig {
        self.config.clone()
    }

    /// Update the config section on disk (preserves checkpoint).
    pub fn set_config(&mut self, config: ResumeConfig) -> EmbResult<()> {
        self.config = config;
        self.flush()
    }

    /// Whether a valid checkpoint exists.
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint.has
    }

    /// Return a snapshot of current checkpoint data.
    pub fn checkpoint_data(&self) -> CheckpointData {
        self.checkpoint.clone()
    }

    /// Save a new checkpoint (overwrites previous).
    pub fn save(&mut self, ctx: CheckpointContext) -> EmbResult<()> {
        self.checkpoint = CheckpointData {
            has: true,
            file_path: ctx.file_path,
            line: ctx.line,
            z_pos: ctx.z,
            hotend_temp: ctx.hotend_temp,
            bed_temp: ctx.bed_temp,
            feed_rate: ctx.feed_rate,
            flow_rate: ctx.flow_rate,
            is_absolute: ctx.is_absolute,
            e_is_relative: ctx.e_is_relative,
            fan_speed: ctx.fan_speed,
        };
        self.flush()
    }

    /// Clear the checkpoint (marks has=false, keeps config).
    pub fn clear(&mut self) -> EmbResult<()> {
        self.checkpoint = CheckpointData::default();
        self.flush()
    }

    // ── internal helpers ──

    fn load_from_disk(path: &str) -> EmbResult<ResumeFile> {
        let content = std::fs::read_to_string(path)?;
        let file: ResumeFile = serde_json::from_str(&content)?;
        Ok(file)
    }

    fn flush(&self) -> EmbResult<()> {
        let file = ResumeFile {
            version: String::new(),
            description: None,
            config: self.config.clone(),
            checkpoint: self.checkpoint.clone(),
        };
        write_json(&self.path, &file)
    }
}

fn write_json<T: Serialize>(path: &str, value: &T) -> EmbResult<()> {
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, content)?;
    Ok(())
}

// ──────────────────────────────────────────────
// CheckpointContext — builder for saving
// ──────────────────────────────────────────────

/// Runtime state snapshot passed to `CheckpointManager::save()`.
pub struct CheckpointContext {
    pub file_path: String,
    pub line: u32,
    pub z: f32,
    pub hotend_temp: f32,
    pub bed_temp: f32,
    pub feed_rate: u16,
    pub flow_rate: u16,
    pub is_absolute: bool,
    pub e_is_relative: bool,
    pub fan_speed: u8,
}

// ──────────────────────────────────────────────
// Recovery helpers
// ──────────────────────────────────────────────

/// Generate the recovery G-code command list from a checkpoint.
pub fn build_resume_commands(cp: &CheckpointData, config: &ResumeConfig) -> Vec<String> {
    let mut cmds = Vec::with_capacity(16);

    // 1. Positioning mode
    if cp.is_absolute {
        cmds.push("G90".to_string());
    } else {
        cmds.push("G91".to_string());
    }

    // 2. E relative mode
    if cp.e_is_relative {
        cmds.push("M83".to_string());
    } else {
        cmds.push("M82".to_string());
    }

    // 3. Feed rate & flow rate
    cmds.push(format!("M220 S{}", cp.feed_rate));
    cmds.push(format!("M221 S{}", cp.flow_rate));

    // 4. Fan speed
    cmds.push(format!("M106 S{}", cp.fan_speed));

    // 5. Tell MCU the current Z height (no physical movement)
    cmds.push(format!("G92 Z{:.4}", cp.z_pos));

    // 6. Lift Z to avoid hitting the print during XY home
    let hop_z = cp.z_pos + config.z_hop;
    cmds.push(format!("G1 Z{:.4} F300", hop_z));

    // 7. Home XY axes (MCU loses position on power loss)
    cmds.push("G28 X Y".to_string());

    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_resume_commands() {
        let cp = CheckpointData {
            has: true,
            file_path: "test.gcode".into(),
            line: 100,
            z_pos: 5.0,
            hotend_temp: 210.0,
            bed_temp: 60.0,
            feed_rate: 100,
            flow_rate: 100,
            is_absolute: true,
            e_is_relative: true,
            fan_speed: 255,
        };
        let cfg = ResumeConfig::default();
        let cmds = build_resume_commands(&cp, &cfg);
        assert!(cmds.contains(&"G90".to_string()));
        assert!(cmds.contains(&"M83".to_string()));
        assert!(cmds.contains(&"M220 S100".to_string()));
        assert!(cmds.contains(&"G28 X Y".to_string()));
        assert!(cmds.iter().any(|c| c.contains("G92 Z5.0000")));
        // Z-hop: 5.0 + 5.0 = 10.0
        assert!(cmds.iter().any(|c| c.contains("Z10.0000")));
    }
}