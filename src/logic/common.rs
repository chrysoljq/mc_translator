use crate::logic::openai::OpenAIClient;
use crate::{log_info, log_warn, log_err};
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
    pub network_semaphore: Arc<Semaphore>,
    pub source_lang: String,
    pub target_lang: String,
}

pub async fn execute_translation_batches(
    map: &Map<String, Value>,
    client: &OpenAIClient,
    context_id: &str,
    ctx: &TranslationContext,
    token: &CancellationToken,
) -> Map<String, Value> {
    let batch_size = ctx.batch_size;
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
        let permit = ctx.network_semaphore.clone().acquire_owned().await.unwrap();
        
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
                        log_err!("[{}] 批次 {} 返回数量不匹配，跳过翻译", context_id, batch_idx + 1);
                        None
                    }
                }
                Err(e) => {
                    log_err!("[{}] 批次翻译失败，跳过翻译。原因: {}", context_id, e);
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

pub fn get_target_filename(original_name: &str, source_lang: &str, target_lang: &str) -> String {
    // 简单的替换逻辑：如果不区分大小写地包含 source_lang，则替换为 target_lang
    // 同时也保留原有的 en_us -> zh_cn 的兜底逻辑，以防 source_lang 设置不精确

    let lower_name = original_name.to_lowercase();
    let lower_source = source_lang.to_lowercase();
    let lower_target = target_lang.to_lowercase();

    if lower_name.contains(&lower_source) {
        original_name.replace(source_lang, target_lang)
                     .replace(&lower_source, &lower_target)
    } else {
        format!("{}_{}", lower_target, original_name)
    }
}

pub fn sanitize_json_content(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    // 移除 BOM
    if let Some('\u{feff}') = chars.peek() {
        chars.next();
    }

    while let Some(c) = chars.next() {
        if !in_string {
            // 检查开头是否是 // 注释
            if c == '/' {
                if let Some(&next_c) = chars.peek() {
                    if next_c == '/' {
                        // 发现注释，跳过直到换行
                        chars.next(); // consume second '/'
                        while let Some(&comment_c) = chars.peek() {
                            if comment_c == '\n' || comment_c == '\r' {
                                break;
                            }
                            chars.next();
                        }
                        continue;
                    }
                }
            }
            // 检查 # 注释 (YAML/Properties 风格兼容)
            if c == '#' {
                 while let Some(&comment_c) = chars.peek() {
                    if comment_c == '\n' || comment_c == '\r' {
                        break;
                    }
                    chars.next();
                }
                continue;
            }

            if c == '\"' {
                in_string = true;
            }
            result.push(c);
        } else {
            // inside string
            if escape {
                escape = false;
                match c {
                    '\n' => result.push('n'), // 修正：反斜杠后接换行符，视为需要被转换为 \n
                    '\r' => {}, 
                    _ => result.push(c),
                }
            } else if c == '\\' {
                escape = true;
                result.push(c);
            } else if c == '\"' {
                in_string = false;
                result.push(c);
            } else {
                // 处理字符串内部的换行符和控制字符
                match c {
                    '\n' => result.push_str("\\n"),
                    '\r' => {}, // 忽略回车
                    '\t' => result.push_str("\\t"),
                    _ => {
                        if c.is_control() {
                            // 忽略其他控制字符，防止 JSON 解析报错
                        } else {
                            result.push(c);
                        }
                    },
                }
            }
        }
    }
    result
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
            let sanitized = sanitize_json_content(&content);
            let json: serde_json::Value =
                serde_json::from_str(&sanitized).unwrap_or(serde_json::Value::Object(Map::new()));
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
    ctx: Arc<TranslationContext>,
    format: FileFormat,
    builtin_map: Option<serde_json::Map<String, serde_json::Value>>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let skip_existing = ctx.skip_existing;
    let update_existing = ctx.update_existing;
    // 构造标准输出路径: output/assets/{modid}/lang/{zh_cn.x}
    let target_name = get_target_filename(original_filename, &ctx.source_lang, &ctx.target_lang);
    let final_path = output_root
        .join("assets")
        .join(mod_id)
        .join("lang")
        .join(&target_name);

    if !update_existing && skip_existing && final_path.exists() {
        log_info!("跳过已存在的文件: {:?}", final_path);
        return Ok(());
    }

    let (map_to_translate, mut base_map) = if update_existing {
        // [更新模式]
        let existing_map = read_map_from_file(&final_path, format).unwrap_or_default();
        let builtin_entries = builtin_map.unwrap_or_default();

        let mut pending = serde_json::Map::new();
        let mut recovered_from_builtin = 0;

        // 这里需要修改 base_map，因为我们要把 built-in 的内容补充进去
        // 但 existing_map 是只读的，所以我们要先 clone 一份作为 base
        let mut final_base_map = existing_map.clone();

        for (k, v) in &src_map {
            // 如果输出文件里已经有了，跳过
            if final_base_map.contains_key(k) {
                continue;
            }

            // 如果输出文件没有，检查内置汉化
            if let Some(builtin_val) = builtin_entries.get(k) {
                // 有内置汉化，直接使用，不重新翻译
                final_base_map.insert(k.clone(), builtin_val.clone());
                recovered_from_builtin += 1;
            } else {
                // 既没有输出，也没有内置，加入待翻译队列
                pending.insert(k.clone(), v.clone());
            }
        }

        if pending.is_empty() && recovered_from_builtin == 0 {
            log_info!("无新增条目，无需更新: {:?}", final_path);
            return Ok(());
        }

        if recovered_from_builtin > 0 {
            log_info!(
                "从内置汉化中恢复了 {} 个条目 (ModID: {})",
                recovered_from_builtin,
                mod_id
            );
        }

        if !pending.is_empty() {
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
            let raw_path = raw_dir.join(format!("{}_{}", mod_id, original_filename));
            let raw_file = fs::File::create(&raw_path)?;
            serde_json::to_writer_pretty(raw_file, &pending)?;
            log_info!("已备份增量原始内容: {:?}", raw_path);
        }

        (pending, final_base_map)
    } else {
        // [全量模式]
        (src_map, serde_json::Map::new())
    };

    let translated_part =
        execute_translation_batches(&map_to_translate, client, mod_id, &ctx, token).await;

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
