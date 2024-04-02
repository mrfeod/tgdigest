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
}

impl AppContext {
    pub fn new(config: Option<std::path::PathBuf>) -> Result<AppContext> {
        let working_dir = std::env::current_dir()?;

        let config = config.expect("No config file provided");

        let data = fs::read_to_string(config).expect("Unable to read file");
        let ctx: AppContext =
            serde_json::from_str(data.as_str()).expect("Unable to parse cfg.json");
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
}
