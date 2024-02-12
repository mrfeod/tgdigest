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

use log;
use rocket::fs::NamedFile;
use rocket::response::content;
use rocket::response::content::RawHtml;
use rocket::tokio::task::spawn_blocking;
use simple_logger::SimpleLogger;

#[macro_use]
extern crate rocket;

#[macro_use]
extern crate lazy_static;

struct App {
    args: Args,
    ctx: context::AppContext,
    html_renderer: HtmlRenderer,
    card_renderer: CardRenderer,
}

impl App {
    fn new() -> Result<App> {
        let args = Args::parse_args();

        let ctx = match context::AppContext::new(args.config.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                panic!("Error: {}", e);
            }
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let html_renderer: HtmlRenderer = HtmlRenderer::new(&ctx)?;
        let card_renderer: CardRenderer = rt.block_on(CardRenderer::new())?;

        Ok(App {
            args,
            ctx,
            html_renderer,
            card_renderer,
        })
    }
}

lazy_static! {
    static ref APP: App = App::new().unwrap();
}

#[get("/")]
async fn index() -> RawHtml<String> {
    return digest("ithueti", "ithueti", None, None, None, None).await;
}

#[get("/pic/<channel>")]
async fn image(channel: &str) -> Option<NamedFile> {
    let app = &APP;
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
    let handle = spawn_blocking(|| async {
        let tg = tg::TelegramAPI::create().await.unwrap();
        let client = tg.client();
        workers::tg::download_pic(client, tg_task, &app.ctx).await
    })
    .await
    .unwrap();

    let file = handle.await.unwrap();

    NamedFile::open(file).await.ok()
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
    let app = &APP;
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
    let handle = spawn_blocking(|| async {
        let tg = tg::TelegramAPI::create().await.unwrap();
        let client = tg.client();
        workers::tg::get_top_posts(client, tg_task).await
    })
    .await
    .unwrap();

    let post_top = match handle.await {
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
    let app = &APP;
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
    let handle = spawn_blocking(|| async {
        let tg = tg::TelegramAPI::create().await.unwrap();
        let client = tg.client();
        workers::tg::get_top_posts(client, tg_task).await
    })
    .await
    .unwrap();

    let post_top = match handle.await {
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

    let card_renderer = CardRenderer::new().await.unwrap();
    match card_renderer
        .render_html(&app.ctx.output_dir, &rendered_html)
        .await
    {
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
        app.ctx.output_dir.to_str().unwrap_or("unknown")
    );
    let mut command = if cfg!(windows) {
        Command::new("C:/Program Files/Git/usr/bin/bash.exe")
    } else {
        Command::new("/bin/bash")
    };
    let output = command
        .current_dir(app.ctx.output_dir.to_str().unwrap())
        .arg("-c")
        .arg(video_maker)
        .output()
        .expect("Failed to execute script");

    // Print the output of the script
    println!("Status: {}", output.status);
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));

    let file = app.ctx.output_dir.join("digest.mp4");
    NamedFile::open(file).await.ok()
}

#[launch]
fn rocket() -> _ {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    {
        let app = &APP;
        println!(
            "Load app with config from {}",
            app.args.config.as_ref().unwrap().to_str().unwrap()
        );
    }

    rocket::build().mount("/", routes![index, digest, image, video])
}

// async fn async_main() -> Result<()> {
//     let cli = Args::parse_args();
//     let ctx = context::AppContext::new(cli.config.clone())?;
//     let task = Task::from_cli(cli);

//     SimpleLogger::new()
//         .with_level(log::LevelFilter::Debug)
//         .init()
//         .unwrap();

//     let tg = tg::TelegramAPI::create(&ctx).await?;
//     let client = tg.client();

//     let post_top = workers::tg::get_top_posts(&client, task.clone()).await?;

//

//     match &task.command {
//         Commands::Digest {} => {
//             let digest_context = workers::digest::create_context(post_top, task.clone())?;
//             let digest_file = html_renderer.render_to_file(
//                 format!("{}/digest_template.html", task.mode).as_str(),
//                 &digest_context,
//             )?;
//             println!("Digest file rendered: {}", digest_file.to_str().unwrap());
//         }
//         Commands::Cards { .. } => {
//             let render_context = workers::cards::create_context(post_top, task.clone())?;

//             let channel_pic = workers::tg::download_pic(&client, task.clone(), &ctx).await;
//             match channel_pic {
//                 Ok(pic) => {
//                     println!("Downloaded pic: {}", pic.to_str().unwrap());
//                 }
//                 Err(e) => {
//                     println!("Error: {}", e);
//                 }
//             }

//             let rendered_html = html_renderer.render(
//                 format!("{}/render_template.html", task.mode).as_str(),
//                 &render_context,
//             )?;
//             println!(
//                 "Render file rendered to html: lenght={}",
//                 rendered_html.len()
//             );

//             card_renderer
//                 .render_html(&ctx.output_dir, &rendered_html)
//                 .await?;
//         }
//     }

//     // End

//     Ok(())
// }

// fn main() -> Result<()> {
//     runtime::Builder::new_current_thread()
//         .enable_all()
//         .build()
//         .unwrap()
//         .block_on(async_main())
// }
