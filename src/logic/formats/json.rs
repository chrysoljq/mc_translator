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

    if skip_existing && final_path.exists() {
        log_success!("跳过已存在: {:?}", final_path);
        return Ok(());
    }

    let content = fs::read_to_string(file_path)?;
    let json_data: serde_json::Value = serde_json::from_str(&content)?;

    if let serde_json::Value::Object(map) = json_data {
        let final_map =
            execute_translation_batches(&map, client, &mod_id, batch_size, &token).await;

        // 检查是否被取消，如果被取消则不保存
        if token.is_cancelled() {
            log_info!("任务已取消，放弃保存 JSON 文件: {:?}", final_path);
            return Ok(());
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut out_file = fs::File::create(&final_path)?;
        let out_json = serde_json::to_string_pretty(&final_map)?;
        out_file.write_all(out_json.as_bytes())?;

        log_success!("JSON 翻译完成 (ModID: {}): {:?}", mod_id, final_path);
    } else {
        log_warn!("JSON 格式错误，根节点必须是对象: {}", file_path.display());
    }

    Ok(())
}
