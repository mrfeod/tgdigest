use crate::cli::*;
use crate::util::Result;

use chrono::{DateTime, Local, Utc};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub command: Commands,

    /// t.me/<CHANNEL_NAME>
    pub channel_name: String,

    /// Count of posts in digest
    pub top_count: usize,

    /// Template name from file-configured 'input_dir'
    pub mode: String,

    /// The id of the post to place it in "Editor choice" block
    pub editor_choice_post_id: i32,

    // UTC timestamp
    pub from_date: i64,

    // UTC timestamp
    pub to_date: i64,
}

impl Task {
    pub fn from_cli(cli: Args) -> Self {
        let current_date = DateTime::<Utc>::from_timestamp(Local::now().timestamp(), 0).unwrap();
        let week_ago = current_date - chrono::Duration::days(7);
        Task {
            command: cli.command.clone(),
            channel_name: cli.channel_name.clone(),
            top_count: cli.top_count,
            mode: cli.mode.clone(),
            editor_choice_post_id: cli.editor_choice_post_id,
            from_date: cli.from_date.unwrap_or(week_ago).timestamp(),
            to_date: cli.to_date.unwrap_or(current_date).timestamp(),
        }
    }

    fn from_string(s: &str) -> Result<Task> {
        let task: Task = serde_json::from_str(s)?;
        Ok(task)
    }

    fn to_string(&self) -> Result<String> {
        let task = serde_json::to_string(self)?;
        Ok(task)
    }
}
