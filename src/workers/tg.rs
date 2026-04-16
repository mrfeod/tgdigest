use grammers_client::types::{Downloadable, Media};
use grammers_client::client::files::DownloadIter;

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

pub const DEFAULT_FETCH_LIMIT: usize = 1000;

pub async fn fetch_posts(
    client: &grammers_client::Client,
    task: &Task,
    limit: usize,
    progress: Option<&std::sync::atomic::AtomicUsize>,
    cancelled: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Vec<Post>> {
    let channel = get_channel(client, task.channel_name.as_str()).await?;
    let mut messages = client
        .iter_messages(channel)
        .max_date(task.to_date as i32)
        .limit(limit);
    let mut posts: Vec<Post> = Vec::new();
    while let Some(message) = messages.next().await? {
        if cancelled.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            log::info!("Fetch cancelled for {}, returning {} posts", task.channel_name, posts.len());
            break;
        }
        let date = message.date().timestamp();
        if date > task.to_date {
            continue;
        }
        if date < task.from_date {
            break;
        }
        let grouped_id = message.grouped_id();
        let post = Post {
            date,
            id: message.id(),
            views: message.view_count(),
            forwards: message.forward_count(),
            replies: message.reply_count(),
            reactions: message.reaction_count(),
            message: Some(message.msg.message),
            image: None,
            grouped_id,
        };
        posts.push(post);
        if let Some(p) = progress {
            p.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    log::debug!(
        "Fetched {} posts for https://t.me/{} from {} to {}",
        posts.len(),
        task.channel_name,
        task.from_date,
        task.to_date
    );
    Ok(posts)
}

pub async fn get_channel_title(client: &grammers_client::Client, channel_name: &str) -> Result<String> {
    let channel = get_channel(client, channel_name).await?;
    Ok(channel.name().to_string())
}

pub async fn get_top_posts(client: grammers_client::Client, task: Task) -> Result<TopPost> {
    let mut posts = fetch_posts(&client, &task, DEFAULT_FETCH_LIMIT, None, None).await?;
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
        grouped_id: None,
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
    post.channel_title = Some(channel.name().to_string());

    // Resolve forward source username from cached entities
    if let Some(ref mut fwd) = post.forward_from {
        if let Some(ref from_id) = fwd.from_id {
            let peer = match from_id.peer_type.as_str() {
                "channel" => Some(grammers_tl_types::enums::Peer::Channel(
                    grammers_tl_types::types::PeerChannel { channel_id: from_id.id },
                )),
                "user" => Some(grammers_tl_types::enums::Peer::User(
                    grammers_tl_types::types::PeerUser { user_id: from_id.id },
                )),
                _ => None,
            };
            if let Some(peer) = peer {
                if let Some(chat) = message.chats.get(&peer) {
                    fwd.from_username = chat.username().map(|s| s.to_string());
                }
            }
        }
    }

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
/// Returns (Downloadable, mime_type, file_size, media_id) for use with iter_download.
/// file_size is None for photos (unknown without downloading).
pub async fn resolve_media(
    client: &grammers_client::Client,
    channel_name: &str,
    msg_id: i32,
) -> Result<(Downloadable, String, Option<i64>, i64)> {
    let channel = get_channel(client, channel_name).await?;
    let message = client
        .get_messages_by_id(channel, &[msg_id])
        .await?
        .pop()
        .flatten()
        .ok_or_else(|| format!("Message {}/{} not found", channel_name, msg_id))?;

    if let Some(photo) = message.photo() {
        let media_id = photo.id();
        return Ok((
            Downloadable::Media(Media::Photo(photo)),
            "image/jpeg".to_string(),
            None,
            media_id,
        ));
    }

    if let Some(media) = message.media() {
        match media {
            Media::Document(doc) => {
                let media_id = doc.id();
                let mime = doc
                    .mime_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let size = doc.size();
                return Ok((Downloadable::Media(Media::Document(doc)), mime, Some(size), media_id));
            }
            Media::Sticker(sticker) => {
                let doc = sticker.document;
                let media_id = doc.id();
                let mime = doc
                    .mime_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let size = doc.size();
                return Ok((Downloadable::Media(Media::Document(doc)), mime, Some(size), media_id));
            }
            _ => {}
        }
    }

    Err("No downloadable media in message".into())
}

/// Download the thumbnail of a video/document.
/// Returns (bytes, mime_type) — the thumb is small enough to download in one go.
pub async fn download_thumb(
    client: &grammers_client::Client,
    channel_name: &str,
    msg_id: i32,
) -> Result<(Vec<u8>, String)> {
    let channel = get_channel(client, channel_name).await?;
    let message = client
        .get_messages_by_id(channel, &[msg_id])
        .await?
        .pop()
        .flatten()
        .ok_or_else(|| format!("Message {}/{} not found", channel_name, msg_id))?;

    let raw_media = message.msg.media.as_ref()
        .ok_or("No media in message")?;

    use grammers_tl_types::{enums, types};

    let doc = match raw_media {
        enums::MessageMedia::Document(md) => {
            match md.document.as_ref() {
                Some(enums::Document::Document(doc)) => doc,
                _ => return Err("No document in message".into()),
            }
        }
        _ => return Err("Message has no document media".into()),
    };

    // Find the best thumb size — prefer "m" (320px), fall back to largest available
    let thumbs = doc.thumbs.as_ref()
        .ok_or("Document has no thumbnails")?;

    let best_type = thumbs.iter().find_map(|t| match t {
        enums::PhotoSize::Size(s) if s.r#type == "m" => Some(s.r#type.clone()),
        _ => None,
    }).or_else(|| thumbs.iter().rev().find_map(|t| match t {
        enums::PhotoSize::Size(s) => Some(s.r#type.clone()),
        _ => None,
    })).ok_or("No downloadable thumb size found")?;

    let location = types::InputDocumentFileLocation {
        id: doc.id,
        access_hash: doc.access_hash,
        file_reference: doc.file_reference.clone(),
        thumb_size: best_type,
    };

    let mut iter = DownloadIter::new_from_location(client, location.into());
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = iter.next().await {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > 1024 * 1024 {
            break; // safety limit for a thumbnail
        }
    }

    if bytes.is_empty() {
        return Err("Thumbnail download returned empty data".into());
    }

    Ok((bytes, "image/jpeg".to_string()))
}
