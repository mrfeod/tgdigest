use grammers_client::types::{Downloadable, Media};

use crate::context::AppContext;
use crate::post::*;
use crate::post_data::{self, PostData};
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
    client: grammers_client::Client,
    task: Task,
    ctx: &AppContext,
) -> Result<std::path::PathBuf> {
    let channel = get_channel(&client, task.channel_name.as_str()).await?;
    let photo = channel.photo_downloadable(true);
    let Some(photo) = photo else {
        return Err(format!("Can't find photo for t.me/{}", task.channel_name).into());
    };

    let photo_out: std::path::PathBuf = ctx.output_dir.join(format!("{}.png", task.channel_name));
    log::trace!(
        "Path to pic for channel t.me/{} {}",
        task.channel_name,
        photo_out.display()
    );
    client.download_media(&photo, photo_out.clone()).await?;
    Ok(photo_out)
}

pub async fn fetch_posts(client: &grammers_client::Client, task: &Task) -> Result<Vec<Post>> {
    let channel = get_channel(client, task.channel_name.as_str()).await?;
    let mut messages = client
        .iter_messages(channel)
        .max_date(task.to_date as i32)
        .limit(30000);
    let posts = Post::get_by_date(&mut messages, task.from_date, task.to_date).await?;
    log::debug!(
        "Fetched {} posts for https://t.me/{} from {} to {}",
        posts.len(),
        task.channel_name,
        task.from_date,
        task.to_date
    );
    Ok(posts)
}

pub async fn get_top_posts(client: grammers_client::Client, task: Task) -> Result<TopPost> {
    let mut posts = fetch_posts(&client, &task).await?;
    let post_top = TopPost::get_top(task.top_count, &mut posts);
    Ok(post_top)
}

pub async fn get_post(
    client: grammers_client::Client,
    task: Task,
    ctx: &AppContext,
) -> Result<Post> {
    let channel = get_channel(&client, &task.channel_name).await?;
    let message = client
        .get_messages_by_id(channel, &[task.editor_choice_post_id])
        .await?
        .pop()
        .flatten();

    let Some(message) = message else {
        return Err(format!(
            "Can't find post t.me/{}/{}",
            task.channel_name, task.editor_choice_post_id
        )
        .into());
    };

    let photo_id = match message.photo() {
        Some(photo) => {
            let photo_id = photo.id();
            let photo_dowloadable = Downloadable::Media(Media::Photo(photo));
            let photo_out: std::path::PathBuf = ctx.output_dir.join(format!("{}.jpg", photo_id));
            client.download_media(&photo_dowloadable, photo_out).await?;
            Some(photo_id)
        }
        None => None,
    };

    Ok(Post {
        date: message.date().timestamp(),
        id: message.id(),
        views: message.view_count(),
        forwards: message.forward_count(),
        replies: message.reply_count(),
        reactions: message.reaction_count(),
        message: Some(message.msg.message),
        image: photo_id,
    })
}

/// Fetch a single post with full data for the JSON API.
/// No media is downloaded — media URLs point to the /media/ streaming proxy.
/// If the message belongs to a media group (album), fetches all group members.
pub async fn get_post_data(
    client: grammers_client::Client,
    task: Task,
) -> Result<PostData> {
    let channel = get_channel(&client, &task.channel_name).await?;
    let message = client
        .get_messages_by_id(&channel, &[task.editor_choice_post_id])
        .await?
        .pop()
        .flatten();

    let Some(message) = message else {
        return Err(format!(
            "Can't find post t.me/{}/{}",
            task.channel_name, task.editor_choice_post_id
        )
        .into());
    };

    let mut post = post_data::from_message(&message, &task.channel_name);

    // If this message is part of a media group, fetch all album members
    if let Some(grouped_id) = message.grouped_id() {
        // Albums are consecutive messages. Fetch nearby IDs (±10).
        let msg_id = message.id();
        let nearby_ids: Vec<i32> = ((msg_id - 10)..=(msg_id + 10)).collect();
        let nearby = client
            .get_messages_by_id(&channel, &nearby_ids)
            .await?;

        let mut album: Vec<_> = nearby
            .into_iter()
            .flatten()
            .filter(|m| m.grouped_id() == Some(grouped_id))
            .map(|m| post_data::album_item_from_message(&m, &task.channel_name))
            .collect();

        album.sort_by_key(|item| item.msg_id);
        post.album = album;
    }

    Ok(post)
}

/// Resolve media metadata for a post without downloading anything.
/// Returns (Downloadable, mime_type, file_size) for use with iter_download.
/// file_size is None for photos (unknown without downloading).
pub async fn resolve_media(
    client: &grammers_client::Client,
    channel_name: &str,
    msg_id: i32,
) -> Result<(Downloadable, String, Option<i64>)> {
    let channel = get_channel(client, channel_name).await?;
    let message = client
        .get_messages_by_id(channel, &[msg_id])
        .await?
        .pop()
        .flatten()
        .ok_or_else(|| format!("Message {}/{} not found", channel_name, msg_id))?;

    if let Some(photo) = message.photo() {
        return Ok((
            Downloadable::Media(Media::Photo(photo)),
            "image/jpeg".to_string(),
            None,
        ));
    }

    if let Some(media) = message.media() {
        if let Media::Document(doc) = media {
            let mime = doc
                .mime_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let size = doc.size();
            return Ok((Downloadable::Media(Media::Document(doc)), mime, Some(size)));
        }
    }

    Err("No downloadable media in message".into())
}
