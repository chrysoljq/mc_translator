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
        let fname = file.name();
        if fname.contains("assets") && fname.contains(&ctx.source_lang) {
            if fname.ends_with(".json") || fname.ends_with(".lang") {
                targets.push(fname.to_string());
            }
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

        let is_lang_file = target_path.ends_with(".lang");
        let format = if is_lang_file { FileFormat::Lang } else { FileFormat::Json };

        let src_map = if is_lang_file {
            let mut map = serde_json::Map::new();
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(
                        k.trim().to_string(),
                        serde_json::Value::String(v.trim().to_string()),
                    );
                }
            }
            map
        } else {
            let mut sanitized = crate::logic::common::sanitize_json_content(&content);
            if sanitized.trim().is_empty() {
                 sanitized = "{}".to_string();
            }
            match serde_json::from_str(&sanitized) {
                Ok(serde_json::Value::Object(map)) => map,
                Ok(_) => continue,
                Err(e) => {
                    log_err!("JSON 解析失败: {} -> {} (Error: {})", jar_name, target_path, e);
                    continue;
                }
            }
        };

        let target_filename = crate::logic::common::get_target_filename(&file_name, &ctx.source_lang, &ctx.target_lang);
        
        // 尝试从 JAR 中读取内置汉化 (e.g. assets/modid/lang/zh_cn.json / .lang)
        let builtin_path = Path::new(&target_path)
            .parent()
            .map(|p| p.join(&target_filename))
            .map(|p| p.to_string_lossy().replace('\\', "/")); 

        let mut builtin_map = None;
        if let Some(bp) = builtin_path {
            if let Ok(mut zf) = archive.by_name(&bp) {
                let mut content = String::new();
                if zf.read_to_string(&mut content).is_ok() {
                    if is_lang_file {
                         // Parse built-in lang
                        let mut map = serde_json::Map::new();
                        for line in content.lines() {
                            if let Some((k, v)) = line.split_once('=') {
                                map.insert(k.trim().to_string(), serde_json::Value::String(v.trim().to_string()));
                            }
                        }
                        builtin_map = Some(map);
                    } else {
                        // Parse built-in json, assume it's is standard
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(map) = json.as_object() {
                                builtin_map = Some(map.clone());
                            }
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
            ctx.clone(),
            format,
            builtin_map,
            token,
        )
        .await?;
    }
    Ok(())
}
