use crate::context;
use crate::path_util;
use crate::util::*;
use crate::Args;

use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;
use once_cell::sync::OnceCell;
use std::io::{self, BufRead as _, Write as _};

static TG: OnceCell<grammers_client::client::Client> = OnceCell::new();

fn prompt(message: &str) -> Result<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

pub struct TelegramAPI {}

impl TelegramAPI {
    async fn init_client() -> Result<grammers_client::client::Client> {
        println!("Connecting to Telegram...");

        let args = Args::parse_args();

        let ctx = match context::AppContext::new(args.config.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                panic!("Error: {}", e);
            }
        };

        let api_id = ctx.tg_id;
        let api_hash = ctx.tg_hash.clone();
        let tg_session = ctx.tg_session.clone();
        let session = match Session::load_file_or_create(&tg_session) {
            Ok(session) => session,
            Err(why) => panic!(
                "Can't load session file {}: {why}",
                path_util::to_slash(&tg_session).unwrap().to_str().unwrap()
            ),
        };
        let client = Client::connect(Config {
            session,
            api_id,
            api_hash,
            params: Default::default(),
        })
        .await
        .expect("Can't connect to Telegram");
        println!("Connected!");

        if !client.is_authorized().await.expect("Authorization error") {
            println!("Signing in...");
            let phone = prompt("Enter your phone number (international format): ")?;
            let token = client.request_login_code(&phone).await?;
            let code = prompt("Enter the code you received: ")?;
            let signed_in = client.sign_in(&token, &code).await;
            match signed_in {
                Err(SignInError::PasswordRequired(password_token)) => {
                    // Note: this `prompt` method will echo the password in the console.
                    //       Real code might want to use a better way to handle this.
                    let hint = password_token.hint().unwrap_or("None");
                    let prompt_message = format!("Enter the password (hint {}): ", &hint);
                    let password = prompt(prompt_message.as_str())?;

                    client
                        .check_password(password_token, password.trim())
                        .await?;
                }
                Ok(_) => (),
                Err(e) => panic!("{}", e),
            };
            println!("Signed in!");
            match client.session().save_to_file(tg_session) {
                Ok(_) => {}
                Err(e) => {
                    println!("Failed to save the session {}", e);
                }
            }
        }

        Ok(client)
    }

    pub async fn create() -> Result<TelegramAPI> {
        match TG.get() {
            Some(_) => Ok(TelegramAPI {}),
            None => {
                let client = Self::init_client().await?;
                TG.set(client).unwrap();
                Ok(TelegramAPI {})
            }
        }
    }

    pub fn client() -> grammers_client::client::Client {
        // This handle can be `clone()`'d around and freely moved into other tasks, so you can invoke
        // methods concurrently if you need to. While you do this, the single owned `client` is the
        // one that communicates with the network.
        let client = TG.get();
        assert!(client.is_some());
        client.unwrap().clone()
    }
}
