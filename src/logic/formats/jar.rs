use crate::log_info;
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

        let src_json: serde_json::Value = serde_json::from_str(&content)?;
        let src_map = match src_json {
            serde_json::Value::Object(map) => map,
            _ => continue,
        };

        core_translation_pipeline(
            src_map,
            &mod_id,
            &file_name,
            Path::new(output_root),
            client,
            ctx.batch_size,
            ctx.skip_existing,
            ctx.update_existing,
            FileFormat::Json,
            token,
        )
        .await?;
    }
    Ok(())
}
