use crate::action::ActionType;
use crate::post::TopPost;
use crate::task::*;
use crate::util::*;
use crate::workers::block::Block;
use crate::workers::card::Card;

#[derive(serde::Serialize)]
pub struct DigestData {
    pub blocks: Vec<Block>,
    pub editor_choice_id: i32,
    pub channel_name: String,
    pub channel_title: String,
}

impl DigestData {
    pub fn to_context(&self) -> RenderingContext {
        let mut context = RenderingContext::new();
        context.insert("blocks", &self.blocks);
        context.insert("editor_choice_id", &self.editor_choice_id);
        context.insert("channel_name", &self.channel_name);
        context.insert("channel_title", &self.channel_title);
        context
    }

    /// Slim JSON for /data/ endpoint: blocks have only header and cards: [[id, count], ...]
    pub fn to_json(&self) -> serde_json::Value {
        let blocks: Vec<serde_json::Value> = self.blocks.iter().map(|b| {
            let cards: Vec<[i32; 2]> = b.cards.as_ref().map(|cards| {
                cards.iter().map(|c| [c.id, c.count.unwrap_or(0)]).collect()
            }).unwrap_or_default();
            serde_json::json!({
                "header": b.header,
                "cards": cards,
            })
        }).collect();
        serde_json::json!({
            "status": "ready",
            "blocks": blocks,
            "editor_choice_id": self.editor_choice_id,
            "channel_name": self.channel_name,
            "channel_title": self.channel_title,
        })
    }
}

pub fn create_digest_data(post_top: TopPost, task: Task, channel_title: &str) -> Result<DigestData> {
    log::debug!("Creating digest data");
    let get_posts = |action: ActionType| post_top.index(action);
    let blocks = vec![
        Block {
            header: String::from("По комментариям"),
            icon: icon_url("💬"),
            cards: Card::create_cards(get_posts(ActionType::Replies), ActionType::Replies),
            ..Block::default()
        },
        Block {
            header: String::from("По реакциям"),
            icon: icon_url("👏"),
            cards: Card::create_cards(get_posts(ActionType::Reactions), ActionType::Reactions),
            ..Block::default()
        },
        Block {
            header: String::from("По репостам"),
            icon: icon_url("🔁"),
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Forwards), ActionType::Forwards),
        },
        Block {
            header: String::from("По просмотрам"),
            icon: icon_url("👁️"),
            filter: String::from("filter-blue"),
            cards: Card::create_cards(get_posts(ActionType::Views), ActionType::Views),
        },
    ]
    .into_iter()
    .filter(|b| b.cards.is_some())
    .collect::<Vec<Block>>();

    Ok(DigestData {
        blocks,
        editor_choice_id: task.editor_choice_post_id,
        channel_name: task.channel_name.clone(),
        channel_title: channel_title.to_string(),
    })
}
