use crate::logic::openai::OpenAIClient;
use crate::{log_info, log_warn};
use serde_json::{Map, Value};
use std::path::Path;
use tokio_util::sync::CancellationToken;

pub async fn execute_translation_batches(
    map: &Map<String, Value>,
    client: &OpenAIClient,
    context_id: &str,
    batch_size: usize,
    token: &CancellationToken,
) -> Map<String, Value> {
    let safe_batch_size = if batch_size == 0 { 20 } else { batch_size };

    let pending_items: Vec<(&String, &String)> = map
        .iter()
        .filter_map(|(k, v)| {
            if let Value::String(s) = v {
                if !s.trim().is_empty() {
                    return Some((k, s));
                }
            }
            None
        })
        .collect();

    let total_items = pending_items.len();
    let mut final_map = map.clone();

    if total_items == 0 {
        return final_map;
    }

    // 分批处理
    for (batch_idx, chunk) in pending_items.chunks(safe_batch_size).enumerate() {
        if token.is_cancelled() {
            break;
        }

        log_info!(
            "[{}] 批次 {}/{} ({} 条目)",
            context_id,
            batch_idx + 1,
            (total_items + safe_batch_size - 1) / safe_batch_size,
            chunk.len()
        );

        let source_texts: Vec<String> = chunk.iter().map(|(_, v)| v.to_string()).collect();

        let translated_texts = match client
            .translate_text_list(source_texts, context_id, token)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                log_warn!("批次翻译失败: {}", e);
                continue;
            }
        };

        if translated_texts.len() == chunk.len() {
            for (i, (original_key, _)) in chunk.iter().enumerate() {
                final_map.insert(
                    (*original_key).clone(),
                    Value::String(translated_texts[i].clone()),
                );
            }
        } else {
            log_warn!("警告: AI 返回的数组长度不匹配，跳过此批次回填");
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
