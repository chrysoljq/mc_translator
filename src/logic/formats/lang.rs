use crate::log_info;
use crate::logic::common::{
    FileFormat, TranslationContext, core_translation_pipeline, extract_mod_id, read_map_from_file
};
use crate::logic::openai::OpenAIClient;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub async fn process_lang(
    file_path: &Path,
    output_root: &str,
    client: &OpenAIClient,
    ctx: Arc<TranslationContext>,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    log_info!("处理 LANG: {}", file_path.display());

    let src_map = read_map_from_file(file_path, FileFormat::Lang)?;
    if src_map.is_empty() {
        return Ok(());
    }

    let mod_id = extract_mod_id(file_path);
    let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();

    let target_filename = crate::logic::common::get_target_filename(&file_name);

    // 检查是否有同目录的内置汉化文件 (e.g. zh_cn.lang)
    let builtin_path = file_path.with_file_name(&target_filename);
    let mut builtin_map = None;
    if builtin_path.exists() {
         if let Ok(map) = read_map_from_file(&builtin_path, FileFormat::Lang) {
            builtin_map = Some(map);
        }
    }

    core_translation_pipeline(
        src_map,
        &mod_id,
        &file_name,
        Path::new(output_root),
        client,
        ctx.batch_size,
        ctx.skip_existing,
        ctx.update_existing,
        FileFormat::Lang,
        builtin_map,
        token,
    )
    .await
}
