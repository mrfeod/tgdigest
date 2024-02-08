use grammers_client::types::Channel;

use crate::context::AppContext;
use crate::post::*;
use crate::task::Task;
use crate::util::Result;

async fn get_channel(
    client: &grammers_client::Client,
    channel_name: &str,
) -> Result<grammers_client::types::chat::Chat> {
    match client.resolve_username(channel_name).await? {
        Some(channel) => Ok(channel),
        None => Err(format!("Can't find channel t.me/{}", channel_name).into()),
    }
}

pub async fn download_pic(
    client: &grammers_client::Client,
    task: Task,
    ctx: &AppContext,
) -> Result<std::path::PathBuf> {
    let channel = get_channel(client, task.channel_name.as_str()).await?;
    let photo = channel.photo_downloadable(true);
    match photo {
        Some(photo) => {
            let photo_out: std::path::PathBuf =
                ctx.output_dir.join(format!("{}.png", task.channel_name));
            println!(
                "Path to pic for channed t.me/{} {}",
                task.channel_name,
                photo_out.to_str().unwrap()
            );
            match client.download_media(&photo, photo_out.clone()).await {
                Ok(_) => return Ok(photo_out),
                Err(e) => return Err(e.into()),
            };
        }
        None => {}
    }
    Err(format!("Can't find photo for t.me/{}", task.channel_name).into())
}

pub async fn get_top_posts(client: &grammers_client::Client, task: Task) -> Result<TopPost> {
    let channel = get_channel(client, task.channel_name.as_str()).await?;
    let mut messages = client
        .iter_messages(channel)
        .max_date(task.to_date as i32)
        .limit(30000);
    let mut posts = Post::get_by_date(&mut messages, task.from_date, task.to_date).await?;

    let post_top = TopPost::get_top(task.top_count, &mut posts);
    println!(
        "Fetched data for https://t.me/{} from {} to {}",
        task.channel_name, task.from_date, task.to_date
    );

    return Ok(post_top);
}
