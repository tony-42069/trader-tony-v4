//! One-shot interactive Telegram login.
//!
//! Run locally (NOT on Railway) once to generate the session file:
//!   cargo run --bin tg_login
//!
//! Reads TG_API_ID, TG_API_HASH, TG_PHONE, TG_SESSION_PATH from the env.
//! Prompts on stdin for SMS code and (if enabled) 2FA password.

use anyhow::{anyhow, Context, Result};
use dotenv::dotenv;
use grammers_client::{Client, Config, InitParams, SignInError};
use grammers_session::Session;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn prompt(label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let api_id: i32 = env::var("TG_API_ID")
        .context("TG_API_ID not set")?
        .parse()
        .context("TG_API_ID must be an integer")?;
    let api_hash = env::var("TG_API_HASH").context("TG_API_HASH not set")?;
    let phone = env::var("TG_PHONE").context("TG_PHONE not set (e.g. +14155551234)")?;
    let session_path = PathBuf::from(
        env::var("TG_SESSION_PATH").unwrap_or_else(|_| "data/tg_session.session".to_string()),
    );

    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let session = if session_path.exists() {
        println!(
            "Existing session at {:?} — loading and verifying...",
            session_path
        );
        Session::load_file(&session_path).context("Failed to load existing session")?
    } else {
        // grammers-session 0.7.0's save_to_file uses OpenOptions::open without
        // create(true), so it requires the file to exist. Pre-create it now.
        std::fs::File::create(&session_path)
            .with_context(|| format!("Failed to pre-create session file at {:?}", session_path))?;
        Session::new()
    };

    // grammers 0.7.0: Config takes { session, api_id, api_hash, params }.
    // InitParams::default() is correct; no extra fields needed for login.
    let client = Client::connect(Config {
        session,
        api_id,
        api_hash,
        params: InitParams::default(),
    })
    .await
    .map_err(|e| anyhow!("Failed to connect to Telegram: {:?}", e))?;

    if client.is_authorized().await.unwrap_or(false) {
        println!("✅ Session already authorised. Saving and exiting.");
        client
            .session()
            .save_to_file(&session_path)
            .context("Failed to save session file")?;
        return Ok(());
    }

    // grammers 0.7.0: request_login_code takes only &phone (no api_hash param).
    println!("Requesting SMS/Telegram login code for {}...", phone);
    let token = client
        .request_login_code(&phone)
        .await
        .map_err(|e| anyhow!("Failed to request login code: {:?}", e))?;

    let code = prompt("Enter the code you received")?;

    // grammers 0.7.0 SignInError variants:
    //   SignUpRequired { terms_of_service: Option<TermsOfService> }
    //   PasswordRequired(PasswordToken)
    //   InvalidCode
    //   InvalidPassword        (no inner value)
    //   Other(InvocationError)
    match client.sign_in(&token, &code).await {
        Ok(_) => {}
        Err(SignInError::PasswordRequired(password_token)) => {
            let password = prompt("2FA password")?;
            // check_password takes PasswordToken by value and password: impl AsRef<[u8]>.
            client
                .check_password(password_token, password.as_bytes())
                .await
                .map_err(|e| anyhow!("2FA password check failed: {:?}", e))?;
        }
        Err(e) => return Err(anyhow!("sign_in failed: {:?}", e)),
    }

    client
        .session()
        .save_to_file(&session_path)
        .context("Failed to save session file")?;

    println!("✅ Logged in. Session saved to {:?}", session_path);
    println!("Copy this file to your Railway volume mount at the same path.");

    Ok(())
}
