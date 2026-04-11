mod action;
mod cache;
mod card_renderer;
mod cli;
mod context;
mod html_renderer;
mod path_util;
mod post;
mod post_data;
mod task;
mod tg;
mod util;
mod workers;

use crate::cache::PostCache;
use crate::card_renderer::CardRenderer;
use crate::cli::*;
use crate::html_renderer::HtmlRenderer;
use crate::post::TopPost;
use crate::task::*;
use crate::util::*;

use chrono::{DateTime, Datelike, Days, Months, Utc};
use rocket::fs::NamedFile;
use rocket::http::Status;
use rocket::http::ContentType;
use rocket::http::Header;
use rocket::response::{content, Response};
use rocket::response::content::RawHtml;
use rocket::response::status;
use rocket::tokio::sync::Mutex;
use rustc_hash::FxHasher;
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

#[macro_use]
extern crate rocket;

struct App {
    args: Args,
    ctx: context::AppContext,
    cache: PostCache,
    html_renderer: HtmlRenderer,
    card_renderer: CardRenderer,
    render_queue:
        Mutex<HashMap<String, Option<std::result::Result<PathBuf, status::Custom<String>>>>>,
}

impl App {
    async fn new() -> Result<App> {
        let args = Args::parse_args();

        let ctx = match context::AppContext::new(&args.config) {
            Ok(ctx) => ctx,
            Err(e) => {
                panic!("Error: {}", e);
            }
        };

        let db_path = ctx.tg_session.with_file_name("cache.db");
        let cache = PostCache::new(&db_path)?;
        log::info!("Opened cache DB at {}", db_path.display());

        let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
        let card_renderer: CardRenderer = CardRenderer::new().await?;

        Ok(App {
            args,
            ctx,
            cache,
            html_renderer,
            card_renderer,
            render_queue: Mutex::new(HashMap::new()),
        })
    }
}

fn hash(s: String) -> String {
    let mut hasher = FxHasher::default();
    hasher.write(s.as_bytes());
    format!("{:0>20}", hasher.finish().to_string())
}

fn http_status(status: Status, msg: &str) -> status::Custom<String> {
    log::info!("HTTP status {}: {}", status.to_string(), msg);
    status::Custom(status, format!("{}: {}", status, msg))
}

fn http_status_err<T>(status: Status, msg: &str) -> std::result::Result<T, status::Custom<String>> {
    Err(http_status(status, msg))
}

async fn get_top_posts_cached(app: &App, task: &Task, force: bool) -> std::result::Result<TopPost, Box<dyn std::error::Error>> {
    if !force {
        if let Some(mut cached) = app.cache.get_cached_posts(&task.channel_name, task.from_date, task.to_date)? {
            return Ok(TopPost::get_top(task.top_count, &mut cached));
        }
    }

    let client = tg::TelegramAPI::client();
    let mut posts = workers::tg::fetch_posts(&client, task).await?;
    let _ = app.cache.store_posts(&task.channel_name, task.from_date, task.to_date, &posts);

    Ok(TopPost::get_top(task.top_count, &mut posts))
}

fn get_date_from_year(year: i32) -> std::result::Result<DateTime<Utc>, status::Custom<String>> {
    if year < 2014 {
        return http_status_err(Status::BadRequest, "Telegram did not exist");
    };
    match DateTime::<Utc>::from_timestamp(0, 0)
        .unwrap()
        .with_year(year)
    {
        Some(from_date) => Ok(from_date),
        None => http_status_err(Status::BadRequest, "Provided year is not allowed"),
    }
}

fn get_date_from_month(
    year: i32,
    month: u32,
) -> std::result::Result<DateTime<Utc>, status::Custom<String>> {
    let from_date = get_date_from_year(year)?;

    let from_date = from_date.with_month(month);
    match from_date {
        Some(from_date) => Ok(from_date),
        None => http_status_err(Status::BadRequest, "Provided month is not allowed"),
    }
}

/// Weeks start on a Monday. A week refers to the month in which the week
/// starts, so there are 4 months with 5 weeks and 8 with 4 weeks in a year.
/// In 2024, for example, the months with 5 weeks are January, April, July and
/// December.
fn get_date_from_week(
    year: i32,
    month: u32,
    week: u32,
) -> std::result::Result<DateTime<Utc>, status::Custom<String>> {
    let from_date = get_date_from_month(year, month)?;

    let base_day = 1 + from_date
        .with_day(1)
        .unwrap()
        .weekday()
        .number_from_monday();
    let day = match week {
        1..=5 => (week - 1) * 7 + base_day,
        _ => 32, // Overflow day
    };
    let from_date = from_date.with_day(day);
    match from_date {
        Some(from_date) => Ok(from_date),
        None => http_status_err(Status::BadRequest, "Provided week is not allowed"),
    }
}

#[get("/favicon.ico")]
async fn favicon(app: &rocket::State<Arc<App>>) -> Option<NamedFile> {
    let path = app.ctx.input_dir.join("favicon.ico");
    match path.exists() {
        false => None,
        _ => NamedFile::open(path).await.ok(),
    }
}

#[get("/")]
async fn index(
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    return digest("ithueti", "ithueti", None, None, None, None, None, app).await;
}

#[get("/pic/<channel>")]
async fn image(
    channel: &str,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let task = Task {
        channel_name: channel.to_string(),
        command: Commands::Digest {},
        ..Task::default()
    };
    log::debug!("Working on task: {}", task.to_string().unwrap());

    let file = app
        .ctx
        .output_dir
        .join(format!("{}.png", task.channel_name));
    if file.exists() {
        return NamedFile::open(file)
            .await
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_str()));
    }

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();
    let file = workers::tg::download_pic(client, tg_task, &app.ctx)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    NamedFile::open(file)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))
}

#[get("/digest/<mode>/<channel>/<year>/<month>/<week>?<top_count>&<editor_choice>&<force>")]
async fn digest_by_week(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    week: u32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let from_date = get_date_from_week(year, month, week)?;
    let to_date = from_date.checked_add_days(Days::new(7)).unwrap();

    digest(
        mode,
        channel,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get("/digest/<mode>/<channel>/<year>/<month>?<top_count>&<editor_choice>&<force>")]
async fn digest_by_month(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let from_date = get_date_from_month(year, month)?;

    let to_date = from_date.checked_add_months(Months::new(1)).unwrap();

    digest(
        mode,
        channel,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get("/digest/<mode>/<channel>/<year>?<top_count>&<editor_choice>&<force>")]
async fn digest_by_year(
    mode: &str,
    channel: &str,
    year: i32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let from_date = get_date_from_year(year)?;
    let to_date = from_date.checked_add_months(Months::new(12)).unwrap();

    digest(
        mode,
        channel,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get("/digest/<mode>/<channel>?<top_count>&<editor_choice>&<from_date>&<to_date>&<force>")]
async fn digest(
    mode: &str,
    channel: &str,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let task = Task::default();
    let task = Task {
        command: Commands::Digest {},
        mode: mode.to_string(),
        channel_name: channel.to_string(),
        top_count: top_count.unwrap_or(task.top_count),
        editor_choice_post_id: editor_choice.unwrap_or(task.editor_choice_post_id),
        from_date: from_date.unwrap_or(task.from_date),
        to_date: to_date.unwrap_or(task.to_date),
        ..task
    };
    log::debug!("Working on task: {}", task.to_string().unwrap());

    if task.from_date < 0 || task.to_date < 0 {
        return http_status_err(Status::BadRequest, "Provided date is not allowed");
    }

    let post_top = get_top_posts_cached(&app, &task, force.unwrap_or(false))
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let digest_context = match workers::digest::create_context(post_top, task.clone()) {
        Ok(digest_context) => digest_context,
        Err(e) => {
            return http_status_err(Status::InternalServerError, e.to_string().as_ref());
        }
    };
    let digest = match app.html_renderer.render(
        format!("{}/digest_template.html", task.mode).as_str(),
        &digest_context,
    ) {
        Ok(digest) => digest,
        Err(e) => {
            return http_status_err(Status::InternalServerError, e.to_string().as_ref());
        }
    };
    log::trace!("Digest html rendered: lenght={}", digest.len());
    Ok(content::RawHtml(digest))
}

#[get(
    "/video/<mode>/<channel>/<year>/<month>/<week>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<force>"
)]
async fn video_by_week(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    week: u32,
    replies: Option<usize>,
    reactions: Option<usize>,
    forwards: Option<usize>,
    views: Option<usize>,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let from_date = get_date_from_week(year, month, week)?;
    let to_date = from_date.checked_add_days(Days::new(7)).unwrap();

    video(
        mode,
        channel,
        replies,
        reactions,
        forwards,
        views,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get(
    "/video/<mode>/<channel>/<year>/<month>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<force>"
)]
async fn video_by_month(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    replies: Option<usize>,
    reactions: Option<usize>,
    forwards: Option<usize>,
    views: Option<usize>,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let from_date = get_date_from_month(year, month)?;

    let to_date = from_date.checked_add_months(Months::new(1)).unwrap();

    video(
        mode,
        channel,
        replies,
        reactions,
        forwards,
        views,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get(
    "/video/<mode>/<channel>/<year>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<force>"
)]
async fn video_by_year(
    mode: &str,
    channel: &str,
    year: i32,
    replies: Option<usize>,
    reactions: Option<usize>,
    forwards: Option<usize>,
    views: Option<usize>,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let from_date = get_date_from_year(year)?;
    let to_date = from_date.checked_add_months(Months::new(12)).unwrap();

    video(
        mode,
        channel,
        replies,
        reactions,
        forwards,
        views,
        top_count,
        editor_choice,
        Some(from_date.timestamp()),
        Some(to_date.timestamp()),
        force,
        app,
    )
    .await
}

#[get(
    "/video/<mode>/<channel>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<from_date>&<to_date>&<force>"
)]
async fn video(
    mode: &str,
    channel: &str,
    replies: Option<usize>,
    reactions: Option<usize>,
    forwards: Option<usize>,
    views: Option<usize>,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
    force: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let task = Task::default();
    let task = Task {
        command: Commands::Cards {
            replies,
            reactions,
            forwards,
            views,
        },
        mode: mode.to_string(),
        channel_name: channel.to_string(),
        top_count: top_count.unwrap_or(task.top_count),
        editor_choice_post_id: editor_choice.unwrap_or(task.editor_choice_post_id),
        from_date: from_date.unwrap_or(task.from_date),
        to_date: to_date.unwrap_or(task.to_date),
        task_id: "0".to_string(),
    };
    let task = match task.to_string() {
        Ok(task_string) => Task {
            task_id: hash(task_string),
            ..task
        },
        Err(_) => task,
    };

    log::debug!("Working on task: {}", task.to_string().unwrap());

    if task.from_date < 0 || task.to_date < 0 {
        return http_status_err(Status::BadRequest, "Provided date is not allowed");
    }

    let mut queue = app.render_queue.lock().await;
    let render_result = match queue.get(&task.task_id) {
        Some(option) => match option {
            Some(result) => result.clone(),
            None => {
                log::trace!("Task is in progress");
                return http_status_err(Status::Accepted, "Task is in progress, try again later");
            }
        },
        None => Ok(app.ctx.output_dir.join(format!("{}.mp4", task.task_id))),
    };
    // Can remove unconditionaly - it's already done or not started yet
    queue.remove(&task.task_id);

    let file = match render_result {
        Ok(file) => file,
        Err(e) => {
            log::error!("Rendering task failed: {}", e.1);
            return Err(e);
        }
    };

    if file.exists() {
        log::trace!("Used cache: {}", file.to_str().unwrap_or("unknown"));
        match NamedFile::open(file).await {
            Ok(file) => return Ok(file),
            Err(e) => {
                log::error!("Failed to open file: {}", e);
            }
        }
    }

    let tg_task = task.clone();
    let force = force.unwrap_or(false);
    let post_top = get_top_posts_cached(&app, &tg_task, force)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let render_context = workers::cards::create_context(post_top, task.clone())
        .map_err(|e| http_status(Status::BadRequest, e.to_string().as_ref()))?;

    image(&task.channel_name, app).await?;

    let rendered_html = app
        .html_renderer
        .render(
            format!("{}/render_template.html", task.mode).as_str(),
            &render_context,
        )
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    log::debug!(
        "Render file rendered to html: lenght={}",
        rendered_html.len()
    );

    queue.insert(task.task_id.clone(), None);
    drop(queue);

    let app_clone = Arc::clone(app.inner());
    tokio::spawn(async move {
        let result = match render_video(&task, &rendered_html, &app_clone).await {
            Ok(file) => {
                log::info!("Rendered video: {}", file.to_str().unwrap_or("unknown"));
                Ok(file)
            }
            Err(e) => {
                log::error!("Failed to render video: {}", e.1);
                Err(e)
            }
        };
        app_clone
            .render_queue
            .lock()
            .await
            .insert(task.task_id, Some(result));
    });
    http_status_err(Status::Accepted, "Try again later")
}

async fn render_video(
    task: &Task,
    rendered_html: &str,
    app: &Arc<App>,
) -> std::result::Result<PathBuf, status::Custom<String>> {
    let output_dir = app.ctx.output_dir.join(&task.task_id);
    tokio::fs::create_dir_all(&output_dir)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    app.card_renderer
        .render_html(&output_dir, rendered_html)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let video_maker = app
        .ctx
        .input_dir
        .join(format!("{}/make_video.sh", task.mode));
    let video_maker = path_util::to_slash(&video_maker).expect("Can't fix path to make_video.sh");
    log::debug!(
        "Running bash: {} at {}",
        video_maker.to_str().unwrap_or("unknown"),
        output_dir.to_str().unwrap_or("unknown")
    );
    let mut command = if cfg!(windows) {
        Command::new("C:/Program Files/Git/usr/bin/bash.exe")
    } else {
        Command::new("bash")
    };
    let output = command
        .current_dir(output_dir.to_str().unwrap())
        .arg(video_maker)
        .output()
        .expect("Failed to execute script");

    // Print the output of the script
    log::debug!("Status: {}", output.status);
    log::debug!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    log::debug!("Stderr: {}", String::from_utf8_lossy(&output.stderr));

    let file = output_dir.join("digest.mp4");
    if !output.status.success() || !file.exists() {
        return http_status_err(Status::InternalServerError, "Failed to make a video");
    }

    let new_file = app.ctx.output_dir.join(format!("{}.mp4", task.task_id));
    tokio::fs::rename(file, &new_file)
        .await
        .map_err(|_| http_status(Status::InternalServerError, "Failed to move final file"))?;

    Ok(new_file)
}

#[get("/post/<channel>/<id>")]
async fn post_json(
    channel: &str,
    id: i32,
    _app: &rocket::State<Arc<App>>,
) -> std::result::Result<rocket::serde::json::Json<post_data::PostData>, status::Custom<String>> {
    let task = Task {
        command: Commands::Post {},
        channel_name: channel.to_string(),
        editor_choice_post_id: id,
        ..Task::default()
    };
    log::debug!("Working on task: {}", task.to_string().unwrap());

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();

    let post = workers::tg::get_post_data(client, tg_task)
        .await
        .map_err(|e| http_status(Status::NotFound, e.to_string().as_ref()))?;

    Ok(rocket::serde::json::Json(post))
}

#[get("/view/<channel>/<id>?<views>&<forwards>&<reactions>&<comments>&<px_limit>&<dark>")]
async fn view_post(
    channel: &str,
    id: i32,
    views: Option<bool>,
    forwards: Option<bool>,
    reactions: Option<bool>,
    comments: Option<bool>,
    px_limit: Option<u32>,
    dark: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let task = Task {
        command: Commands::Post {},
        channel_name: channel.to_string(),
        editor_choice_post_id: id,
        ..Task::default()
    };

    let client = tg::TelegramAPI::client();
    let post = workers::tg::get_post_data(client, task)
        .await
        .map_err(|e| http_status(Status::NotFound, e.to_string().as_ref()))?;

    // Render entities into HTML text
    let rendered_text = render_entities(&post.text, &post.entities);

    let mut ctx = tera::Context::new();
    ctx.insert("rendered_text", &rendered_text);
    ctx.insert("photo", &post.photo);
    ctx.insert("video", &post.video);
    ctx.insert("document", &post.document);
    ctx.insert("contact", &post.contact);
    ctx.insert("web_page", &post.web_page);
    ctx.insert("forward_from", &post.forward_from);
    ctx.insert("album", &post.album);
    // Determine if media leads the post (before text)
    let has_leading_media = post.photo.is_some() || post.video.is_some() || !post.album.is_empty();
    ctx.insert("has_leading_media", &has_leading_media);
    ctx.insert("channel_name", channel);
    ctx.insert("channel_title", &post.channel_title);
    ctx.insert("userpic_url", &format!("/userpic/{}", channel));
    ctx.insert("post_id", &post.id);
    ctx.insert("post_date", &DateTime::<Utc>::from_timestamp(post.date, 0)
        .map(|dt| dt.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default());
    ctx.insert("dark", &dark.unwrap_or(false));

    let show_views = views.unwrap_or(true);
    let show_forwards = forwards.unwrap_or(true);
    let show_reactions = reactions.unwrap_or(true);
    let show_comments = comments.unwrap_or(true);
    let show_stats = show_views || show_forwards || show_reactions || show_comments;

    ctx.insert("show_stats", &show_stats);
    ctx.insert("show_views", &show_views);
    ctx.insert("show_forwards", &show_forwards);
    ctx.insert("show_reactions", &show_reactions);
    ctx.insert("show_comments", &show_comments);
    ctx.insert("views", &post.views);
    ctx.insert("forwards", &post.forwards);
    ctx.insert("reactions", &post.reactions);
    ctx.insert("comments", &post.replies);
    ctx.insert("px_limit", &px_limit);

    let html = app
        .html_renderer
        .render("view_template.html", &ctx)
        .map_err(|e| http_status(Status::InternalServerError, &e.to_string()))?;

    Ok(RawHtml(html))
}

/// Convert post text + TL entities into HTML.
fn render_entities(text: &str, entities: &[post_data::Entity]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i32;

    // Build a list of (offset, is_open, tag) events
    let mut events: Vec<(i32, bool, String)> = Vec::new();
    for e in entities {
        let open_tag = match e.entity_type.as_str() {
            "bold" => "<b>".to_string(),
            "italic" => "<i>".to_string(),
            "underline" => "<u>".to_string(),
            "strike" => "<s>".to_string(),
            "code" => "<code>".to_string(),
            "pre" => {
                if let Some(lang) = &e.language {
                    format!("<pre><code class=\"language-{}\">", html_escape(lang))
                } else {
                    "<pre><code>".to_string()
                }
            }
            "text_url" => {
                if let Some(url) = &e.url {
                    format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener\">", html_escape(url))
                } else {
                    "<span>".to_string()
                }
            }
            "url" => {
                let url_text: String = chars[e.offset as usize..(e.offset + e.length).min(len) as usize].iter().collect();
                format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener\">", html_escape(&url_text))
            }
            "mention" => {
                let mention: String = chars[e.offset as usize..(e.offset + e.length).min(len) as usize].iter().collect();
                let username = mention.trim_start_matches('@');
                format!("<a href=\"https://t.me/{}\" target=\"_blank\" rel=\"noopener\">", html_escape(username))
            }
            "spoiler" => "<span class=\"spoiler\">".to_string(),
            "blockquote" => "<blockquote>".to_string(),
            "hashtag" | "cashtag" | "phone" | "email" | "bank_card" => "<span>".to_string(),
            _ => continue,
        };
        let close_tag = match e.entity_type.as_str() {
            "bold" => "</b>",
            "italic" => "</i>",
            "underline" => "</u>",
            "strike" => "</s>",
            "code" => "</code>",
            "pre" => "</code></pre>",
            "text_url" | "url" | "mention" => "</a>",
            "spoiler" | "hashtag" | "cashtag" | "phone" | "email" | "bank_card" => "</span>",
            "blockquote" => "</blockquote>",
            _ => continue,
        };
        events.push((e.offset, true, open_tag));
        events.push((e.offset + e.length, false, close_tag.to_string()));
    }
    // Sort: by offset, closes before opens at same position
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut result = String::with_capacity(text.len() * 2);
    let mut pos = 0i32;
    for (offset, _is_open, tag) in &events {
        // Emit plain text up to this offset
        while pos < *offset && pos < len {
            let ch = chars[pos as usize];
            match ch {
                '&' => result.push_str("&amp;"),
                '<' => result.push_str("&lt;"),
                '>' => result.push_str("&gt;"),
                _ => result.push(ch),
            }
            pos += 1;
        }
        result.push_str(tag);
    }
    // Remaining text
    while pos < len {
        let ch = chars[pos as usize];
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(ch),
        }
        pos += 1;
    }
    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[get("/img/<id>")]
async fn post_image(
    id: i64,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let file = app.ctx.output_dir.join(format!("{}.jpg", id));
    log::debug!("Trying to open file: {}", file.to_str().unwrap());
    NamedFile::open(file)
        .await
        .map_err(|e| http_status(Status::NotFound, e.to_string().as_ref()))
}

/// Parse "Range: bytes=START-END" header. Returns (start, optional_end).
fn parse_range_header(range: &str) -> Option<(i64, Option<i64>)> {
    let bytes_prefix = "bytes=";
    if !range.starts_with(bytes_prefix) {
        return None;
    }
    let range = &range[bytes_prefix.len()..];
    let mut parts = range.splitn(2, '-');
    let start: i64 = parts.next()?.parse().ok()?;
    let end: Option<i64> = parts.next().and_then(|s| {
        if s.is_empty() {
            None
        } else {
            s.parse().ok()
        }
    });
    Some((start, end))
}

/// Request guard to extract Range header.
struct RangeHeader(Option<(i64, Option<i64>)>);

#[rocket::async_trait]
impl<'r> rocket::request::FromRequest<'r> for RangeHeader {
    type Error = ();
    async fn from_request(
        req: &'r rocket::Request<'_>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        let range = req
            .headers()
            .get_one("Range")
            .and_then(parse_range_header);
        rocket::request::Outcome::Success(RangeHeader(range))
    }
}

const CHUNK_SIZE: i32 = 512 * 1024; // Must match grammers MAX_CHUNK_SIZE

/// Streaming media response that supports Range requests.
struct MediaStream {
    content_type: ContentType,
    total_size: Option<i64>,
    range_start: i64,
    range_end: i64,
    is_range: bool,
    reader: rocket::tokio::io::DuplexStream,
}

impl<'r> rocket::response::Responder<'r, 'static> for MediaStream {
    fn respond_to(self, _req: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        let mut builder = Response::build();
        builder.header(self.content_type);
        builder.header(Header::new("Accept-Ranges", "bytes"));

        if let Some(total) = self.total_size {
            if self.is_range {
                builder.status(Status::PartialContent);
                builder.header(Header::new(
                    "Content-Range",
                    format!("bytes {}-{}/{}", self.range_start, self.range_end, total),
                ));
                let content_length = self.range_end - self.range_start + 1;
                builder.header(Header::new("Content-Length", content_length.to_string()));
            } else {
                builder.header(Header::new("Content-Length", total.to_string()));
            }
        }

        builder.streamed_body(self.reader);
        builder.ok()
    }
}

#[get("/media/<channel>/<id>")]
async fn media_proxy(
    channel: &str,
    id: i32,
    _app: &rocket::State<Arc<App>>,
    range: RangeHeader,
) -> std::result::Result<MediaStream, status::Custom<String>> {
    let client = tg::TelegramAPI::client();
    let (downloadable, mime, total_size) = workers::tg::resolve_media(&client, channel, id)
        .await
        .map_err(|e| http_status(Status::NotFound, &e.to_string()))?;

    let content_type = ContentType::parse_flexible(&mime).unwrap_or(ContentType::Binary);

    // Parse Range
    let (range_start, range_end, is_range) = match (range.0, total_size) {
        (Some((start, end)), Some(total)) => {
            let end = end.unwrap_or(total - 1).min(total - 1);
            (start, end, true)
        }
        _ => (0, total_size.unwrap_or(0) - 1, false),
    };

    let skip_bytes = if is_range { range_start } else { 0 };
    let skip_chunks_count = (skip_bytes / CHUNK_SIZE as i64) as i32;
    let skip_remainder = (skip_bytes % CHUNK_SIZE as i64) as usize;

    let bytes_to_send = if is_range {
        range_end - range_start + 1
    } else {
        total_size.unwrap_or(i64::MAX)
    };

    let mut download_iter = client.iter_download(&downloadable);
    if skip_chunks_count > 0 {
        download_iter = download_iter.skip_chunks(skip_chunks_count);
    }

    // Bridge DownloadIter chunks into an AsyncRead via DuplexStream
    let (duplex_write, duplex_read) = rocket::tokio::io::duplex(CHUNK_SIZE as usize * 2);

    rocket::tokio::spawn(async move {
        use rocket::tokio::io::AsyncWriteExt;
        let mut writer = duplex_write;
        let mut remaining = bytes_to_send;
        let mut first = true;

        while remaining > 0 {
            match download_iter.next().await {
                Ok(Some(chunk)) => {
                    let mut data = &chunk[..];
                    if first && skip_remainder > 0 {
                        data = &data[skip_remainder..];
                    }
                    first = false;
                    let to_write = data.len().min(remaining as usize);
                    if writer.write_all(&data[..to_write]).await.is_err() {
                        break;
                    }
                    remaining -= to_write as i64;
                }
                _ => break,
            }
        }
    });

    Ok(MediaStream {
        content_type,
        total_size,
        range_start,
        range_end,
        is_range,
        reader: duplex_read,
    })
}

#[get("/userpic/<channel>")]
async fn userpic_proxy(
    channel: &str,
    _app: &rocket::State<Arc<App>>,
) -> std::result::Result<MediaStream, status::Custom<String>> {
    let client = tg::TelegramAPI::client();
    let chat = client
        .resolve_username(channel)
        .await
        .map_err(|e| http_status(Status::InternalServerError, &e.to_string()))?
        .ok_or_else(|| http_status(Status::NotFound, &format!("Channel {} not found", channel)))?;

    let photo = chat
        .photo_downloadable(true)
        .ok_or_else(|| http_status(Status::NotFound, "Channel has no photo"))?;

    let mut download_iter = client.iter_download(&photo);

    let (duplex_write, duplex_read) = rocket::tokio::io::duplex(CHUNK_SIZE as usize * 2);

    rocket::tokio::spawn(async move {
        use rocket::tokio::io::AsyncWriteExt;
        let mut writer = duplex_write;
        while let Ok(Some(chunk)) = download_iter.next().await {
            if writer.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });

    Ok(MediaStream {
        content_type: ContentType::JPEG,
        total_size: None,
        range_start: 0,
        range_end: 0,
        is_range: false,
        reader: duplex_read,
    })
}

#[get("/localmedia/<filename>")]
async fn localmedia_file(
    filename: &str,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    // Sanitize: only allow alphanumeric, dots, dashes, underscores
    if filename.contains('/') || filename.contains('\\') || filename.starts_with('.') {
        return http_status_err(Status::BadRequest, "Invalid filename");
    }
    let file = app.ctx.output_dir.join(filename);
    log::debug!("Serving local media file: {}", file.display());
    NamedFile::open(file)
        .await
        .map_err(|e| http_status(Status::NotFound, e.to_string().as_ref()))
}

#[rocket::main]
async fn main() {
    // #[cfg(debug_assertions)]
    let log_level = log::LevelFilter::Debug;
    // #[cfg(not(debug_assertions))]
    // let log_level = log::LevelFilter::Info;
    SimpleLogger::new().with_level(log_level).init().unwrap();
    log::debug!("Debug mode");

    let app = match App::new().await {
        Ok(app) => {
            log::info!(
                "Loaded app with config from {}",
                app.args.config.to_str().unwrap()
            );
            app
        }
        Err(e) => panic!("Error: {}", e),
    };
    let mut app = Arc::new(app);

    match tg::TelegramAPI::create(&app.ctx).await {
        Ok(_) => {
            log::info!("Connected to Telegram");
            rocket::tokio::task::spawn(async {
                let tg = tg::TelegramAPI::client();
                let tg_ping_timeout = std::time::Duration::from_secs(60);
                let tg_error_exit_code = -1;
                loop {
                    rocket::tokio::time::sleep(tg_ping_timeout).await;
                    // Ping Telegram to keep connection alive
                    match tg.get_me().await {
                        Ok(_) => {
                            log::debug!("Telegram ping successful");
                        }
                        Err(e) => {
                            log::error!("Telegram ping failed: {}", e);
                            // TODO Graceful shutdown on errors
                            std::process::exit(tg_error_exit_code);
                        }
                    }
                }
            });
        }
        Err(e) => panic!("Error: {}", e),
    };

    rocket::build()
        .mount(
            "/",
            routes![
                favicon,
                index,
                image,
                digest_by_week,
                digest_by_month,
                digest_by_year,
                digest,
                video_by_week,
                video_by_month,
                video_by_year,
                video,
                post_json,
                view_post,
                post_image,
                media_proxy,
                userpic_proxy,
                localmedia_file
            ],
        )
        .manage(app.clone())
        .launch()
        .await
        .unwrap();

    log::info!("Rocket server stopped");
    let app = Arc::get_mut(&mut app).unwrap();
    match app.card_renderer.close().await {
        Ok(_) => log::info!("Browser closed"),
        Err(e) => log::error!("{}", e),
    }
}
