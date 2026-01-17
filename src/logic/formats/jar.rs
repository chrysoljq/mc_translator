use std::path::Path;
use std::fs;
use tokio_util::sync::CancellationToken;
use crate::logic::openai::OpenAIClient;
use crate::logic::common::execute_translation_batches;
use crate::{log_info, log_warn, log_success};
use zip::ZipArchive;
use std::io::{Read, Write};

pub async fn process_jar(
    jar_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    skip_existing: bool,
    update_existing: bool,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let file_name = jar_path.file_name().unwrap_or_default().to_string_lossy();
    log_info!("正在扫描 JAR: {}", file_name);

    let file = fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut targets = Vec::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.name().contains("assets") && file.name().ends_with("en_us.json") {
            targets.push(file.name().to_string());
        }
    }

    if targets.is_empty() {
        log_warn!("跳过: 未找到 en_us.json 语言文件");
        return Ok(());
    }

    for target_path in targets {
        if token.is_cancelled() {
            log_info!("JAR 处理已取消");
            break;
        }

        // 从 zip 内部路径提取 modid
        let parts: Vec<&str> = target_path.split('/').collect();
        let mod_id = parts
            .iter()
            .position(|&x| x == "assets")
            .and_then(|i| parts.get(i + 1))
            .unwrap_or(&"unknown");

        let out_sub_path = target_path.replace("en_us.json", "zh_cn.json");
        let final_path = Path::new(output_root).join(out_sub_path);
        if skip_existing && final_path.exists() {
            log_info!("跳过已存在: {} -> {:?}", target_path, final_path);
            continue;
        }

        log_info!("发现语言文件: {} (ModID: {})", target_path, mod_id);

        let mut content = String::new();
        {
            let mut zf = archive.by_name(&target_path)?;
            zf.read_to_string(&mut content)?;
        }

        let json_data: serde_json::Value = serde_json::from_str(&content)?;

        if let serde_json::Value::Object(src_map) = json_data {
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
                        continue;
                    }
                    
                    log_info!("增量更新检测到 {} 个新条目 (ModID: {})", pending_map.len(), mod_id);
                    (pending_map, existing_map)
                } else {
                    (src_map.clone(), serde_json::Map::new())
                }
            } else {
                (src_map.clone(), serde_json::Map::new())
            };

            let translated_part =
                execute_translation_batches(&map_to_translate, client, mod_id, batch_size, &token).await;

            if token.is_cancelled() {
                log_info!("任务已取消，放弃保存 JAR 导出文件: {:?}", final_path);
                break;
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
                 log_success!("JAR 导出更新完成: {:?}", final_path);
            } else {
                 log_success!("JAR 导出生成完成: {:?}", final_path);
            }
        }
    }
    Ok(())
}
