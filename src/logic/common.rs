use crate::logic::openai::OpenAIClient;
use crate::{log_info, log_warn};
use tokio_util::sync::CancellationToken;
use std::path::Path;

pub async fn execute_translation_batches(
    map: &serde_json::Map<String, serde_json::Value>,
    client: &OpenAIClient,
    mod_id: &str,
    batch_size: usize,
    token: &CancellationToken,
) -> serde_json::Map<String, serde_json::Value> {
    let safe_batch_size = if batch_size == 0 { 1 } else { batch_size };
    let total_items = map.len();
    let keys: Vec<String> = map.keys().cloned().collect();
    let mut final_map = serde_json::Map::new();

    for (idx, chunk) in keys.chunks(safe_batch_size).enumerate() {
        if token.is_cancelled() {
            break;
        }
        log_info!(
            "正在翻译 [{}] 第 {}/{} 批 (共 {} 条)",
            mod_id,
            idx + 1,
            (total_items + safe_batch_size - 1) / safe_batch_size,
            total_items
        );

        let mut sub_map = serde_json::Map::new();
        for k in chunk {
            if let Some(v) = map.get(k) {
                sub_map.insert(k.clone(), v.clone());
            }
        }

        match client.translate_batch(sub_map.clone(), mod_id, token).await {
            Ok(translated) => final_map.extend(translated),
            Err(e) => {
                log_warn!("批次失败保留原文: {}", e);
                final_map.extend(sub_map); // 失败回退
            }
        }
    }
    final_map
}

pub fn extract_mod_id(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    if let Some(idx) = parts.iter().position(|x| x == "assets") {
        if idx + 1 < parts.len() {
            return parts[idx + 1].to_string();
        }
    }

    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

