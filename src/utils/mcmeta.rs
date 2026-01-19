use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct PackInfo {
    pub pack_format: i32,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mcmeta {
    pub pack: PackInfo,
}

impl Mcmeta {
    pub fn new(pack_format: i32, description: String) -> Self {
        Self {
            pack: PackInfo {
                pack_format,
                description,
            },
        }
    }
}

pub fn write_mcmeta(output_path: &str) -> Result<()> {
    let pack_format = 3;
    let description = "\u{00A7}aAI汉化材质包\u{00A7}r，由 \u{00A7}bmc translator \u{00A7}r生成".to_string();
    let mcmeta = Mcmeta::new(pack_format, description);
    let output_path = Path::new(output_path).join("pack.mcmeta");
    
    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(&mcmeta)?;
    fs::write(output_path, json)?;
    Ok(())
}
