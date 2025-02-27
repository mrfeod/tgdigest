use crate::action::ActionType;
use crate::post::Post;
use crate::util;

#[derive(Clone, serde::Serialize)]
pub struct Card {
    pub id: i32,
    pub count: Option<i32>,
    pub header: String,
    pub icon: String,
    pub filter: String,
}

impl Card {
    fn default() -> Self {
        Card {
            id: -1,
            count: None,
            header: String::from("UNDEFINED"),
            icon: util::icon_url("⚠️"),
            filter: String::from(""),
        }
    }

    pub fn create_card(post: Option<&Post>, action: ActionType) -> Card {
        match post {
            None => Card::default(),
            Some(post) => Card {
                id: post.id,
                count: post.count(action),
                ..Card::default()
            },
        }
    }

    pub fn create_cards(posts: &[Post], action: ActionType) -> Option<Vec<Card>> {
        match posts
            .iter()
            .map(|p| Card::create_card(Some(p), action))
            .filter(|c| c.count.is_some())
            .collect::<Vec<Card>>()
        {
            cards if !cards.is_empty() => Some(cards),
            _ => None,
        }
    }
}
