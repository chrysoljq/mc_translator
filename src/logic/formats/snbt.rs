use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use std::sync::Arc;
use regex::Regex;
use tokio_util::sync::CancellationToken;
use crate::logic::openai::OpenAIClient;
use crate::logic::common::{TranslationContext, execute_translation_batches};
use crate::{log_info, log_success};

pub async fn process_snbt(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    ctx: Arc<TranslationContext>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    log_info!("处理 SNBT 任务文件: {}", file_path.display());

    let file_stem = file_path.file_stem().unwrap_or_default().to_string_lossy();
    let output_path = if let Some(idx) = file_path.components().position(|c| c.as_os_str() == "config") {
        let relative_path: PathBuf = file_path.components().skip(idx).collect();
        Path::new(output_root).join(relative_path)
    } else {
        Path::new(output_root).join(file_path.file_name().unwrap())
    };

    if ctx.skip_existing && output_path.exists() {
        log_success!("跳过已存在的文件: {:?}", output_path);
        return Ok(());
    }

    let content = fs::read_to_string(file_path)?;

    // 正则提取：仅提取 title, subtitle 和 description 中的文本
    let mut extracted_map = serde_json::Map::new();
    let mut replacements = Vec::new(); // 存储 (Range, KeyIndex) 以便回填

    // 匹配 title: "..." 或 subtitle: "..."
    let re_kv = Regex::new(r#"(title|subtitle)\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap();
    // 匹配 description: [ ... ] 块
    let re_desc_block = Regex::new(r#"description\s*:\s*\[([\s\S]*?)\]"#).unwrap();
    // 匹配 description 块内部的字符串 "..."
    let re_str = Regex::new(r#""((?:[^"\\]|\\.)*)""#).unwrap();

    let mut counter = 0;

    // 提取 Title/Subtitle
    for caps in re_kv.captures_iter(&content) {
        if let Some(val_match) = caps.get(2) {
            if val_match.as_str().trim().is_empty() || !val_match.as_str().chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            let key = counter.to_string();
            extracted_map.insert(key.clone(), serde_json::Value::String(val_match.as_str().to_string()));
            replacements.push((val_match.range(), key));
            counter += 1;
        }
    }

    // 提取 Description
    for caps in re_desc_block.captures_iter(&content) {
        if let Some(block) = caps.get(1) {
            let block_start = block.start();
            // 在 description 列表内部再次查找字符串
            for str_caps in re_str.captures_iter(block.as_str()) {
                if let Some(inner_match) = str_caps.get(1) {
                     if inner_match.as_str().trim().is_empty() || !inner_match.as_str().chars().any(|c| c.is_alphabetic()) {
                        continue;
                    }
                    let key = counter.to_string();
                    extracted_map.insert(key.clone(), serde_json::Value::String(inner_match.as_str().to_string()));
                    
                    // 计算在整个 content 中的绝对位置
                    let abs_start = block_start + inner_match.start();
                    let abs_end = block_start + inner_match.end();
                    replacements.push((abs_start..abs_end, key));
                    counter += 1;
                }
            }
        }
    }

    if extracted_map.is_empty() {
        log_info!("未发现可翻译内容: {}", file_path.display());
        return Ok(());
    }

    log_info!("提取到 {} 条条目，开始翻译...", extracted_map.len());

    // 这里 mod_id 传入 "ftbquests" 或文件名作为标识
    let translated_map = execute_translation_batches(
        &extracted_map, 
        client, 
        &format!("Quest_{}", file_stem), 
        ctx.batch_size, 
        token
    ).await;

    if token.is_cancelled() {
        return Ok(());
    }

    // 回填内容，根据 Range 的 start 从大到小排序
    replacements.sort_by(|a, b| b.0.start.cmp(&a.0.start));

    let mut new_content = content.clone();
    for (range, key) in replacements {
        if let Some(trans_val) = translated_map.get(&key).and_then(|v| v.as_str()) {
            // 仅当翻译结果不为空时替换
            if !trans_val.trim().is_empty() {
                new_content.replace_range(range, trans_val);
            }
        }
    }

    // 保存
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out_file = fs::File::create(&output_path)?;
    out_file.write_all(new_content.as_bytes())?;

    log_success!("SNBT 翻译完成: {:?}", output_path);
    Ok(())
}
