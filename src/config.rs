use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub api_key: String,
    pub base_url: String,
    pub input_path: String,
    pub output_path: String,
    pub check_path: String, // TODO: 设置更新检查路径
    pub model: String,
    pub source_lang: String,
    pub target_lang: String,
    pub batch_size: usize,
    pub skip_existing: bool,
    pub max_retries: u32,
    pub retry_delay: u64,
    pub file_semaphore: usize,
    pub max_network_concurrency: usize,
    pub prompt: String,
    pub skip_quest: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            input_path: String::new(),
            output_path: "./MC_Translator/output_cn".to_string(),
            check_path: "./MC_Translator/output_cn".to_string(),
            source_lang: "en_us".to_string(),
            target_lang: "zh_cn".to_string(),
            model: "gpt-3.5-turbo".to_string(), 
            batch_size: 200,
            skip_existing: true,
            max_retries: 5,
            retry_delay: 10,
            file_semaphore: 5,
            max_network_concurrency: 10, // Global limit for concurrent network requests
            prompt: "你是一个《我的世界》(Minecraft) 模组本地化专家。当前模组 ID: 【{MOD_ID}】。\n\
        我将发送一个包含英文原文的 JSON 字符串数组。\n\
        请将数组中的每一项翻译为简体中文，并返回一个 JSON 字符串数组。\n\
        要求：\n\
        1. **严格保持顺序**：输出数组的第 N 项必须对应输入数组的第 N 项。\n\
        2. **严格保持长度**：输出数组的元素数量必须与输入完全一致。\n\
        3. 请严格保留格式代码（如 §a, %s, {{0}}，\\n 等）。\n\
        4. 只返回纯净的 JSON 字符串，不要包含 Markdown 代码块标记。".to_string(),
            skip_quest: true,
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        PathBuf::from("./MC_Translator/config.json")
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