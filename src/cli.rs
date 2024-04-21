use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tgdigest")]
#[command(author = "Anton Sosnin <antsosnin@yandex.ru>")]
#[command(version = "0.5")]
#[command(about = "Create digest for your telegram channel", long_about = None)]
pub struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    pub config: std::path::PathBuf,
}

impl Args {
    pub fn parse_args() -> Self {
        Args::parse()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum Commands {
    /// Generate cards from chosen digest posts from 1 to <TOP_COUNT>
    Cards {
        replies: Option<usize>,
        reactions: Option<usize>,
        forwards: Option<usize>,
        views: Option<usize>,
    },

    /// Generate digest
    Digest {},

    /// Show post
    Post {},
}
