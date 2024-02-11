pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub type RenderingContext = tera::Context;

pub fn icon_url(icon: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/svg/emoji_u{}.svg",
        format!("{:04x}", icon.chars().nth(0).unwrap_or('❌') as u32)
    )
}
