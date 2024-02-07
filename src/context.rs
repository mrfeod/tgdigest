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

        let config = path_util::handle_path(
            Some(config.unwrap_or(std::path::PathBuf::from("./cfg.json"))),
            &working_dir,
        )?;

        let data = fs::read_to_string(config).expect("Unable to read file");
        let ctx: AppContext =
            serde_json::from_str(data.as_str()).expect("Unable to parse cfg.json");
        let ctx: AppContext = AppContext {
            input_dir: path_util::handle_path(Some(ctx.input_dir), &working_dir)?,
            output_dir: path_util::handle_path(Some(ctx.output_dir), &working_dir)?,
            tg_session: path_util::handle_path(Some(ctx.tg_session), &working_dir)?,
            ..ctx
        };
        println!("Loaded context {:#?}", ctx);
        Ok(ctx)
    }
}
