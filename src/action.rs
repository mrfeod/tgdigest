#[derive(Copy, Clone)]
pub enum ActionType {
    Replies = 0,
    Reactions,
    Forwards,
    Views,
}

impl ActionType {
    pub fn from(value: usize) -> ActionType {
        match value {
            0 => ActionType::Replies,
            1 => ActionType::Reactions,
            2 => ActionType::Forwards,
            3 => ActionType::Views,
            _ => panic!("No ActionType for {value}"),
        }
    }
}
