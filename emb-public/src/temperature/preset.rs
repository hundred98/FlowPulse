//! Temperature preset manager
//!
//! This module manages temperature presets for different materials (PLA, ABS, PETG, etc.).

use super::types::TemperaturePreset;
use crate::common::EmbResult;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Preset manager
pub struct PresetManager {
    /// Temperature presets
    presets: Arc<RwLock<Vec<TemperaturePreset>>>,
}

impl PresetManager {
    /// Create a new preset manager
    pub fn new() -> Self {
        Self {
            presets: Arc::new(RwLock::new(Self::default_presets())),
        }
    }

    /// Create a preset manager with custom presets
    pub fn with_presets(presets: Vec<TemperaturePreset>) -> Self {
        Self {
            presets: Arc::new(RwLock::new(presets)),
        }
    }

    /// Get default presets
    fn default_presets() -> Vec<TemperaturePreset> {
        vec![
            TemperaturePreset::new("PLA".to_string(), 200.0, 60.0)
                .with_fan(100),
            TemperaturePreset::new("ABS".to_string(), 240.0, 100.0)
                .with_chamber(50.0)
                .with_fan(0),
            TemperaturePreset::new("PETG".to_string(), 230.0, 80.0)
                .with_fan(50),
            TemperaturePreset::new("TPU".to_string(), 220.0, 50.0)
                .with_fan(30),
            TemperaturePreset::new("Nylon".to_string(), 250.0, 80.0)
                .with_chamber(60.0)
                .with_fan(20),
        ]
    }

    /// Get all presets
    pub async fn get_all(&self) -> Vec<TemperaturePreset> {
        self.presets.read().await.clone()
    }

    /// Get a preset by name
    pub async fn get(&self, name: &str) -> Option<TemperaturePreset> {
        let presets = self.presets.read().await;
        presets.iter().find(|p| p.name == name).cloned()
    }

    /// Add a new preset
    pub async fn add(&self, preset: TemperaturePreset) -> EmbResult<()> {
        let mut presets = self.presets.write().await;

        // Check if preset with same name already exists
        if presets.iter().any(|p| p.name == preset.name) {
            return Err(crate::common::EmbError::InvalidParam(format!(
                "Preset '{}' already exists",
                preset.name
            )));
        }

        presets.push(preset);
        Ok(())
    }

    /// Update an existing preset
    pub async fn update(&self, preset: TemperaturePreset) -> EmbResult<()> {
        let mut presets = self.presets.write().await;

        // Find and update the preset
        if let Some(existing) = presets.iter_mut().find(|p| p.name == preset.name) {
            *existing = preset;
            Ok(())
        } else {
            Err(crate::common::EmbError::InvalidParam(format!(
                "Preset '{}' not found",
                preset.name
            )))
        }
    }

    /// Remove a preset by name
    pub async fn remove(&self, name: &str) -> EmbResult<()> {
        let mut presets = self.presets.write().await;

        // Find and remove the preset
        let initial_len = presets.len();
        presets.retain(|p| p.name != name);

        if presets.len() == initial_len {
            Err(crate::common::EmbError::InvalidParam(format!(
                "Preset '{}' not found",
                name
            )))
        } else {
            Ok(())
        }
    }

    /// Check if a preset exists
    pub async fn exists(&self, name: &str) -> bool {
        let presets = self.presets.read().await;
        presets.iter().any(|p| p.name == name)
    }

    /// Get preset names
    pub async fn get_names(&self) -> Vec<String> {
        let presets = self.presets.read().await;
        presets.iter().map(|p| p.name.clone()).collect()
    }

    /// Load presets from configuration
    pub async fn load_from_config(&self, presets: Vec<TemperaturePreset>) {
        let mut current = self.presets.write().await;
        *current = presets;
    }

    /// Export presets to configuration format
    pub async fn export_to_config(&self) -> Vec<TemperaturePreset> {
        self.presets.read().await.clone()
    }

    /// Reset to default presets
    pub async fn reset_to_defaults(&self) {
        let mut presets = self.presets.write().await;
        *presets = Self::default_presets();
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_all_presets() {
        let manager = PresetManager::new();
        let presets = manager.get_all().await;

        assert!(!presets.is_empty());
        assert!(presets.iter().any(|p| p.name == "PLA"));
        assert!(presets.iter().any(|p| p.name == "ABS"));
    }

    #[tokio::test]
    async fn test_get_preset() {
        let manager = PresetManager::new();

        let preset = manager.get("PLA").await;
        assert!(preset.is_some());
        let preset = preset.unwrap();
        assert_eq!(preset.hotend_temp, 200.0);
        assert_eq!(preset.bed_temp, 60.0);
        assert_eq!(preset.fan_speed, 100);

        let not_found = manager.get("Unknown").await;
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_add_preset() {
        let manager = PresetManager::new();

        let custom = TemperaturePreset::new("Custom".to_string(), 210.0, 65.0);
        let result = manager.add(custom).await;
        assert!(result.is_ok());

        // Try to add duplicate
        let duplicate = TemperaturePreset::new("Custom".to_string(), 220.0, 70.0);
        let result = manager.add(duplicate).await;
        assert!(result.is_err());

        // Verify preset was added
        let preset = manager.get("Custom").await;
        assert!(preset.is_some());
        let preset = preset.unwrap();
        assert_eq!(preset.hotend_temp, 210.0);
    }

    #[tokio::test]
    async fn test_update_preset() {
        let manager = PresetManager::new();

        let updated = TemperaturePreset::new("PLA".to_string(), 205.0, 65.0);
        let result = manager.update(updated).await;
        assert!(result.is_ok());

        let preset = manager.get("PLA").await;
        assert!(preset.is_some());
        let preset = preset.unwrap();
        assert_eq!(preset.hotend_temp, 205.0);
        assert_eq!(preset.bed_temp, 65.0);

        // Try to update non-existent preset
        let not_found = TemperaturePreset::new("Unknown".to_string(), 200.0, 60.0);
        let result = manager.update(not_found).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_preset() {
        let manager = PresetManager::new();

        let result = manager.remove("PLA").await;
        assert!(result.is_ok());

        let preset = manager.get("PLA").await;
        assert!(preset.is_none());

        // Try to remove non-existent preset
        let result = manager.remove("Unknown").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exists_preset() {
        let manager = PresetManager::new();

        assert!(manager.exists("PLA").await);
        assert!(!manager.exists("Unknown").await);
    }

    #[tokio::test]
    async fn test_get_names() {
        let manager = PresetManager::new();
        let names = manager.get_names().await;

        assert!(names.contains(&"PLA".to_string()));
        assert!(names.contains(&"ABS".to_string()));
    }

    #[tokio::test]
    async fn test_reset_to_defaults() {
        let manager = PresetManager::new();

        // Remove a preset
        manager.remove("PLA").await.unwrap();
        assert!(!manager.exists("PLA").await);

        // Reset to defaults
        manager.reset_to_defaults().await;
        assert!(manager.exists("PLA").await);
    }
}
