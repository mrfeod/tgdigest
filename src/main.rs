mod action;
mod card_renderer;
mod cli;
mod context;
mod html_renderer;
mod path_util;
mod post;
mod task;
mod tg;
mod util;
mod workers;

use std::process::Command;

use crate::card_renderer::CardRenderer;
use crate::cli::*;
use crate::html_renderer::HtmlRenderer;
use crate::task::*;
use crate::util::*;

use chrono::DateTime;
use chrono::Datelike;
use chrono::Days;
use chrono::Months;
use chrono::Utc;
use log;
use once_cell::sync::OnceCell;
use rocket::fs::NamedFile;
use rocket::http::Status;
use rocket::response::content;
use rocket::response::content::RawHtml;
use rocket::response::status;
use simple_logger::SimpleLogger;

#[macro_use]
extern crate rocket;

struct App {
    args: Args,
    ctx: context::AppContext,
    html_renderer: HtmlRenderer,
    card_renderer: CardRenderer,
}

impl App {
    async fn new() -> Result<App> {
        let args = Args::parse_args();

        let ctx = match context::AppContext::new(args.config.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                panic!("Error: {}", e);
            }
        };

        let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
        let card_renderer: CardRenderer = CardRenderer::new().await?;

        Ok(App {
            args,
            ctx,
            html_renderer,
            card_renderer,
        })
    }

    async fn create() -> Result<&'static App> {
        match APP.get() {
            Some(app) => Ok(app),
            None => {
                let app = Self::new().await?;
                APP.set(app).map_err(|_| "Can't set app")?;
                Ok(APP.get().unwrap())
            }
        }
    }

    fn get() -> &'static App {
        APP.get().unwrap()
    }
}

static APP: OnceCell<App> = OnceCell::new();

fn http_status(status: Status, msg: &str) -> status::Custom<String> {
    status::Custom(status, format!("{}: {}", status, msg))
}

fn http_status_err<T>(status: Status, msg: &str) -> std::result::Result<T, status::Custom<String>> {
    Err(http_status(status, msg))
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

#[get("/")]
async fn index() -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    return digest("ithueti", "ithueti", None, None, None, None).await;
}

#[get("/pic/<channel>")]
async fn image(channel: &str) -> std::result::Result<NamedFile, status::Custom<String>> {
    let app = App::get();
    let task = Task {
        channel_name: channel.to_string(),
        command: Commands::Digest {},
        ..Task::from_cli(&app.args)
    };
    println!("Working on task: {}", task.to_string().unwrap());

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

#[get("/digest/<mode>/<channel>/<year>/<month>/<week>?<top_count>&<editor_choice>")]
async fn digest_by_week(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    week: u32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/digest/<mode>/<channel>/<year>/<month>?<top_count>&<editor_choice>")]
async fn digest_by_month(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/digest/<mode>/<channel>/<year>?<top_count>&<editor_choice>")]
async fn digest_by_year(
    mode: &str,
    channel: &str,
    year: i32,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/digest/<mode>/<channel>?<top_count>&<editor_choice>&<from_date>&<to_date>")]
async fn digest(
    mode: &str,
    channel: &str,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
) -> std::result::Result<RawHtml<String>, status::Custom<String>> {
    let app = App::get();
    let task = Task::from_cli(&app.args);
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
    println!("Working on task: {}", task.to_string().unwrap());

    if task.from_date < 0 || task.to_date < 0 {
        return http_status_err(Status::BadRequest, "Provided date is not allowed");
    }

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();
    let future = workers::tg::get_top_posts(client, tg_task);
    let post_top = match future.await {
        Ok(post_top) => post_top,
        Err(e) => return http_status_err(Status::InternalServerError, e.to_string().as_ref()),
    };

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
    println!("Digest html rendered: lenght={}", digest.len());
    Ok(content::RawHtml(digest))
}

#[get("/video/<mode>/<channel>/<year>/<month>/<week>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>")]
async fn video_by_week(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    week: u32,
    replies: usize,
    reactions: usize,
    forwards: usize,
    views: usize,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/video/<mode>/<channel>/<year>/<month>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>")]
async fn video_by_month(
    mode: &str,
    channel: &str,
    year: i32,
    month: u32,
    replies: usize,
    reactions: usize,
    forwards: usize,
    views: usize,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/video/<mode>/<channel>/<year>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>")]
async fn video_by_year(
    mode: &str,
    channel: &str,
    year: i32,
    replies: usize,
    reactions: usize,
    forwards: usize,
    views: usize,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
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
    )
    .await
}

#[get("/video/<mode>/<channel>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<from_date>&<to_date>")]
async fn video(
    mode: &str,
    channel: &str,
    replies: usize,
    reactions: usize,
    forwards: usize,
    views: usize,
    top_count: Option<usize>,
    editor_choice: Option<i32>,
    from_date: Option<i64>,
    to_date: Option<i64>,
) -> std::result::Result<NamedFile, status::Custom<String>> {
    let app = App::get();
    let task = Task::from_cli(&app.args);
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
        ..task
    };
    println!("Working on task: {}", task.to_string().unwrap());

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();
    let post_top = workers::tg::get_top_posts(client, tg_task)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let render_context = workers::cards::create_context(post_top, task.clone())
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    image(channel).await?;

    let rendered_html = app
        .html_renderer
        .render(
            format!("{}/render_template.html", task.mode).as_str(),
            &render_context,
        )
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    println!(
        "Render file rendered to html: lenght={}",
        rendered_html.len()
    );

    let output_dir = app.ctx.output_dir.join(&task.task_id);
    tokio::fs::create_dir_all(&output_dir)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    app.card_renderer
        .render_html(&output_dir, &rendered_html)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))?;

    let video_maker = app
        .ctx
        .input_dir
        .join(format!("{}/make_video.sh", task.mode));
    let video_maker = path_util::to_slash(&video_maker).expect("Can't fix path to make_video.sh");
    println!(
        "Running bash: {} at {}",
        video_maker.to_str().unwrap_or("unknown"),
        output_dir.to_str().unwrap_or("unknown")
    );
    let mut command = if cfg!(windows) {
        Command::new("C:/Program Files/Git/usr/bin/bash.exe")
    } else {
        Command::new("/bin/bash")
    };
    let output = command
        .current_dir(output_dir.to_str().unwrap())
        .arg("-c")
        .arg(video_maker)
        .output()
        .expect("Failed to execute script");

    // Print the output of the script
    println!("Status: {}", output.status);
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));

    let file = output_dir.join("digest.mp4");
    NamedFile::open(file)
        .await
        .map_err(|e| http_status(Status::InternalServerError, e.to_string().as_ref()))
}

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    {
        match tg::TelegramAPI::create().await {
            Ok(_) => {
                println!("Connected to Telegram");
            }
            Err(e) => panic!("Error: {}", e),
        };
        match App::create().await {
            Ok(_) => {
                println!(
                    "Loaded app with config from {}",
                    App::get().args.config.as_ref().unwrap().to_str().unwrap()
                )
            }
            Err(e) => panic!("Error: {}", e),
        };
    }

    rocket::build()
        .mount(
            "/",
            routes![
                index,
                image,
                digest_by_week,
                digest_by_month,
                digest_by_year,
                digest,
                video_by_week,
                video_by_month,
                video_by_year,
                video
            ],
        )
        .launch()
        .await
        .unwrap();
}
