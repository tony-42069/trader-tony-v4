//! Telegram MTProto client for the call-out sniper.
//!
//! Owns a grammers `Client`, subscribes to one channel, and forwards new
//! message bodies into an mpsc channel. Reconnects automatically on disconnect.
//!
//! Session persistence: first-time login is performed by the `tg_login` bin.
//! Once the session file exists, this module reuses it without interaction.

use anyhow::{anyhow, Context, Result};
use grammers_client::{Client, Config, InitParams, Update};
use grammers_session::Session;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Wraps a grammers Client subscribed to one channel.
pub struct TelegramClient {
    client: Client,
    channel_handle: String,
}

impl TelegramClient {
    /// Connect using an existing session file. Returns an error if the session
    /// is missing or invalid — run the `tg_login` binary first.
    pub async fn connect(
        api_id: i32,
        api_hash: &str,
        session_path: &Path,
        channel_handle: &str,
    ) -> Result<Self> {
        if !session_path.exists() {
            return Err(anyhow!(
                "Telegram session file not found at {:?}. Run `cargo run --bin tg_login` first.",
                session_path
            ));
        }

        let session = Session::load_file(session_path)
            .with_context(|| format!("Failed to load TG session from {:?}", session_path))?;

        // Client::connect returns Result<Client, AuthorizationError>; map to anyhow.
        let client = Client::connect(Config {
            session,
            api_id,
            api_hash: api_hash.to_string(),
            params: InitParams {
                catch_up: false, // do NOT replay history on startup — only live messages
                ..Default::default()
            },
        })
        .await
        .map_err(|e| anyhow!("Failed to connect grammers client: {:?}", e))?;

        // is_authorized() returns Result<bool, InvocationError>; treat errors as not authorized.
        if !client.is_authorized().await.unwrap_or(false) {
            return Err(anyhow!(
                "Telegram session is not authorized. Re-run `cargo run --bin tg_login`."
            ));
        }

        info!("✅ Connected to Telegram as authorised user");

        Ok(Self {
            client,
            channel_handle: channel_handle.trim_start_matches('@').to_string(),
        })
    }

    /// Spawn the background listener task. Returns a receiver that yields
    /// message bodies as soon as they arrive in the configured channel.
    ///
    /// Consumes `self` and moves the client into the spawned task (the caller
    /// retains only the receiver end of the channel).
    ///
    /// The receiver buffer is 32; if the consumer can't keep up, the sender
    /// drops messages (with a warning) rather than block the TG event loop.
    pub fn spawn_listener(self) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel::<String>(32);
        let client = self.client;
        let channel_handle = self.channel_handle;

        tokio::spawn(async move {
            // Resolve channel handle to a chat once at startup.
            let chat = match client.resolve_username(&channel_handle).await {
                Ok(Some(chat)) => {
                    info!("📡 Resolved TG channel @{} -> id {}", channel_handle, chat.id());
                    chat
                }
                Ok(None) => {
                    error!(
                        "TG channel @{} not found — listener task exiting",
                        channel_handle
                    );
                    return;
                }
                Err(e) => {
                    error!(
                        "Failed to resolve TG channel @{}: {:?}",
                        channel_handle, e
                    );
                    return;
                }
            };
            let target_chat_id = chat.id();

            loop {
                // next_update returns Result<Update, InvocationError> (not Option).
                let update = match client.next_update().await {
                    Ok(update) => update,
                    Err(e) => {
                        error!("TG next_update error: {:?} — retrying in 2s", e);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                if let Update::NewMessage(msg) = update {
                    // Filter: only messages in our channel, not outgoing.
                    // msg.chat() returns Chat by value (Chat: Clone, not Copy).
                    if msg.chat().id() != target_chat_id {
                        continue;
                    }
                    // msg.outgoing() returns bool directly.
                    if msg.outgoing() {
                        continue;
                    }

                    // msg.text() returns &str; convert to owned String.
                    let text = msg.text().to_string();
                    if text.is_empty() {
                        continue;
                    }

                    match tx.try_send(text) {
                        Ok(_) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            warn!(
                                "TG message channel full — dropping message (consumer too slow)"
                            );
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            info!("TG message receiver closed — listener exiting");
                            return;
                        }
                    }
                }
            }
        });

        rx
    }
}
