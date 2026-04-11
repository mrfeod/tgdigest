use grammers_tl_types::enums;

// ── JSON response structs ──────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct PostData {
    // Identity
    pub id: i32,
    pub date: i64,
    pub edit_date: Option<i64>,
    pub url: String,

    // Text
    pub text: String,
    pub entities: Vec<Entity>,

    // Media
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<PhotoData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<DocumentData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<ContactData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_page: Option<WebPageData>,

    // Engagement
    pub views: Option<i32>,
    pub forwards: Option<i32>,
    pub replies: Option<i32>,
    pub reactions: Option<i32>,

    // Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_from: Option<ForwardData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_msg_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouped_id: Option<i64>,
    pub pinned: bool,

    // Album: all media items in the group (populated when grouped_id is set)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub album: Vec<AlbumItem>,
}

/// A single media item within an album (media group).
#[derive(serde::Serialize)]
pub struct AlbumItem {
    pub msg_id: i32,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<PhotoData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<DocumentData>,
}

#[derive(serde::Serialize)]
pub struct Entity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub offset: i32,
    pub length: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<i64>,
}

#[derive(serde::Serialize)]
pub struct PhotoData {
    pub id: i64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
}

#[derive(serde::Serialize)]
pub struct VideoData {
    pub id: i64,
    pub url: String,
    pub mime_type: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub round_message: bool,
    pub supports_streaming: bool,
}

#[derive(serde::Serialize)]
pub struct DocumentData {
    pub id: i64,
    pub url: String,
    pub mime_type: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    // Audio-specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioInfo>,
}

#[derive(serde::Serialize)]
pub struct AudioInfo {
    pub duration: i32,
    pub voice: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performer: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ContactData {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: String,
    pub user_id: i64,
}

#[derive(serde::Serialize)]
pub struct WebPageData {
    pub url: String,
    pub display_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
}

#[derive(serde::Serialize)]
pub struct ForwardData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<PeerData>,
    pub date: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_post: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_author: Option<String>,
}

#[derive(serde::Serialize)]
pub struct PeerData {
    #[serde(rename = "type")]
    pub peer_type: String,
    pub id: i64,
}

// ── Conversion from grammers types ─────────────────────────────────────

pub fn extract_entities(entities: &[enums::MessageEntity]) -> Vec<Entity> {
    entities
        .iter()
        .filter_map(|e| {
            let (entity_type, url, language, user_id, document_id) = match e {
                enums::MessageEntity::Unknown(_) => return None,
                enums::MessageEntity::Mention(_) => ("mention", None, None, None, None),
                enums::MessageEntity::Hashtag(_) => ("hashtag", None, None, None, None),
                enums::MessageEntity::BotCommand(_) => ("bot_command", None, None, None, None),
                enums::MessageEntity::Url(_) => ("url", None, None, None, None),
                enums::MessageEntity::Email(_) => ("email", None, None, None, None),
                enums::MessageEntity::Bold(_) => ("bold", None, None, None, None),
                enums::MessageEntity::Italic(_) => ("italic", None, None, None, None),
                enums::MessageEntity::Code(_) => ("code", None, None, None, None),
                enums::MessageEntity::Pre(pre) => {
                    let lang = if pre.language.is_empty() {
                        None
                    } else {
                        Some(pre.language.clone())
                    };
                    ("pre", None, lang, None, None)
                }
                enums::MessageEntity::TextUrl(u) => {
                    ("text_url", Some(u.url.clone()), None, None, None)
                }
                enums::MessageEntity::MentionName(m) => {
                    ("mention_name", None, None, Some(m.user_id), None)
                }
                enums::MessageEntity::InputMessageEntityMentionName(_) => return None,
                enums::MessageEntity::Phone(_) => ("phone", None, None, None, None),
                enums::MessageEntity::Cashtag(_) => ("cashtag", None, None, None, None),
                enums::MessageEntity::Underline(_) => ("underline", None, None, None, None),
                enums::MessageEntity::Strike(_) => ("strike", None, None, None, None),
                enums::MessageEntity::BankCard(_) => ("bank_card", None, None, None, None),
                enums::MessageEntity::Spoiler(_) => ("spoiler", None, None, None, None),
                enums::MessageEntity::CustomEmoji(ce) => {
                    ("custom_emoji", None, None, None, Some(ce.document_id))
                }
                enums::MessageEntity::Blockquote(_) => ("blockquote", None, None, None, None),
            };
            Some(Entity {
                entity_type: entity_type.to_string(),
                offset: e.offset(),
                length: e.length(),
                url,
                language,
                user_id,
                document_id,
            })
        })
        .collect()
}

pub fn extract_forward(fwd: &enums::MessageFwdHeader) -> ForwardData {
    let enums::MessageFwdHeader::Header(h) = fwd;
    ForwardData {
        from_name: h.from_name.clone(),
        from_id: h.from_id.as_ref().map(extract_peer),
        date: h.date as i64,
        channel_post: h.channel_post,
        post_author: h.post_author.clone(),
    }
}

pub fn extract_peer(peer: &enums::Peer) -> PeerData {
    match peer {
        enums::Peer::User(u) => PeerData {
            peer_type: "user".into(),
            id: u.user_id,
        },
        enums::Peer::Chat(c) => PeerData {
            peer_type: "chat".into(),
            id: c.chat_id,
        },
        enums::Peer::Channel(c) => PeerData {
            peer_type: "channel".into(),
            id: c.channel_id,
        },
    }
}

pub fn extract_reply_to_msg_id(reply: &enums::MessageReplyHeader) -> Option<i32> {
    match reply {
        enums::MessageReplyHeader::Header(h) => h.reply_to_msg_id,
        enums::MessageReplyHeader::MessageReplyStoryHeader(_) => None,
    }
}

/// Extract photo dimensions from the largest PhotoSize variant.
pub fn photo_dimensions(sizes: &[enums::PhotoSize]) -> (Option<i32>, Option<i32>) {
    let mut best_w: Option<i32> = None;
    let mut best_h: Option<i32> = None;
    let mut best_size = 0i32;
    for s in sizes {
        match s {
            enums::PhotoSize::Size(ps) => {
                if ps.size > best_size {
                    best_size = ps.size;
                    best_w = Some(ps.w);
                    best_h = Some(ps.h);
                }
            }
            enums::PhotoSize::Progressive(pp) => {
                let max = pp.sizes.iter().copied().max().unwrap_or(0);
                if max > best_size {
                    best_size = max;
                    best_w = Some(pp.w);
                    best_h = Some(pp.h);
                }
            }
            _ => {}
        }
    }
    (best_w, best_h)
}

/// Build media fields from raw MessageMedia. Returns (photo, video, document, contact, web_page).
pub fn extract_media(
    media: &enums::MessageMedia,
    channel_name: &str,
    msg_id: i32,
) -> (
    Option<PhotoData>,
    Option<VideoData>,
    Option<DocumentData>,
    Option<ContactData>,
    Option<WebPageData>,
) {
    match media {
        enums::MessageMedia::Photo(mp) => {
            let photo = mp.photo.as_ref().and_then(|p| match p {
                enums::Photo::Photo(photo) => {
                    let (w, h) = photo_dimensions(&photo.sizes);
                    Some(PhotoData {
                        id: photo.id,
                        url: format!("/media/{}/{}", channel_name, msg_id),
                        width: w,
                        height: h,
                    })
                }
                enums::Photo::Empty(_) => None,
            });
            (photo, None, None, None, None)
        }
        enums::MessageMedia::Document(md) => {
            let Some(enums::Document::Document(doc)) = md.document.as_ref() else {
                return (None, None, None, None, None);
            };
            let filename = doc_filename(&doc.attributes);
            let is_video = doc.attributes.iter().any(|a| matches!(a, enums::DocumentAttribute::Video(_)));
            let is_audio = doc.attributes.iter().any(|a| matches!(a, enums::DocumentAttribute::Audio(_)));

            if is_video {
                let video_attr = doc.attributes.iter().find_map(|a| {
                    if let enums::DocumentAttribute::Video(v) = a { Some(v) } else { None }
                });
                let video = VideoData {
                    id: doc.id,
                    url: format!("/media/{}/{}", channel_name, msg_id),
                    mime_type: doc.mime_type.clone(),
                    size: doc.size,
                    duration: video_attr.map(|v| v.duration),
                    width: video_attr.map(|v| v.w),
                    height: video_attr.map(|v| v.h),
                    filename,
                    round_message: video_attr.is_some_and(|v| v.round_message),
                    supports_streaming: video_attr.is_some_and(|v| v.supports_streaming),
                };
                (None, Some(video), None, None, None)
            } else if is_audio {
                let audio_attr = doc.attributes.iter().find_map(|a| {
                    if let enums::DocumentAttribute::Audio(au) = a { Some(au) } else { None }
                });
                let document = DocumentData {
                    id: doc.id,
                    url: format!("/media/{}/{}", channel_name, msg_id),
                    mime_type: doc.mime_type.clone(),
                    size: doc.size,
                    filename,
                    audio: audio_attr.map(|a| AudioInfo {
                        duration: a.duration,
                        voice: a.voice,
                        title: a.title.clone(),
                        performer: a.performer.clone(),
                    }),
                };
                (None, None, Some(document), None, None)
            } else {
                let document = DocumentData {
                    id: doc.id,
                    url: format!("/media/{}/{}", channel_name, msg_id),
                    mime_type: doc.mime_type.clone(),
                    size: doc.size,
                    filename,
                    audio: None,
                };
                (None, None, Some(document), None, None)
            }
        }
        enums::MessageMedia::Contact(c) => {
            let contact = ContactData {
                phone_number: c.phone_number.clone(),
                first_name: c.first_name.clone(),
                last_name: c.last_name.clone(),
                user_id: c.user_id,
            };
            (None, None, None, Some(contact), None)
        }
        enums::MessageMedia::WebPage(mw) => {
            let wp = match &mw.webpage {
                enums::WebPage::Page(page) => Some(WebPageData {
                    url: page.url.clone(),
                    display_url: page.display_url.clone(),
                    site_name: page.site_name.clone(),
                    title: page.title.clone(),
                    description: page.description.clone(),
                    r#type: page.r#type.clone(),
                    embed_url: page.embed_url.clone(),
                    author: page.author.clone(),
                    duration: page.duration,
                }),
                _ => None,
            };
            (None, None, None, None, wp)
        }
        _ => (None, None, None, None, None),
    }
}

fn doc_filename(attrs: &[enums::DocumentAttribute]) -> Option<String> {
    attrs.iter().find_map(|a| {
        if let enums::DocumentAttribute::Filename(f) = a {
            Some(f.file_name.clone())
        } else {
            None
        }
    })
}

pub fn mime_ext(mime: &str) -> &str {
    match mime {
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/mp4" => "m4a",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        _ => "bin",
    }
}

/// Build a PostData from a grammers Message and channel_name.
pub fn from_message(msg: &grammers_client::types::Message, channel_name: &str) -> PostData {
    let raw = &msg.msg;

    let entities = raw
        .entities
        .as_deref()
        .map(extract_entities)
        .unwrap_or_default();

    let (photo, video, document, contact, web_page) = raw
        .media
        .as_ref()
        .map(|m| extract_media(m, channel_name, msg.id()))
        .unwrap_or((None, None, None, None, None));

    let forward_from = raw.fwd_from.as_ref().map(extract_forward);
    let reply_to_msg_id = raw.reply_to.as_ref().and_then(extract_reply_to_msg_id);

    PostData {
        id: msg.id(),
        date: msg.date().timestamp(),
        edit_date: raw.edit_date.map(|d| d as i64),
        url: format!("https://t.me/{}/{}", channel_name, msg.id()),
        text: raw.message.clone(),
        entities,
        photo,
        video,
        document,
        contact,
        web_page,
        views: msg.view_count(),
        forwards: msg.forward_count(),
        replies: msg.reply_count(),
        reactions: msg.reaction_count(),
        post_author: raw.post_author.clone(),
        forward_from,
        reply_to_msg_id,
        grouped_id: raw.grouped_id,
        pinned: raw.pinned,
        album: Vec::new(),
    }
}

/// Build an AlbumItem from one message in a media group.
pub fn album_item_from_message(
    msg: &grammers_client::types::Message,
    channel_name: &str,
) -> AlbumItem {
    let (photo, video, document, _, _) = msg
        .msg
        .media
        .as_ref()
        .map(|m| extract_media(m, channel_name, msg.id()))
        .unwrap_or((None, None, None, None, None));

    AlbumItem {
        msg_id: msg.id(),
        url: format!("/media/{}/{}", channel_name, msg.id()),
        photo,
        video,
        document,
    }
}
