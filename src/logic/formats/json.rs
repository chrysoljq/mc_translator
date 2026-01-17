use std::path::Path;
use std::fs;
use std::io::Write;
use tokio_util::sync::CancellationToken;
use crate::logic::openai::OpenAIClient;
use crate::logic::common::{execute_translation_batches, extract_mod_id};
use crate::{log_info, log_success, log_warn};

pub async fn process_json(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    skip_existing: bool,
    update_existing: bool,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    log_info!("处理 JSON 文件: {}", file_path.display());

    // 提取 Mod ID (如果路径中没有 assets，会回退使用文件名)
    let mod_id = extract_mod_id(file_path);
    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let new_name = if file_name.contains("en_us") {
        file_name.replace("en_us", "zh_cn")
    } else {
        format!("zh_cn_{}", file_name)
    };

    let final_path = Path::new(output_root)
        .join("assets")
        .join(&mod_id) // 使用提取到的 mod_id
        .join("lang")
        .join(new_name);

    if !update_existing && skip_existing && final_path.exists() {
        log_success!("跳过已存在: {:?}", final_path);
        return Ok(());
    }

    let content = fs::read_to_string(file_path)?;
    let json_data: serde_json::Value = serde_json::from_str(&content)?;

    if let serde_json::Value::Object(src_map) = json_data {
        // 准备待翻译的数据
        let (map_to_translate, mut base_map) = if update_existing && final_path.exists() {
            let existing_content = fs::read_to_string(&final_path).unwrap_or_default();
            let existing_json: serde_json::Value = serde_json::from_str(&existing_content)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            if let serde_json::Value::Object(existing_map) = existing_json {
                let mut pending_map = serde_json::Map::new();
                for (k, v) in &src_map {
                    if !existing_map.contains_key(k) {
                        pending_map.insert(k.clone(), v.clone());
                    }
                }
                
                if pending_map.is_empty() {
                    log_info!("没有检测到新增条目，无需更新: {:?}", final_path);
                    return Ok(());
                }
                
                log_info!("增量更新检测到 {} 个新条目: {:?}", pending_map.len(), final_path);
                (pending_map, existing_map)
            } else {
                (src_map.clone(), serde_json::Map::new())
            }
        } else {
            (src_map.clone(), serde_json::Map::new())
        };

        let translated_part =
            execute_translation_batches(&map_to_translate, client, &mod_id, batch_size, &token).await;

        if token.is_cancelled() {
            log_info!("任务已取消，放弃保存 JSON 文件: {:?}", final_path);
            return Ok(());
        }

        for (k, v) in translated_part {
            base_map.insert(k, v);
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut out_file = fs::File::create(&final_path)?;
        let out_json = serde_json::to_string_pretty(&base_map)?;
        out_file.write_all(out_json.as_bytes())?;

        if update_existing && final_path.exists() {
            log_success!("JSON 更新完成 (ModID: {}): {:?}", mod_id, final_path);
        } else {
            log_success!("JSON 翻译完成 (ModID: {}): {:?}", mod_id, final_path);
        }

    } else {
        log_warn!("JSON 格式错误，根节点必须是对象: {}", file_path.display());
    }

    Ok(())
}
