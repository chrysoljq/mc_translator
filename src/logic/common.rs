use crate::logic::openai::OpenAIClient;
use crate::{log_info, log_warn};
use anyhow::Result;
use serde_json::{Map, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use tokio_util::sync::CancellationToken;
use tokio::task::JoinSet;
use tokio::sync::Semaphore;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TranslationContext {
    pub batch_size: usize,
    pub skip_existing: bool,
    pub update_existing: bool,
}

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

    // 限制单个文件内的并发请求数（例如同时处理 10 个批次）
    let batch_semaphore = Arc::new(Semaphore::new(5));
    let mut tasks = JoinSet::new();

    // 分批并创建异步任务
    for (batch_idx, chunk) in pending_items.chunks(safe_batch_size).enumerate() {
        if token.is_cancelled() {
            break;
        }

        let source_texts: Vec<String> = chunk.iter().map(|(_, v)| v.to_string()).collect();
        let original_keys: Vec<String> = chunk.iter().map(|(k, _)| (*k).clone()).collect();
        
        let client = client.clone();
        let context_id = context_id.to_string();
        let token = token.clone();
        let permit = batch_semaphore.clone().acquire_owned().await.unwrap();
        
        let chunk_len = chunk.len();
        let total_batches = (total_items + safe_batch_size - 1) / safe_batch_size;

        log_info!(
            "[{}] 准备批次 {}/{} ({} 条目)",
            context_id,
            batch_idx + 1,
            total_batches,
            chunk_len
        );

        tasks.spawn(async move {
            let _permit = permit; // 任务结束时自动释放信号量
            
            // 执行翻译请求
            let result = match client.translate_text_list(source_texts, &context_id, &token).await {
                Ok(translated_texts) => {
                    if translated_texts.len() == chunk_len {
                        Some(translated_texts)
                    } else {
                        log_warn!("警告: [{}] 批次 {} 返回数量不匹配，跳过翻译", context_id, batch_idx + 1);
                        None
                    }
                }
                Err(e) => {
                    log_warn!("批次翻译失败，跳过翻译。原因: {}", e);
                    None
                }
            };
            (original_keys, result)
        });
    }

    // 收集所有任务结果并回填到 Map 中
    while let Some(res) = tasks.join_next().await {
        if let Ok((keys, maybe_texts)) = res {
            match maybe_texts {
                Some(texts) => {
                    for (key, text) in keys.iter().zip(texts.iter()) {
                        final_map.insert(key.clone(), Value::String(text.clone()));
                    }
                }
                None => {
                    for key in keys {
                        final_map.remove(&key);
                    }
                }
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
    
    if let Some(idx) = parts.iter().position(|x| x == "lang") {
        if idx > 0 {
            return parts[idx - 1].to_string();
        }
    }
    // support special path like modpack_dir/resources/dsurround/dsurround/data/chat/en_us.lang
    else if let Some(idx) = parts.iter().position(|x| x == "data") {
        if idx > 0 {
            return parts[idx - 1].to_string();
        }
    }
    log_warn!("发现无法解析的模组：{:?}", path);

    // path.file_stem()
    //     .unwrap_or_default()
    //     .to_string_lossy()
    //     .to_string()
    "unknown_mod".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileFormat {
    Json,
    Lang,
}

pub fn get_target_filename(original_name: &str) -> String {
    if original_name.contains("en_us") {
        original_name.replace("en_us", "zh_cn")
    } else if original_name.contains("en_") {
        original_name
            .replace("en_", "zh_")
            .replace("US", "CN")
            .replace("us", "cn")
    } else {
        format!("zh_cn_{}", original_name)
    }
}

pub fn read_map_from_file(
    path: &Path,
    format: FileFormat,
) -> Result<Map<String, serde_json::Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    match format {
        FileFormat::Json => {
            let content = fs::read_to_string(path)?;
            let json: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::Value::Object(Map::new()));
            Ok(json.as_object().cloned().unwrap_or_default())
        }
        FileFormat::Lang => {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            let mut map = Map::new();
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() || line.trim().starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(
                        k.trim().to_string(),
                        serde_json::Value::String(v.trim().to_string()),
                    );
                }
            }
            Ok(map)
        }
    }
}

pub fn write_map_to_file(
    path: &Path,
    map: &Map<String, serde_json::Value>,
    format: FileFormat,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;

    match format {
        FileFormat::Json => {
            serde_json::to_writer_pretty(file, map)?;
        }
        FileFormat::Lang => {
            for (k, v) in map {
                if let Some(str_val) = v.as_str() {
                    let escaped_val = str_val.replace('\n', "\\n").replace('\r', ""); // 处理换行符
                    writeln!(file, "{}={}", k, escaped_val)?;
                }
            }
        }
    }
    Ok(())
}

pub async fn core_translation_pipeline(
    src_map: serde_json::Map<String, serde_json::Value>,
    mod_id: &str,
    original_filename: &str,
    output_root: &Path,
    client: &OpenAIClient,
    batch_size: usize,
    skip_existing: bool,
    update_existing: bool,
    format: FileFormat,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    // 构造标准输出路径: output/assets/{modid}/lang/{zh_cn.x}
    let target_name = get_target_filename(original_filename);
    let final_path = output_root
        .join("assets")
        .join(mod_id)
        .join("lang")
        .join(&target_name);

    if !update_existing && skip_existing && final_path.exists() {
        log_info!("跳过已存在的文件: {:?}", final_path);
        return Ok(());
    }

    let (map_to_translate, mut base_map) = if update_existing && final_path.exists() {
        // [更新模式]
        let existing_map = read_map_from_file(&final_path, format).unwrap_or_default();

        let mut pending = serde_json::Map::new();
        for (k, v) in &src_map {
            if !existing_map.contains_key(k) {
                pending.insert(k.clone(), v.clone());
            }
        }

        if pending.is_empty() {
            log_info!("无新增条目，无需更新: {:?}", final_path);
            return Ok(());
        }

        log_info!(
            "增量更新检测到 {} 个新条目 (ModID: {})",
            pending.len(),
            mod_id
        );

        // [保存增量原始内容]
        let raw_dir = output_root.join("raw_content");
        if !raw_dir.exists() {
            fs::create_dir_all(&raw_dir)?;
        }
        // 文件名增加 hash 或 timestamp 防止覆盖？这里暂时用 modid+filename
        let raw_path = raw_dir.join(format!("{}_{}", mod_id, original_filename));
        let raw_file = fs::File::create(&raw_path)?;
        serde_json::to_writer_pretty(raw_file, &pending)?;
        log_info!("已备份增量原始内容: {:?}", raw_path);

        (pending, existing_map)
    } else {
        // [全量模式]
        (src_map, serde_json::Map::new())
    };

    let translated_part =
        execute_translation_batches(&map_to_translate, client, mod_id, batch_size, token).await;

    if token.is_cancelled() {
        log_warn!("任务取消，放弃保存: {:?}", final_path);
        return Ok(());
    }

    for (k, v) in translated_part {
        base_map.insert(k, v);
    }

    write_map_to_file(&final_path, &base_map, format)?;

    let action_str = if update_existing && final_path.exists() {
        "更新"
    } else {
        "生成"
    };
    log_info!("{}完成 (ModID: {}): {:?}", action_str, mod_id, final_path);

    Ok(())
}
