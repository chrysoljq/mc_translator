use std::path::Path;
use std::fs;
use std::io::Write;
use tokio_util::sync::CancellationToken;
use crate::logic::openai::OpenAIClient;
use crate::logic::common::{execute_translation_batches, extract_mod_id};
use crate::{log_info, log_success, log_warn};
use std::io::{BufRead, BufReader};

pub async fn process_lang(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    skip_existing: bool,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    log_info!("处理 LANG 文件: {}", file_path.display());

    let file = fs::File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut map = serde_json::Map::new();

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

    if map.is_empty() {
        log_warn!("Lang 文件内容为空或格式无法解析");
        return Ok(());
    }

    let mod_id = extract_mod_id(file_path);

    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let new_name = if file_name.contains("en_") {
        file_name
            .replace("en_", "zh_")
            .replace("US", "CN")
            .replace("us", "cn")
    } else {
        format!("zh_CN_{}", file_name)
    };

    let final_path = Path::new(output_root)
        .join("assets")
        .join(&mod_id)
        .join("lang")
        .join(new_name);

    if skip_existing && final_path.exists() {
        log_success!("跳过已存在: {:?}", final_path);
        return Ok(());
    }

    let final_map = execute_translation_batches(&map, client, &mod_id, batch_size, &token).await;

    // 检查是否被取消，如果被取消则不保存
    if token.is_cancelled() {
        log_info!("任务已取消，放弃保存 Lang 文件: {:?}", final_path);
        return Ok(());
    }

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out_file = fs::File::create(&final_path)?;
    for (key, val) in final_map {
        if let Some(str_val) = val.as_str() {
            writeln!(out_file, "{}={}", key, str_val)?;
        }
    }

    log_success!("Lang 翻译完成 (ModID: {}): {:?}", mod_id, final_path);
    Ok(())
}
