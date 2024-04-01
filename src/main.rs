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
use rocket::response::content;
use rocket::response::content::RawHtml;
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
}

static APP: OnceCell<App> = OnceCell::new();

#[get("/")]
async fn index() -> RawHtml<String> {
    return digest("ithueti", "ithueti", None, None, None, None).await;
}

#[get("/pic/<channel>")]
async fn image(channel: &str) -> Option<NamedFile> {
    let app = APP.get().unwrap();
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
        return NamedFile::open(file).await.ok();
    }

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();
    let file = workers::tg::download_pic(client, tg_task, &app.ctx)
        .await
        .unwrap();

    NamedFile::open(file).await.ok()
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
) -> RawHtml<String> {
    let from_date = DateTime::<Utc>::from_timestamp(0, 0)
        .unwrap()
        .with_year(year);
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided year is not allowed".to_string()),
    };

    let from_date = from_date.with_month(month);
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided month is not allowed".to_string()),
    };

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
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided week is not allowed".to_string()),
    };

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
) -> RawHtml<String> {
    let from_date = DateTime::<Utc>::from_timestamp(0, 0)
        .unwrap()
        .with_year(year);
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided year is not allowed".to_string()),
    };

    let from_date = from_date.with_month(month);
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided month is not allowed".to_string()),
    };

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
) -> RawHtml<String> {
    let from_date = DateTime::<Utc>::from_timestamp(0, 0)
        .unwrap()
        .with_year(year);
    let from_date = match from_date {
        Some(from_date) => from_date,
        None => return content::RawHtml("Provided year is not allowed".to_string()),
    };

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
) -> RawHtml<String> {
    let app = APP.get().unwrap();
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

    let tg_task = task.clone();
    let client = tg::TelegramAPI::client();
    let future = workers::tg::get_top_posts(client, tg_task);
    let post_top = match future.await {
        Ok(post_top) => post_top,
        Err(e) => return content::RawHtml(e.to_string()),
    };

    let digest_context = match workers::digest::create_context(post_top, task.clone()) {
        Ok(digest_context) => digest_context,
        Err(e) => {
            return content::RawHtml(e.to_string());
        }
    };
    let digest = match app.html_renderer.render(
        format!("{}/digest_template.html", task.mode).as_str(),
        &digest_context,
    ) {
        Ok(digest) => digest,
        Err(e) => {
            return content::RawHtml(e.to_string());
        }
    };
    println!("Digest html rendered: lenght={}", digest.len());
    content::RawHtml(digest)
}

#[get("/render/<mode>/<channel>?<replies>&<reactions>&<forwards>&<views>&<top_count>&<editor_choice>&<from_date>&<to_date>")]
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
) -> Option<NamedFile> {
    let app = APP.get().unwrap();
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
    let future = workers::tg::get_top_posts(client, tg_task);

    let post_top = match future.await {
        Ok(post_top) => post_top,
        Err(e) => {
            println!("Error: {}", e);
            return None;
        }
    };

    let render_context = match workers::cards::create_context(post_top, task.clone()) {
        Ok(render_context) => render_context,
        Err(e) => {
            println!("Error: {}", e);
            return None;
        }
    };

    image(channel).await;

    let rendered_html = match app.html_renderer.render(
        format!("{}/render_template.html", task.mode).as_str(),
        &render_context,
    ) {
        Ok(rendered_html) => rendered_html,
        Err(e) => {
            println!("Error: {}", e);
            return None;
        }
    };
    println!(
        "Render file rendered to html: lenght={}",
        rendered_html.len()
    );

    let output_dir = app.ctx.output_dir.join(&task.task_id);
    if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
        println!("Failed to create task directory: {}", e);
        return None;
    }

    let card_renderer = CardRenderer::new().await.unwrap();
    match card_renderer.render_html(&output_dir, &rendered_html).await {
        Ok(_) => (),
        Err(e) => {
            println!("Rendering error: {}", e);
            return None;
        }
    }

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
    NamedFile::open(file).await.ok()
}

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    {
        match tg::TelegramAPI::create().await {
            Ok(tg) => {
                println!("Connected to Telegram");
                tg
            }
            Err(e) => panic!("Error: {}", e),
        };
        let app = match APP.get() {
            Some(app) => app,
            None => {
                let app = match App::new().await {
                    Ok(app) => app,
                    Err(e) => panic!("Error: {}", e),
                };
                match APP.set(app) {
                    Ok(_) => {}
                    Err(_) => {
                        panic!("Error on creating App");
                    }
                }
                APP.get().unwrap()
            }
        };
        println!(
            "Load app with config from {}",
            app.args.config.as_ref().unwrap().to_str().unwrap()
        );
    }

    rocket::build()
        .mount(
            "/",
            routes![
                index,
                digest,
                digest_by_week,
                digest_by_month,
                digest_by_year,
                image,
                video
            ],
        )
        .launch()
        .await
        .unwrap();
}
