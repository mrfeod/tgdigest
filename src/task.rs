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

    // Unique task id
    pub task_id: String,
}

impl Task {
    pub fn default() -> Self {
        let current_date = DateTime::<Utc>::from_timestamp(Local::now().timestamp(), 0).unwrap();
        let week_ago = current_date - chrono::Duration::days(7);
        Task {
            command: Commands::Digest {},
            channel_name: "ithueti".to_string(),
            top_count: 3,
            mode: "watermark".to_string(),
            editor_choice_post_id: 0,
            from_date: week_ago.timestamp(),
            to_date: current_date.timestamp(),
            task_id: uuid::Uuid::new_v4().as_simple().to_string(),
        }
    }

    pub fn from_string(s: &str) -> Result<Task> {
        let task: Task = serde_json::from_str(s)?;
        Ok(task)
    }

    pub fn to_string(&self) -> Result<String> {
        let task = serde_json::to_string(self)?;
        Ok(task)
    }
}
