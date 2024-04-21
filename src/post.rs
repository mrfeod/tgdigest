use crate::action::ActionType;
use crate::util::Result;

use chrono::{DateTime, Utc};
use partial_sort::PartialSort;

#[derive(Clone, serde::Serialize)]
pub struct Post {
    pub date: i64,
    pub id: i32,
    pub views: Option<i32>,
    pub forwards: Option<i32>,
    pub replies: Option<i32>,
    pub reactions: Option<i32>,
    pub message: Option<String>,
    pub image: Option<i64>,
}

impl Post {
    pub async fn get_by_date(
        messages: &mut grammers_client::client::messages::MessageIter,
        from_date: i64,
        to_date: i64,
    ) -> Result<Vec<Post>> {
        let mut posts: Vec<Post> = Vec::new();
        while let Some(message) = messages.next().await? {
            let message: grammers_client::types::Message = message;

            let date = message.date().timestamp();
            if date > to_date {
                continue;
            }
            if date < from_date {
                break;
            }

            posts.push(Post {
                date: date,
                id: message.id(),
                views: message.view_count(),
                forwards: message.forward_count(),
                replies: message.reply_count(),
                reactions: message.reaction_count(),
                message: Some(message.msg.message),
                image: None,
            });
        }

        Result::Ok(posts)
    }

    pub fn count(&self, index: ActionType) -> Option<i32> {
        match index {
            ActionType::Replies => self.replies,
            ActionType::Reactions => self.reactions,
            ActionType::Forwards => self.forwards,
            ActionType::Views => self.views,
        }
    }
}

#[derive(serde::Serialize)]
pub struct TopPost {
    pub top_count: usize,
    pub replies: Vec<Post>,
    pub reactions: Vec<Post>,
    pub forwards: Vec<Post>,
    pub views: Vec<Post>,
}

impl TopPost {
    fn get_top_by(top_count: usize, posts: &mut Vec<Post>, action: ActionType) -> Vec<Post> {
        let mut top_count = top_count;
        if posts.len() < top_count {
            // panic!("Size of posts less than {}", top_count)
            top_count = posts.len();
        }

        posts.partial_sort(top_count, |a, b| b.count(action).cmp(&a.count(action)));
        posts[0..top_count].to_vec()
    }

    pub fn get_top(top_count: usize, posts: &mut Vec<Post>) -> TopPost {
        TopPost {
            top_count,
            replies: Self::get_top_by(top_count, posts, ActionType::Replies),
            reactions: Self::get_top_by(top_count, posts, ActionType::Reactions),
            forwards: Self::get_top_by(top_count, posts, ActionType::Forwards),
            views: Self::get_top_by(top_count, posts, ActionType::Views),
        }
    }

    pub fn index(&self, index: ActionType) -> &Vec<Post> {
        match index {
            ActionType::Replies => &self.replies,
            ActionType::Reactions => &self.reactions,
            ActionType::Forwards => &self.forwards,
            ActionType::Views => &self.views,
        }
    }

    fn print(&self) {
        let headers = [
            format!("Top {} by comments:", self.top_count),
            format!("Top {} by reactions:", self.top_count),
            format!("Top {} by forwards:", self.top_count),
            format!("Top {} by views:", self.top_count),
        ];
        for (index, header) in headers.iter().enumerate() {
            println!("{header}");
            let action = ActionType::from(index);
            for (pos, post) in self.index(action).iter().enumerate() {
                match post.count(action) {
                    Some(count) => {
                        println!(
                            "\t{}. {}: {}\t({})",
                            pos + 1,
                            post.id,
                            count,
                            DateTime::<Utc>::from_timestamp(post.date, 0).unwrap()
                        );
                    }
                    None => {
                        println!("No data");
                        break;
                    }
                }
            }
            println!("");
        }
    }
}
