use crate::logic::openai::OpenAIClient;
use crate::{log_err, log_info, log_success, log_warn};
use std::path::Path;
use tokio_util::sync::CancellationToken;
use walkdir::{DirEntry, WalkDir};
use crate::logic::formats::{jar, lang, json, snbt};

fn is_allowed_dir(entry: &DirEntry, root: &Path) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }

    if entry.path() == root {
        return true;
    }

    let allowed_dirs = [
        "resources",
        "mods",
        "kubejs",
        "assets",
        "lang",
        "ftbquests",
        "chapters",
    ];

    if let Ok(relative) = entry.path().strip_prefix(root) {
        if let Some(first_component) = relative.components().next() {
            let first_name = first_component.as_os_str().to_string_lossy();

            if allowed_dirs
                .iter()
                .any(|d| first_name.eq_ignore_ascii_case(d))
            {
                return true;
            }
        }
    }

    let root_name = root.file_name().unwrap_or_default().to_string_lossy();
    if allowed_dirs
        .iter()
        .any(|d| root_name.eq_ignore_ascii_case(d))
    {
        return true;
    }

    false
}

async fn dispatch_file(
    path: &Path,
    output: &str,
    client: &OpenAIClient,
    batch_size: usize,
    skip_existing: bool,
    update_existing: bool,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    match ext.as_ref() {
        "jar" => jar::process_jar(path, output, client, batch_size, skip_existing, update_existing, token).await,
        "json" => json::process_json(path, output, client, batch_size, skip_existing, update_existing, token).await,
        "lang" => lang::process_lang(path, output, client, batch_size, skip_existing, update_existing, token).await,
        "snbt" => snbt::process_snbt(path, output, client, batch_size, skip_existing, token).await, 
        _ => {
            log_warn!("跳过不支持的文件: {}", path.display());
            Ok(())
        }
    }
}

pub async fn run_processing_task(
    input: String,
    output: String,
    api_key: String,
    base_url: String,
    model: String,
    batch_size: usize,
    skip_existing: bool,
    update_existing: bool,
    token: CancellationToken,
) {
    let client = OpenAIClient::new(api_key, base_url, model);
    let input_path = Path::new(&input);

    let result = if input_path.is_file() {
        dispatch_file(
            input_path,
            &output,
            &client,
            batch_size,
            skip_existing,
            update_existing,
            &token,
        )
        .await
    } else if input_path.is_dir() {
        let walker = WalkDir::new(input_path)
            .into_iter()
            .filter_entry(|e| is_allowed_dir(e, input_path));

        for entry in walker.flatten() {
            if token.is_cancelled() {
                log_info!("任务已停止");
                break;
            }
            let path = entry.path();
            if path.is_file() {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();

                let should_process = match ext.as_str() {
                    "jar" => true,
                    "lang" => true,
                    "snbt" => true,
                    "json" => path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "en_us.json") // 这里如果不报错就不需要改类型注解
                        .unwrap_or(false),
                    _ => false,
                };

                if should_process {
                    if let Err(e) =
                        dispatch_file(path, &output, &client, batch_size, skip_existing, update_existing, &token)
                            .await
                    {
                        log_warn!("[错误] 处理 {} 失败: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!("无效的输入路径"))
    };

    match result {
        Ok(_) => {
            log_success!("任务已完成！");
        }
        Err(e) => {
            log_err!("发生严重错误: {}", e);
        }
    }
}