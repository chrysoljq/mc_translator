use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub api_key: String,
    pub base_url: String,
    pub input_path: String,
    pub output_path: String,
    pub model: String,
    pub batch_size: usize,
    pub skip_existing: bool,
    pub timeout: usize,
    pub prompt: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            input_path: String::new(),
            output_path: "./output_cn".to_string(),
            model: "gpt-3.5-turbo".to_string(), 
            batch_size: 200,
            skip_existing: true,
            timeout: 240,
            prompt: "(WIP)You are a Minecraft Mod localization Expert".to_string(), // TODO
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        PathBuf::from("config.json")
    }

    pub fn load() -> Self {
        if let Ok(content) = fs::read_to_string(Self::config_path()) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::config_path(), data);
        }
    }
}