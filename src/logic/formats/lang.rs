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
    update_existing: bool, // [新增] 增量更新开关
    token: &CancellationToken,
) -> anyhow::Result<()> {
    log_info!("处理 LANG 文件: {}", file_path.display());

    let src_map = match read_lang_file(file_path) {
        Ok(map) => map,
        Err(e) => {
            log_warn!("读取 Lang 文件失败或格式错误: {} ({})", file_path.display(), e);
            return Ok(());
        }
    };

    if src_map.is_empty() {
        log_warn!("Lang 文件内容为空: {}", file_path.display());
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

    if !update_existing && skip_existing && final_path.exists() {
        log_success!("跳过已存在: {:?}", final_path);
        return Ok(());
    }

    let (map_to_translate, mut base_map) = if update_existing && final_path.exists() {
        let existing_map = read_lang_file(&final_path).unwrap_or_else(|_| serde_json::Map::new());
        
        let mut pending_map = serde_json::Map::new();
        for (k, v) in &src_map {
            if !existing_map.contains_key(k) {
                pending_map.insert(k.clone(), v.clone());
            }
        }

        if pending_map.is_empty() {
            log_success!("没有检测到新增条目，无需更新: {:?}", final_path);
            return Ok(());
        }

        log_info!("增量更新检测到 {} 个新条目: {:?}", pending_map.len(), final_path);
        (pending_map, existing_map)
    } else {
        (src_map, serde_json::Map::new())
    };

    let translated_part = execute_translation_batches(&map_to_translate, client, &mod_id, batch_size, &token).await;

    if token.is_cancelled() {
        log_info!("任务已取消，放弃保存 Lang 文件: {:?}", final_path);
        return Ok(());
    }

    for (k, v) in translated_part {
        base_map.insert(k, v);
    }

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out_file = fs::File::create(&final_path)?;
    
    for (key, val) in base_map {
        if let Some(str_val) = val.as_str() {
            writeln!(out_file, "{}={}", key, str_val)?;
        }
    }

    if update_existing && final_path.exists() {
        log_success!("Lang 更新完成 (ModID: {}): {:?}", mod_id, final_path);
    } else {
        log_success!("Lang 翻译完成 (ModID: {}): {:?}", mod_id, final_path);
    }
    
    Ok(())
}

fn read_lang_file(path: &Path) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut map = serde_json::Map::new();

    for line in reader.lines() {
        let line = line?;
        // 跳过空行和注释
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }
        // 分割 key=value
        if let Some((k, v)) = line.split_once('=') {
            map.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().to_string()),
            );
        }
    }
    Ok(map)
}
