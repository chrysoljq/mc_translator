use crate::logic::common::TranslationContext;
use crate::logic::openai::OpenAIClient;
use crate::{log_err, log_info, log_success, log_warn};
use std::path::Path;
use tokio_util::sync::CancellationToken;
use walkdir::{DirEntry, WalkDir};
use crate::logic::formats::{jar, lang, json, snbt};
use tokio::task::JoinSet;
use tokio::sync::Semaphore;
use std::sync::Arc;
use crate::config::AppConfig;

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
        "config",
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
    ctx: Arc<TranslationContext>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    match ext.as_ref() {
        "jar" => jar::process_jar(path, output, client, ctx, token).await,
        "json" => json::process_json(path, output, client, ctx, token).await,
        "lang" => lang::process_lang(path, output, client, ctx, token).await,
        "snbt" => snbt::process_snbt(path, output, client, ctx, token).await, 
        _ => {
            log_warn!("跳过不支持的文件: {}", path.display());
            Ok(())
        }
    }
}

pub async fn run_processing_task(
    config: AppConfig,
    update_existing: bool,
    token: CancellationToken,
) {
    let client = OpenAIClient::new(config.clone());
    let ctx = Arc::new(TranslationContext{
        batch_size: config.batch_size,
        skip_existing: config.skip_existing,
        update_existing: update_existing,
        network_semaphore: Arc::new(Semaphore::new(config.max_network_concurrency)),
    });
    let input = config.input_path.clone();
    let output = config.output_path.clone();
    let input_path = Path::new(&input);

    let file_semaphore = Arc::new(Semaphore::new(config.file_semaphore));
    let mut tasks = JoinSet::new();

    let result = if input_path.is_file() {
        dispatch_file(
            input_path,
            &output,
            &client,
            ctx.clone(),
            &token,
        )
        .await
    } else if input_path.is_dir() {
        if config.skip_quest {
            log_info!("已跳过ftbquests检查，请检查是否存在任务本地化文件后开启");
        }
        let walker = WalkDir::new(input_path)
            .into_iter()
            .filter_entry(|e| is_allowed_dir(e, input_path));

        for entry in walker.flatten() {
            if token.is_cancelled() {
                log_info!("任务已停止");
                break;
            }
            
            let path = entry.path().to_path_buf(); // 获取路径的所有权
            
            if path.is_file() {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();

                let should_process = match ext.as_str() {
                    "jar" => true,
                    "lang" => path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "en_us.lang")
                        .unwrap_or(false),
                    "snbt" => !config.skip_quest,
                    "json" => path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "en_us.json")
                        .unwrap_or(false),
                    _ => false,
                };

                if should_process {
                    let client = client.clone();
                    let output = output.clone();
                    let token = token.clone();
                    let permit = file_semaphore.clone().acquire_owned().await.unwrap();
                    let ctx = ctx.clone();

                    tasks.spawn(async move {
                        let _permit = permit; 
                        
                        if let Err(e) = dispatch_file(
                            &path, 
                            &output, 
                            &client, 
                            ctx,
                            &token
                        ).await {
                            log_err!("处理 {} 失败: {}", path.display(), e);
                        }
                    });
                }
            }
        }
        while let Some(_) = tasks.join_next().await {}
        Ok(())
    } else {
        Err(anyhow::anyhow!("无效的输入路径"))
    };

    match result {
        Ok(_) => log_success!("任务已完成！"),
        Err(e) => log_err!("发生严重错误: {}", e),
    }
}