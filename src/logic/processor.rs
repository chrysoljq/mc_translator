use crate::logging::{LogEntry, LogLevel};
use crate::logic::openai::OpenAIClient;
use crate::message::AppMsg;
use crossbeam_channel::Sender;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write}; // 引入 BufRead, BufReader 用于 process_lang
use std::path::Path;
use walkdir::WalkDir;
use zip::ZipArchive;

fn log(sender: &Sender<AppMsg>, level: LogLevel, msg: String) {
    let _ = sender.send(AppMsg::Log(LogEntry::new(level, msg)));
}

async fn dispatch_file(
    path: &Path,
    output: &str,
    client: &OpenAIClient,
    batch_size: usize,
    sender: &Sender<AppMsg>,
) -> anyhow::Result<()> {
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    // 这里调用你之前定义的 process_jar / process_json / process_lang
    match ext.as_ref() {
        "jar" => process_jar(path, output, client, batch_size, sender).await,
        "json" => process_json(path, output, client, batch_size, sender).await,
        "lang" => process_lang(path, output, client, batch_size, sender).await,
        _ => {
            log(sender, LogLevel::Warn, format!("跳过不支持的文件: {}", path.display()));
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
    sender: Sender<AppMsg>,
) {
    let client = OpenAIClient::new(api_key.clone(), base_url.clone(), model.clone());
    
    log(&sender, LogLevel::Info, "正在验证连接...".to_string());
    
    match client.fetch_models().await {
        Ok(_) => log(&sender, LogLevel::Success, "API 连接验证通过".to_string()),
        Err(e) => {
            log(&sender, LogLevel::Error, format!("连接验证失败: {}", e));
            log(&sender, LogLevel::Error, "任务终止".to_string());
            return;
        }
    }

    // 这里不需要重新创建 client，复用上面的即可，或者 clone
    let client = OpenAIClient::new(api_key, base_url, model); 
    let input_path = Path::new(&input);

    let result = if input_path.is_file() {
        // 直接调用辅助函数，而不是闭包
        dispatch_file(input_path, &output, &client, batch_size, &sender).await
    } else if input_path.is_dir() {
        for entry in WalkDir::new(input_path).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                // 简单过滤，避免对非目标文件调用 async 逻辑
                if ["jar", "json", "lang"].contains(&ext.as_str()) {
                    if let Err(e) = dispatch_file(path, &output, &client, batch_size, &sender).await {
                         let _ = log(&sender, LogLevel::Warn, format!("[错误] 处理 {} 失败: {}", path.display(), e));
                    }
                }
            }
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!("无效的输入路径"))
    };

    match result {
        Ok(_) => { let _ = log(&sender, LogLevel::Success, "任务已完成！".to_string()); },
        Err(e) => { let _ = log(&sender, LogLevel::Error, format!("发生严重错误: {}", e)); }
    }
}

async fn execute_translation_batches(
    map: &serde_json::Map<String, serde_json::Value>,
    client: &OpenAIClient,
    mod_id: &str,
    batch_size: usize,
    sender: &Sender<AppMsg>,
) -> serde_json::Map<String, serde_json::Value> {
    let safe_batch_size = if batch_size == 0 { 1 } else { batch_size };
    let total_items = map.len();
    let keys: Vec<String> = map.keys().cloned().collect();
    let mut final_map = serde_json::Map::new();

    for (idx, chunk) in keys.chunks(safe_batch_size).enumerate() {
        log(
            sender,
            LogLevel::Info,
            format!(
                "正在翻译 [{}] 第 {}/{} 批 (共 {} 条)",
                mod_id,
                idx + 1,
                (total_items + safe_batch_size - 1) / safe_batch_size,
                total_items
            ),
        );

        let mut sub_map = serde_json::Map::new();
        for k in chunk {
            if let Some(v) = map.get(k) {
                sub_map.insert(k.clone(), v.clone());
            }
        }

        match client.translate_batch(sub_map.clone(), mod_id).await {
            Ok(translated) => final_map.extend(translated),
            Err(e) => {
                log(
                    sender,
                    LogLevel::Warn,
                    format!("批次失败 (保留原文): {}", e),
                );
                final_map.extend(sub_map); // 失败回退
            }
        }
    }
    final_map
}

fn extract_mod_id(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    if let Some(idx) = parts.iter().position(|x| x == "assets") {
        if idx + 1 < parts.len() {
            return parts[idx + 1].to_string();
        }
    }

    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

async fn process_jar(
    jar_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    sender: &Sender<AppMsg>,
) -> anyhow::Result<()> {
    let file_name = jar_path.file_name().unwrap_or_default().to_string_lossy();
    log(
        sender,
        LogLevel::Info,
        format!("正在扫描 JAR: {}", file_name),
    );

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
        log(
            sender,
            LogLevel::Warn,
            "跳过: 未找到 en_us.json 语言文件".to_string(),
        );
        return Ok(());
    }

    for target_path in targets {
        // 从 zip 内部路径提取 modid
        let parts: Vec<&str> = target_path.split('/').collect();
        let mod_id = parts
            .iter()
            .position(|&x| x == "assets")
            .and_then(|i| parts.get(i + 1))
            .unwrap_or(&"unknown");

        log(
            sender,
            LogLevel::Info,
            format!("发现语言文件: {} (ModID: {})", target_path, mod_id),
        );

        let mut content = String::new();
        {
            let mut zf = archive.by_name(&target_path)?;
            zf.read_to_string(&mut content)?;
        }

        let json_data: serde_json::Value = serde_json::from_str(&content)?;

        if let serde_json::Value::Object(map) = json_data {
            // 使用提取的通用逻辑
            let final_map =
                execute_translation_batches(&map, client, mod_id, batch_size, sender).await;

            // 保存
            let out_sub_path = target_path.replace("en_us.json", "zh_cn.json");
            let final_path = Path::new(output_root).join(out_sub_path);
            if let Some(parent) = final_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut out_file = fs::File::create(&final_path)?;
            let out_json = serde_json::to_string_pretty(&final_map)?;
            out_file.write_all(out_json.as_bytes())?;

            log(
                sender,
                LogLevel::Info,
                format!("已保存 JAR 导出文件: {:?}", final_path),
            );
        }
    }
    Ok(())
}

async fn process_json(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    sender: &Sender<AppMsg>,
) -> anyhow::Result<()> {
    log(sender, LogLevel::Info, format!("处理 JSON 文件: {}", file_path.display()));

    let content = fs::read_to_string(file_path)?;
    // 提取 Mod ID (如果路径中没有 assets，会回退使用文件名)
    let mod_id = extract_mod_id(file_path);

    let json_data: serde_json::Value = serde_json::from_str(&content)?;

    if let serde_json::Value::Object(map) = json_data {
        let final_map = execute_translation_batches(&map, client, &mod_id, batch_size, sender).await;

        // 构造文件名：en_us -> zh_cn
        let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let new_name = if file_name.contains("en_us") {
            file_name.replace("en_us", "zh_cn")
        } else {
            format!("zh_cn_{}", file_name)
        };
        
        // --- 修改开始：构建标准的资源包路径 assets/<modid>/lang/ ---
        let final_path = Path::new(output_root)
            .join("assets")
            .join(&mod_id) // 使用提取到的 mod_id
            .join("lang")
            .join(new_name);
        // --- 修改结束 ---

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut out_file = fs::File::create(&final_path)?;
        let out_json = serde_json::to_string_pretty(&final_map)?;
        out_file.write_all(out_json.as_bytes())?;

        log(sender, LogLevel::Success, format!("JSON 翻译完成 (ModID: {}): {:?}", mod_id, final_path));
    } else {
        log(sender, LogLevel::Warn, format!("JSON 格式错误，根节点必须是对象: {}", file_path.display()));
    }

    Ok(())
}

async fn process_lang(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    batch_size: usize,
    sender: &Sender<AppMsg>,
) -> anyhow::Result<()> {
    log(sender, LogLevel::Info, format!("处理 LANG 文件: {}", file_path.display()));

    let file = fs::File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut map = serde_json::Map::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue; 
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), serde_json::Value::String(v.trim().to_string()));
        }
    }

    if map.is_empty() {
        log(sender, LogLevel::Warn, "Lang 文件内容为空或格式无法解析".to_string());
        return Ok(());
    }

    let mod_id = extract_mod_id(file_path);
    let final_map = execute_translation_batches(&map, client, &mod_id, batch_size, sender).await;

    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let new_name = if file_name.contains("en_") {
        file_name.replace("en_", "zh_").replace("US", "CN").replace("us", "cn")
    } else {
        format!("zh_CN_{}", file_name)
    };

    let final_path = Path::new(output_root)
        .join("assets")
        .join(&mod_id)
        .join("lang")
        .join(new_name);

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut out_file = fs::File::create(&final_path)?;
    for (key, val) in final_map {
        if let Some(str_val) = val.as_str() {
            writeln!(out_file, "{}={}", key, str_val)?;
        }
    }

    log(sender, LogLevel::Success, format!("Lang 翻译完成 (ModID: {}): {:?}", mod_id, final_path));
    Ok(())
}