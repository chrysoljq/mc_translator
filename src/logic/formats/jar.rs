use crate::{log_info, log_warn, log_err};
use crate::logic::common::{FileFormat, TranslationContext, core_translation_pipeline};
use crate::logic::openai::OpenAIClient;
use std::fs;
use std::io::Read;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use zip::ZipArchive;
use std::sync::Arc;

pub async fn process_jar(
    jar_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    ctx: Arc<TranslationContext>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let jar_name = jar_path.file_name().unwrap_or_default().to_string_lossy();
    log_info!("扫描 JAR: {}", jar_name);

    let file = fs::File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)?;

    // 收集目标文件 (避免借用冲突，先收集文件名)
    let mut targets = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.name().contains("assets") && file.name().ends_with("en_us.json") {
            targets.push(file.name().to_string());
        }
    }

    if targets.is_empty() {
        return Ok(());
    }

    // 遍历处理
    for target_path in targets {
        if token.is_cancelled() {
            break;
        }

        // 解析 Mod ID
        let parts: Vec<&str> = target_path.split('/').collect();
        let assets_index = parts.iter().position(|&x| x == "assets");
        let mod_id = assets_index
            .and_then(|i| parts.get(i + 1))
            .unwrap_or(&"unknown")
            .to_string();
        if mod_id == "minecraft" {
            continue;
        }
        
        let file_name = Path::new(&target_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // 读取 ZIP 内的源内容
        let mut content = String::new();
        {
            let mut zf = archive.by_name(&target_path)?;
            zf.read_to_string(&mut content)?;
        }

        if content.trim().is_empty() {
            log_warn!("跳过空文件: {} -> {}", jar_name, target_path);
            continue;
        }

        let mut sanitized = crate::logic::common::sanitize_json_content(&content);
        // 如果处理后只剩空内容（比如只有注释），视为失效
        if sanitized.trim().is_empty() {
             sanitized = "{}".to_string();
        }

        let src_json: serde_json::Value = match serde_json::from_str(&sanitized) {
            Ok(v) => v,
            Err(e) => {
                log_err!("JSON 解析失败: {} -> {} (Error: {})", jar_name, target_path, e);
                let snippet: String = sanitized.chars().take(200).collect();
                log_err!("Sanitized snippet: {}", snippet);
                continue;
            }
        };
        let src_map = match src_json {
            serde_json::Value::Object(map) => map,
            _ => continue,
        };

        let target_filename = crate::logic::common::get_target_filename(&file_name);
        
        // 尝试从 JAR 中读取内置汉化 (e.g. assets/modid/lang/zh_cn.json)
        let builtin_path = Path::new(&target_path)
            .parent()
            .map(|p| p.join(&target_filename))
            .map(|p| p.to_string_lossy().replace('\\', "/")); // ZIP use forward slashes

        let mut builtin_map = None;
        if let Some(bp) = builtin_path {
             // zip lookup is case sensitive usually, but standard mc layout is lowercase
            if let Ok(mut zf) = archive.by_name(&bp) {
                let mut content = String::new();
                if zf.read_to_string(&mut content).is_ok() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(map) = json.as_object() {
                            builtin_map = Some(map.clone());
                        }
                    }
                }
            }
        }

        core_translation_pipeline(
            src_map,
            &mod_id,
            &file_name,
            Path::new(output_root),
            client,
            ctx.clone(), // Clone needed because of loop
            FileFormat::Json,
            builtin_map,
            token,
        )
        .await?;
    }
    Ok(())
}
