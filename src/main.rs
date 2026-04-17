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
use rustc_hash::FxHasher;
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rocket::serde::json::Json;

#[macro_use]
extern crate rocket;

pub struct FetchProgress {
    pub fetched: AtomicUsize,
    pub limit: usize,
    pub done: AtomicBool,
    pub cancelled: AtomicBool,
    pub last_poll: AtomicU64,
    pub error: std::sync::Mutex<Option<String>>,
}

struct App {
    args: Args,
    ctx: context::AppContext,
    cache: PostCache,
    html_renderer: HtmlRenderer,
    card_renderer: CardRenderer,
    fetch_progress: std::sync::Mutex<HashMap<String, Arc<FetchProgress>>>,
    tg_semaphore: Arc<tokio::sync::Semaphore>,
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
        let cache = PostCache::new(&db_path, &ctx.output_dir, ctx.cache_limit_mb)?;
        log::info!("Opened cache DB at {}, media cache in {}", db_path.display(), ctx.output_dir.display());

        let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
        let card_renderer: CardRenderer = CardRenderer::new().await?;

        Ok(App {
            args,
            ctx,
            cache,
            html_renderer,
            card_renderer,
            fetch_progress: std::sync::Mutex::new(HashMap::new()),
            tg_semaphore: Arc::new(tokio::sync::Semaphore::new(2)),
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

fn get_cached_top_posts(app: &App, task: &Task) -> std::result::Result<(TopPost, bool), Box<dyn std::error::Error>> {
    let (mut fresh_posts, stale_ranges) = app.cache.get_posts_and_stale_ranges(
        &task.channel_name, task.from_date, task.to_date,
    )?;
    let is_loading = !stale_ranges.is_empty();
    fresh_posts.sort_by_key(|p| p.id);
    fresh_posts.dedup_by_key(|p| p.id);
    Ok((TopPost::get_top(task.top_count, &mut fresh_posts), is_loading))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn start_background_fetch(app: &Arc<App>, task: &Task, force: bool, force_limit: bool) -> String {
    let limit = if force_limit { 30000 } else { workers::tg::DEFAULT_FETCH_LIMIT };
    let task_id = format!("{}:{}:{}:{}:{}", task.channel_name, task.from_date, task.to_date, force, limit);

    let progress = Arc::new(FetchProgress {
        fetched: AtomicUsize::new(0),
        limit,
        done: AtomicBool::new(false),
        cancelled: AtomicBool::new(false),
        last_poll: AtomicU64::new(now_secs()),
        error: std::sync::Mutex::new(None),
    });

    {
        let mut map = app.fetch_progress.lock().unwrap();
        // Don't start if already running
        if let Some(existing) = map.get(&task_id) {
            if !existing.done.load(Ordering::Relaxed) {
                return task_id;
            }
        }
        // Clean up finished tasks
        map.retain(|_, p| !p.done.load(Ordering::Relaxed));
        map.insert(task_id.clone(), progress.clone());
    }

    let app = app.clone();
    let task = task.clone();
    let tid = task_id.clone();
    let progress_clone = progress.clone();
    tokio::spawn(async move {
        let result = background_fetch(&app, &task, limit, force, &progress).await;
        if let Err(e) = result {
            log::error!("Background fetch error for {}: {}", tid, e);
            *progress.error.lock().unwrap() = Some(e.to_string());
        }
        progress.done.store(true, Ordering::Relaxed);
    });

    // Watchdog: cancel fetch if client stops polling for 10s
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if progress_clone.done.load(Ordering::Relaxed) { break; }
            let elapsed = now_secs() - progress_clone.last_poll.load(Ordering::Relaxed);
            if elapsed > 10 {
                log::info!("Client stopped polling, cancelling fetch");
                progress_clone.cancelled.store(true, Ordering::Relaxed);
                break;
            }
        }
    });

    task_id
}

async fn background_fetch(
    app: &App,
    task: &Task,
    limit: usize,
    force: bool,
    progress: &Arc<FetchProgress>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = tg::TelegramAPI::client();

    if force {
        let posts = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            workers::tg::fetch_posts(&client, task, limit, Some(&progress.fetched), Some(&progress.cancelled)),
        ).await
            .map_err(|_| -> Box<dyn std::error::Error> { "Telegram fetch timed out".into() })??;
        let _ = app.cache.store_posts(&task.channel_name, &posts);
        let _ = app.cache.touch_posts_in_range(&task.channel_name, task.from_date, task.to_date);
        let _ = app.cache.update_fetch_bounds(&task.channel_name, task.from_date, task.to_date);
        return Ok(());
    }

    let (_, stale_ranges) = app.cache.get_posts_and_stale_ranges(
        &task.channel_name, task.from_date, task.to_date,
    )?;

    for (from, to) in &stale_ranges {
        if progress.cancelled.load(Ordering::Relaxed) {
            log::info!("Fetch cancelled between ranges for {}", task.channel_name);
            break;
        }
        log::debug!("Background fetching range [{} .. {}] for {}", from, to, task.channel_name);
        let sub_task = Task {
            from_date: *from,
            to_date: *to,
            ..task.clone()
        };
        let fetched = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            workers::tg::fetch_posts(&client, &sub_task, limit, Some(&progress.fetched), Some(&progress.cancelled)),
        ).await
            .map_err(|_| -> Box<dyn std::error::Error> { "Telegram fetch timed out".into() })??;
        let _ = app.cache.store_posts(&task.channel_name, &fetched);
        let _ = app.cache.touch_posts_in_range(&task.channel_name, *from, *to);
        let _ = app.cache.update_fetch_bounds(&task.channel_name, *from, *to);
    }

    Ok(())
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
    return digest("ithueti", "ithueti", None, None, None, None, None, None, app).await;
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
        None,
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
        None,
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
        None,
        app,
    )
    .await
}

#[get("/digest/<mode>/<channel>?<top_count>&<editor_choice>&<from_date>&<to_date>&<force>&<force_limit>")]
async fn digest(
    mode: &str,
    channel: &str,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
    force: Option<bool>,
    force_limit: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let defaults = Task::default();
    let task = Task {
        command: Commands::Digest {},
        mode: mode.to_string(),
        channel_name: channel.to_string(),
        top_count: top_count.unwrap_or(defaults.top_count),
        editor_choice_post_id: editor_choice.unwrap_or(defaults.editor_choice_post_id),
        from_date: from_date.unwrap_or(defaults.from_date),
        to_date: to_date.unwrap_or(defaults.to_date),
        ..defaults
    };
    log::debug!("Working on task: {}", task.to_string().unwrap());

    if task.from_date < 0 || task.to_date < 0 {
        return http_status_err(Status::BadRequest, "Provided date is not allowed");
    }

    let force = force.unwrap_or(false);
    let force_limit = force_limit.unwrap_or(false);

    // Detect async template: if template source contains "data_url", it fetches data via JS
    let template_name = format!("{}/digest_template.html", task.mode);
    let template_path = app.ctx.input_dir.join(&template_name);
    let is_async_template = std::fs::read_to_string(&template_path)
        .map(|s| s.contains("data_url"))
        .unwrap_or(false);

    if is_async_template {
        // Async template — render shell, JS will fetch from /data/
        let mut data_url = format!(
            "/data/{}/{}?from_date={}&to_date={}&top_count={}&editor_choice={}",
            task.mode, task.channel_name, task.from_date, task.to_date,
            task.top_count, task.editor_choice_post_id
        );
        if force { data_url.push_str("&force=true"); }
        if force_limit { data_url.push_str("&force_limit=true"); }

        let client = tg::TelegramAPI::client();
        let channel_title = workers::tg::get_channel_title(&client, &task.channel_name)
            .await
            .unwrap_or_else(|_| task.channel_name.clone());

        let mut context = tera::Context::new();
        context.insert("channel_name", &task.channel_name);
        context.insert("channel_title", &channel_title);
        context.insert("data_url", &data_url);

        let digest = app.html_renderer.render(&template_name, &context)
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;
        Ok(content::RawHtml(digest))
    } else {
        // Static template — block until data is ready
        let (_, is_stale) = get_cached_top_posts(&app, &task)
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

        if is_stale || force {
            let task_id = start_background_fetch(app.inner(), &task, force, force_limit);
            // Wait for fetch completion, keeping watchdog alive
            loop {
                {
                    let map = app.fetch_progress.lock().unwrap();
                    if let Some(p) = map.get(&task_id) {
                        if p.done.load(Ordering::Relaxed) { break; }
                        p.last_poll.store(now_secs(), Ordering::Relaxed);
                    } else {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }

        let (post_top, _) = get_cached_top_posts(&app, &task)
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

        let client = tg::TelegramAPI::client();
        let channel_title = workers::tg::get_channel_title(&client, &task.channel_name)
            .await
            .unwrap_or_else(|_| task.channel_name.clone());

        let data = workers::digest::create_digest_data(post_top, task.clone(), &channel_title)
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;
        let context = data.to_context();

        let digest = app.html_renderer.render(&template_name, &context)
            .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;
        log::trace!("Digest html rendered (static): length={}", digest.len());
        Ok(content::RawHtml(digest))
    }
}

#[get("/data/<mode>/<channel>?<top_count>&<editor_choice>&<from_date>&<to_date>&<force>&<force_limit>&<task_id>")]
async fn data_endpoint(
    mode: &str,
    channel: &str,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
    force: Option<bool>,
    force_limit: Option<bool>,
    task_id: Option<String>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<Json<serde_json::Value>, status::Custom<String>> {
    let defaults = Task::default();
    let task = Task {
        command: Commands::Digest {},
        mode: mode.to_string(),
        channel_name: channel.to_string(),
        top_count: top_count.unwrap_or(defaults.top_count),
        editor_choice_post_id: editor_choice.unwrap_or(defaults.editor_choice_post_id),
        from_date: from_date.unwrap_or(defaults.from_date),
        to_date: to_date.unwrap_or(defaults.to_date),
        ..defaults
    };

    if task.from_date < 0 || task.to_date < 0 {
        return http_status_err(Status::BadRequest, "Provided date is not allowed");
    }

    // 1. If task_id provided, check its progress
    if let Some(ref tid) = task_id {
        let map = app.fetch_progress.lock().unwrap();
        if let Some(progress) = map.get(tid.as_str()) {
            progress.last_poll.store(now_secs(), Ordering::Relaxed);
            if !progress.done.load(Ordering::Relaxed) {
                return Ok(Json(serde_json::json!({
                    "status": "loading",
                    "task_id": tid,
                    "fetched": progress.fetched.load(Ordering::Relaxed),
                    "limit": progress.limit,
                })));
            }
            let error = progress.error.lock().unwrap().clone();
            if let Some(err) = error {
                return Ok(Json(serde_json::json!({"status": "error", "error": err})));
            }
            // Done successfully — fall through to return data
        }
    }

    // 2. Check cache
    let (_, is_stale) = get_cached_top_posts(&app, &task)
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let force = force.unwrap_or(false);
    let need_fetch = is_stale || force;

    if need_fetch && task_id.is_none() {
        let tid = start_background_fetch(app.inner(), &task, force, force_limit.unwrap_or(false));
        let limit = if force_limit.unwrap_or(false) { 30000 } else { workers::tg::DEFAULT_FETCH_LIMIT };
        return Ok(Json(serde_json::json!({
            "status": "loading",
            "task_id": tid,
            "fetched": 0,
            "limit": limit,
        })));
    }

    // 3. Return data
    let (post_top, _) = get_cached_top_posts(&app, &task)
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let client = tg::TelegramAPI::client();
    let channel_title = workers::tg::get_channel_title(&client, &task.channel_name)
        .await
        .unwrap_or_else(|_| task.channel_name.clone());

    let data = workers::digest::create_digest_data(post_top, task, &channel_title)
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    Ok(Json(data.to_json()))
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

    // Return cached video if available
    let file = app.ctx.output_dir.join(format!("{}.mp4", task.task_id));
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

    // Video needs all data — start fetch and wait for it
    let fetch_task_id = start_background_fetch(app.inner(), &tg_task, force, false);
    loop {
        let is_running = {
            let map = app.fetch_progress.lock().unwrap();
            map.get(&fetch_task_id)
                .is_some_and(|p| !p.done.load(Ordering::Relaxed))
        };
        if !is_running { break; }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let (post_top, _) = get_cached_top_posts(&app, &tg_task)
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let render_context = workers::cards::create_context(post_top, task.clone())
        .map_err(|e| http_status(Status::BadRequest, e.to_string().as_ref()))?;

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

    let file = render_video(&task, &rendered_html, app.inner()).await?;

    match NamedFile::open(file).await {
        Ok(file) => Ok(file),
        Err(e) => http_status_err(Status::InternalServerError, &e.to_string()),
    }
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

#[get("/view/<channel>/<id>?<views>&<forwards>&<reactions>&<comments>&<px_limit>&<dark>&<iframe>")]
async fn view_post(
    channel: &str,
    id: i32,
    views: Option<bool>,
    forwards: Option<bool>,
    reactions: Option<bool>,
    comments: Option<bool>,
    px_limit: Option<u32>,
    dark: Option<bool>,
    iframe: Option<bool>,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let task = Task {
        command: Commands::Post {},
        channel_name: channel.to_string(),
        editor_choice_post_id: id,
        ..Task::default()
    };

    let _permit = app.tg_semaphore.clone().acquire_owned().await
        .map_err(|_| http_status(Status::ServiceUnavailable, "Server is shutting down"))?;

    let client = tg::TelegramAPI::client();
    let post = workers::tg::get_post_data(client, task)
        .await
        .map_err(|e| http_status(Status::NotFound, e.to_string().as_ref()))?;

    drop(_permit);

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
    ctx.insert("post_id", &post.id);
    ctx.insert("post_date", &DateTime::<Utc>::from_timestamp(post.date, 0)
        .map(|dt| dt.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default());
    ctx.insert("dark", &dark.unwrap_or(false));
    ctx.insert("iframe", &iframe.unwrap_or(false));

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

#[get("/thumb/<channel>/<id>")]
async fn thumb_proxy(
    channel: &str,
    id: i32,
    app: &rocket::State<Arc<App>>,
) -> std::result::Result<(ContentType, Vec<u8>), status::Custom<String>> {
    // Check disk cache first (stored as thumb_{channel}_{id}.jpg in media dir)
    let thumb_path = app.cache.media_dir().join(format!("thumb_{}_{}.jpg", channel, id));
    if thumb_path.exists() {
        let data = std::fs::read(&thumb_path)
            .map_err(|e| http_status(Status::InternalServerError, &e.to_string()))?;
        return Ok((ContentType::JPEG, data));
    }

    let permit = app.tg_semaphore.clone().acquire_owned().await
        .map_err(|_| http_status(Status::ServiceUnavailable, "Server is shutting down"))?;

    // Re-check after acquiring permit
    if thumb_path.exists() {
        drop(permit);
        let data = std::fs::read(&thumb_path)
            .map_err(|e| http_status(Status::InternalServerError, &e.to_string()))?;
        return Ok((ContentType::JPEG, data));
    }

    let client = tg::TelegramAPI::client();
    let (bytes, _mime) = workers::tg::download_thumb(&client, channel, id)
        .await
        .map_err(|e| http_status(Status::NotFound, &e.to_string()))?;

    drop(permit);

    // Cache to disk
    let _ = std::fs::write(&thumb_path, &bytes);

    Ok((ContentType::JPEG, bytes))
}

#[get("/media/<channel>/<id>")]
async fn media_proxy(
    channel: &str,
    id: i32,
    app: &rocket::State<Arc<App>>,
    range: RangeHeader,
) -> std::result::Result<MediaStream, status::Custom<String>> {
    // Check disk cache first
    let cached = app.cache.get_cached_media(channel, id).ok().flatten();
    if let Some((path, mime, file_size)) = cached {
        log::debug!("Serving cached media: {}/{}", channel, id);
        return serve_file_media(path, &mime, file_size, range).await;
    }

    // Cache miss — fetch from Telegram (limited concurrency)
    let permit = app.tg_semaphore.clone().acquire_owned().await
        .map_err(|_| http_status(Status::ServiceUnavailable, "Server is shutting down"))?;

    // Re-check cache — another request may have populated it while we waited
    let cached = app.cache.get_cached_media(channel, id).ok().flatten();
    if let Some((path, mime, file_size)) = cached {
        drop(permit);
        log::debug!("Serving cached media (after wait): {}/{}", channel, id);
        return serve_file_media(path, &mime, file_size, range).await;
    }

    let client = tg::TelegramAPI::client();
    let (downloadable, mime, total_size, media_id) = workers::tg::resolve_media(&client, channel, id)
        .await
        .map_err(|e| http_status(Status::NotFound, &e.to_string()))?;

    // If the file is small enough to cache, download fully, cache, then serve from disk
    let should_cache = total_size.map_or(true, |s| s <= 10 * 1024 * 1024); // cache photos (unknown size) and small files
    if should_cache {
        let mut all_bytes = Vec::new();
        let mut download_iter = client.iter_download(&downloadable);
        while let Ok(Some(chunk)) = download_iter.next().await {
            all_bytes.extend_from_slice(&chunk);
            // Safety: abort if somehow exceeds 10MB (e.g. photo larger than expected)
            if all_bytes.len() > 10 * 1024 * 1024 {
                break;
            }
        }

        if all_bytes.len() <= 10 * 1024 * 1024 {
            let _ = app.cache.store_cached_media(channel, id, media_id, &mime, &all_bytes);
            let cached = app.cache.get_cached_media(channel, id).ok().flatten();
            if let Some((path, mime, file_size)) = cached {
                return serve_file_media(path, &mime, file_size, range).await;
            }
        }
        // If store failed, fall through to stream the already-downloaded bytes
        let file_size = all_bytes.len() as i64;
        let content_type = ContentType::parse_flexible(&mime).unwrap_or(ContentType::Binary);
        let (range_start, range_end, is_range) = parse_range_params(range.0, Some(file_size));
        let bytes_to_send = if is_range { range_end - range_start + 1 } else { file_size };

        let (mut duplex_write, duplex_read) = rocket::tokio::io::duplex(CHUNK_SIZE as usize * 2);
        rocket::tokio::spawn(async move {
            use rocket::tokio::io::AsyncWriteExt;
            let start = range_start as usize;
            let end = start + bytes_to_send as usize;
            let _ = duplex_write.write_all(&all_bytes[start..end.min(all_bytes.len())]).await;
        });

        return Ok(MediaStream {
            content_type,
            total_size: Some(file_size),
            range_start,
            range_end,
            is_range,
            reader: duplex_read,
        });
    }

    // Large file — stream directly from Telegram without caching
    let content_type = ContentType::parse_flexible(&mime).unwrap_or(ContentType::Binary);
    let (range_start, range_end, is_range) = parse_range_params(range.0, total_size);

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

    let (duplex_write, duplex_read) = rocket::tokio::io::duplex(CHUNK_SIZE as usize * 2);

    rocket::tokio::spawn(async move {
        use rocket::tokio::io::AsyncWriteExt;
        let _permit = permit; // hold permit until streaming completes
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

fn parse_range_params(range: Option<(i64, Option<i64>)>, total_size: Option<i64>) -> (i64, i64, bool) {
    match (range, total_size) {
        (Some((start, end)), Some(total)) => {
            let end = end.unwrap_or(total - 1).min(total - 1);
            (start, end, true)
        }
        _ => (0, total_size.unwrap_or(0) - 1, false),
    }
}

/// Serve a cached media file from disk with Range support.
async fn serve_file_media(
    path: PathBuf,
    mime: &str,
    file_size: i64,
    range: RangeHeader,
) -> std::result::Result<MediaStream, status::Custom<String>> {
    let content_type = ContentType::parse_flexible(mime).unwrap_or(ContentType::Binary);
    let (range_start, range_end, is_range) = parse_range_params(range.0, Some(file_size));
    let bytes_to_send = if is_range { range_end - range_start + 1 } else { file_size };

    let data = rocket::tokio::fs::read(&path)
        .await
        .map_err(|e| http_status(Status::InternalServerError, &e.to_string()))?;

    let (mut duplex_write, duplex_read) = rocket::tokio::io::duplex(CHUNK_SIZE as usize * 2);
    rocket::tokio::spawn(async move {
        use rocket::tokio::io::AsyncWriteExt;
        let start = range_start as usize;
        let end = (start + bytes_to_send as usize).min(data.len());
        let _ = duplex_write.write_all(&data[start..end]).await;
    });

    Ok(MediaStream {
        content_type,
        total_size: Some(file_size),
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
                data_endpoint,
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
                thumb_proxy,
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
