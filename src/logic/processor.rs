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

// 1.21+: expect lang dir
fn detect_ftb_version(root: &Path) -> bool {
    let components: Vec<_> = root.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    
    for i in 0..components.len().saturating_sub(1) {
        if components[i].eq_ignore_ascii_case("quests") 
           && components[i+1].eq_ignore_ascii_case("lang") 
        {
            return true;
        }
    }

    let candidates = [
        root.join("config/ftbquests/quests/lang"),
        root.join("ftbquests/quests/lang"),
        root.join("quests/lang"),
        root.join("lang"),
    ];

    for path in candidates {
        if path.exists() && path.is_dir() {
            return true;
        }
    }
    false
}

fn is_allowed_dir(entry: &DirEntry, root: &Path, is_ftb_1_21: bool, source_lang: &str) -> bool {
    if !entry.file_type().is_dir() { return true; }
    if entry.path() == root { return true; }

    let path = entry.path();
    let name = entry.file_name().to_string_lossy();
    let path_str = path.to_string_lossy();

    // FTB Quests logic for all version
    if path_str.contains("ftbquests") || path_str.contains("quests") {
        if name.eq_ignore_ascii_case("ftbquests") 
            || name.eq_ignore_ascii_case("quests") 
            || name.eq_ignore_ascii_case("config") { return true; }

        if is_ftb_1_21 {
            if name.eq_ignore_ascii_case("lang") { return true; }
            let comps: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy()).collect();
            let has_lang = comps.iter().any(|c| c.eq_ignore_ascii_case("lang"));
            let has_source = comps.iter().any(|c| c.eq_ignore_ascii_case(source_lang));
            
            return has_lang && has_source;
        } else {
            return true;
        }
    }

    // general logic
    let allowed_roots = ["resources", "mods", "kubejs", "assets", "lang"];
    if let Ok(rel) = path.strip_prefix(root) {
        if let Some(first) = rel.components().next() {
            let first_name = first.as_os_str().to_string_lossy();
            if first_name.eq_ignore_ascii_case("config") {
                return rel.components().count() == 1; // 仅允许 config 根
            }
            if allowed_roots.iter().any(|r| first_name.eq_ignore_ascii_case(r)) {
                return true;
            }
        }
    }
    
    let root_name = root.file_name().unwrap_or_default().to_string_lossy();
    allowed_roots.iter().any(|r| root_name.eq_ignore_ascii_case(r))
}

fn should_process_file(path: &Path, config: &AppConfig, is_ftb_1_21: bool) -> bool {
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    let source_lang = &config.source_lang;

    match ext.as_ref() {
        "jar" => true,
        "lang" => path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase().contains(source_lang))
            .unwrap_or(false),
        "json" => path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase().contains(source_lang))
            .unwrap_or(false),

        "snbt" => {
            // if config.skip_quest { return false; }
            if !is_ftb_1_21 { return true; }

            let components: Vec<_> = path.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect();
            if let Some(idx) = components.iter().rposition(|c| c.eq_ignore_ascii_case("lang")) {
                if let Some(next_comp) = components.get(idx + 1) {
                    if next_comp == path.file_name().unwrap().to_str().unwrap() {
                        return path.file_stem().map_or(false, |s| s.eq_ignore_ascii_case(source_lang));
                    }
                    return next_comp.eq_ignore_ascii_case(source_lang);
                }
            }
            false
        },

        _ => false,
    }
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
    let input = config.input_path.clone();
    let output = config.output_path.clone();
    let input_path = Path::new(&input);
    let is_ftb_1_21 = detect_ftb_version(input_path);
    if is_ftb_1_21 {
        log_info!("检测到 FTB Quests (MC 1.21+ 结构)，将仅处理 lang 目录下的本地化文件。");
    } else {
        log_info!("未检测到 FTB Quests 新版结构，将按传统模式扫描 quests。");
    }
    let ctx = Arc::new(TranslationContext{
        batch_size: config.batch_size,
        skip_existing: config.skip_existing,
        update_existing: update_existing,
        network_semaphore: Arc::new(Semaphore::new(config.max_network_concurrency)),
        source_lang: config.source_lang.clone(),
        target_lang: config.target_lang.clone(),
    });

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
        let source_lang = config.source_lang.clone();
        let walker = WalkDir::new(input_path)
            .into_iter()
            .filter_entry(move |e| is_allowed_dir(e, input_path, is_ftb_1_21, &source_lang));

        for entry in walker.flatten() {
            if token.is_cancelled() {
                break;
            }
            
            let path = entry.path().to_path_buf(); // 获取路径的所有权
            
            if path.is_file() {
                if should_process_file(&path, &config, is_ftb_1_21) {
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
                            log_err!("处理失败 [{}]: {}", path.display(), e);
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