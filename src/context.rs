use crate::path_util;
use crate::util::*;

use std::fs;

#[derive(Debug, serde::Deserialize)]
pub struct AppContext {
    pub input_dir: std::path::PathBuf,
    pub output_dir: std::path::PathBuf,
    pub tg_session: std::path::PathBuf,
    pub tg_id: i32,
    pub tg_hash: String,
    pub proxy_url: Option<String>,
    pub public_base_url: Option<String>,
    #[serde(default = "default_cache_limit_mb")]
    pub cache_limit_mb: u64,
}

fn default_cache_limit_mb() -> u64 {
    1024
}

impl AppContext {
    pub fn new(config: &std::path::Path) -> Result<AppContext> {
        let working_dir = std::env::current_dir()?;

        let data = fs::read_to_string(config).expect("Unable to read file");
        let ctx: AppContext = serde_json::from_str(&data).expect("Unable to parse cfg.json");
        let ctx: AppContext = AppContext {
            input_dir: path_util::handle_path(Some(ctx.input_dir), &working_dir, None)?,
            output_dir: path_util::handle_path(Some(ctx.output_dir), &working_dir, None)?,
            tg_session: path_util::handle_path(
                Some(ctx.tg_session),
                &working_dir,
                Some(path_util::PathExists::DontCare),
            )?,
            ..ctx
        };
        log::info!("Loaded context {:#?}", ctx);
        Ok(ctx)
    }

    pub fn public_base_url(&self) -> String {
        self.public_base_url
            .as_deref()
            .map(|url| url.trim())
            .map(|url| url.trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| "https://tgd.ithueti.club".to_string())
    }

    pub fn public_site_name(&self) -> String {
        let base_url = self.public_base_url();
        base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string()
    }
}
