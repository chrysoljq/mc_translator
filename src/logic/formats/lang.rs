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
        token,
    )
    .await
}
