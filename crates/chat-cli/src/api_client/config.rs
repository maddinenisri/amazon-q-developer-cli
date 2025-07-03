use std::collections::HashMap;
use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

use crate::api_client::custom_model::CustomModelConfig;

/// Configuration for the Amazon Q CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QCliConfig {
    /// Custom model configurations
    pub custom_models: Option<HashMap<String, CustomModelConfig>>,
    /// Default model to use
    pub default_model: Option<String>,
    /// Whether to use custom models by default
    pub prefer_custom_models: Option<bool>,
}

impl Default for QCliConfig {
    fn default() -> Self {
        Self {
            custom_models: None,
            default_model: None,
            prefer_custom_models: Some(false),
        }
    }
}

impl QCliConfig {
    /// Load configuration from the default location
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::default_config_path()?;

        if !config_path.exists() {
            // Create default config if it doesn't exist
            let default_config = Self::default();
            default_config.save()?;
            return Ok(default_config);
        }

        let config_content = std::fs::read_to_string(&config_path)?;
        let config: Self = serde_json::from_str(&config_content)?;
        Ok(config)
    }

    /// Save configuration to the default location
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::default_config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config_content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, config_content)?;
        Ok(())
    }

    /// Get the default configuration file path
    pub fn default_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;

        Ok(home_dir.join(".config").join("amazon-q").join("config.json"))
    }

    /// Get a custom model configuration by name
    pub fn get_custom_model(&self, name: &str) -> Option<&CustomModelConfig> {
        self.custom_models.as_ref()?.get(name)
    }

    /// Add or update a custom model configuration
    #[allow(dead_code)]
    pub fn set_custom_model(&mut self, name: String, config: CustomModelConfig) {
        if self.custom_models.is_none() {
            self.custom_models = Some(HashMap::new());
        }

        if let Some(models) = &mut self.custom_models {
            models.insert(name, config);
        }
    }

    /// Remove a custom model configuration
    #[allow(dead_code)]
    pub fn remove_custom_model(&mut self, name: &str) -> Option<CustomModelConfig> {
        self.custom_models.as_mut()?.remove(name)
    }

    /// List all available custom model names
    #[allow(dead_code)]
    pub fn list_custom_models(&self) -> Vec<String> {
        self.custom_models
            .as_ref()
            .map(|models| models.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QCliConfig::default();
        assert!(config.custom_models.is_none());
        assert!(config.default_model.is_none());
        assert_eq!(config.prefer_custom_models, Some(false));
    }

    #[test]
    fn test_custom_model_operations() {
        let mut config = QCliConfig::default();

        let model_config = CustomModelConfig {
            base_url: "https://api.example.com".to_string(),
            api_key: Some("test-key".to_string()),
            model_id: "claude-3-sonnet".to_string(),
            timeout_seconds: Some(60),
            headers: None,
        };

        // Test adding a custom model
        config.set_custom_model("test-model".to_string(), model_config.clone());
        assert!(config.get_custom_model("test-model").is_some());
        assert_eq!(config.list_custom_models(), vec!["test-model"]);

        // Test removing a custom model
        let removed = config.remove_custom_model("test-model");
        assert!(removed.is_some());
        assert!(config.get_custom_model("test-model").is_none());
        assert!(config.list_custom_models().is_empty());
    }
}
