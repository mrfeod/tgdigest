use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tgdigest")]
#[command(author = "Anton Sosnin <antsosnin@yandex.ru>")]
#[command(version = "0.5")]
#[command(about = "Create digest for your telegram channel", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,

    /// t.me/<CHANNEL_NAME>
    pub channel_name: String,

    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<std::path::PathBuf>,

    #[arg(long, default_value_t = 3)]
    /// Count of posts in digest
    pub top_count: usize,

    /// Template name from file-configured 'input_dir'
    #[arg(short, long)]
    pub mode: String,

    #[arg(short, long, default_value_t = -1)]
    /// The id of the post to place it in "Editor choice" block
    pub editor_choice_post_id: i32,

    #[arg(short, long)]
    pub from_date: Option<DateTime<Utc>>,

    #[arg(short, long)]
    pub to_date: Option<DateTime<Utc>>,
}

impl Args {
    pub fn parse_args() -> Self {
        Args::parse()
    }
}

#[derive(Subcommand, Clone, serde::Serialize, serde::Deserialize)]
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
}
