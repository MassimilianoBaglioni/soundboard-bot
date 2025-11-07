mod general;
mod soundboard;
mod spotify;

use dotenvy::dotenv;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::prelude::TypeMapKey;
use reqwest::Client as HttpClient;
use rspotify::{ClientCredsSpotify, Credentials};
use serenity::all::GuildId;
use songbird::{tracks::TrackQueue, SerenityInit};
use std::{collections::HashMap, env, fs::File, io::Cursor, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const AUDIO_PATH: &str = "./audio/";

struct Data {
    last_interaction: Arc<Mutex<Instant>>,
    soundboard_data: Vec<(String, String, String)>,
    tracks: Arc<Mutex<HashMap<GuildId, (TrackQueue, Vec<String>)>>>,
    spotify_client: ClientCredsSpotify,
    playlist_cancellation: Mutex<HashMap<GuildId, CancellationToken>>,
}

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

/// Sends the soundboard message
#[poise::command(slash_command, prefix_command)]
async fn soundboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    general::delete_messages(&ctx.serenity_context(), &ctx.channel_id()).await;
    ctx.say("Done").await?;

    let voice_channel_id = match general::get_user_voice_channel(
        &ctx.serenity_context(),
        &ctx.author().id,
        &ctx.guild_id().unwrap(),
    )
    .await
    {
        Some(id) => id,
        None => {
            ctx.say("Done").await?;
            return Ok(());
        }
    };

    soundboard::soundboard_handler(
        &ctx.serenity_context(),
        &ctx.channel_id(),
        ctx.guild_id().unwrap(),
        &voice_channel_id,
        &ctx.author().id,
        ctx.data(),
    )
    .await;
    Ok(())
}

/// Skips the current playing track
#[poise::command(slash_command, prefix_command)]
async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    general::skip_song(&ctx.guild_id().unwrap(), ctx.data()).await;
    ctx.say("Track skipped.").await?;
    Ok(())
}

/// Skips the current track and clears the queue
#[poise::command(slash_command, prefix_command)]
async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    // Since adding the songs takes a lot of time, if clear is called while songs are added, only the already loaded tracks are cleared, the other async function will keep adding.
    general::clear(&ctx.guild_id().unwrap(), ctx.data()).await;
    ctx.say("Cleared all queued songs.").await?;
    Ok(())
}

/// Pauses the current playing track
#[poise::command(slash_command, prefix_command)]
async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    general::pause_song(&ctx.guild_id().unwrap(), ctx.data()).await;
    ctx.say("Track paused.").await?;
    Ok(())
}

/// Resumes the current paused track
#[poise::command(slash_command, prefix_command)]
async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    general::resume_song(&ctx.guild_id().unwrap(), ctx.data()).await;
    ctx.say("Track resumed.").await?;
    Ok(())
}

/// Play a song from YouTube, provide URL or title
#[poise::command(slash_command, prefix_command)]
async fn play(
    ctx: Context<'_>,
    #[description = "Url or title"] title: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    general::play_songs(
        &ctx.serenity_context(),
        title,
        ctx.guild_id().unwrap(),
        ctx.channel_id(),
        &ctx.author().id,
        ctx.data(),
    )
    .await;

    ctx.say("Done").await?;
    Ok(())
}

/// Lists all the queued songs.
#[poise::command(slash_command, prefix_command)]
async fn list(ctx: Context<'_>) -> Result<(), Error> {
    const MAX_MSG_LEN: usize = 2000;
    ctx.defer().await?;

    let hmap = ctx.data().tracks.lock().await;

    let titles = match &hmap.get(&ctx.guild_id().unwrap()) {
        Some(tuple) => tuple.1.clone(),
        None => {
            let _ = ctx.say("No songs queued.").await?;
            return Ok(());
        }
    };

    let mut messages = Vec::<String>::new();

    let mut formatted_msg = String::from("");

    for (index, title) in titles.iter().enumerate() {
        let tmp_msg = if index == 0 {
            format!("`{:>2}.`*__ Now playing__:* **{}**\n", index, title)
        } else {
            format!("`{:>2}.`**{}**\n", index, title)
        };

        if formatted_msg.len() + tmp_msg.len() > MAX_MSG_LEN {
            messages.push(formatted_msg.clone());
            formatted_msg.clear();
        }
        formatted_msg.push_str(&tmp_msg);
    }

    messages.push(formatted_msg.clone());

    for message in messages {
        let _ = ctx.say(message).await?;
    }

    Ok(())
}

/// Seeks FORWARD in the currently playing track by the specified number of seconds.
#[poise::command(slash_command, prefix_command)]
async fn seek(
    ctx: Context<'_>,
    #[description = "Absolute position in seconds to seek to."] seconds: String,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    general::seek(
        &ctx.serenity_context(),
        &ctx.guild_id().unwrap(),
        &ctx.channel_id(),
        ctx.data(),
        seconds,
    )
    .await;
    ctx.say("Done").await?;
    Ok(())
}

async fn update_yt_dlp() {
    let (yt_dlp_filename, url) = if cfg!(target_os = "windows") {
        (
            "yt-dlp.exe",
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        )
    } else {
        (
            "yt-dlp",
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
        )
    };

    let response = reqwest::get(url)
        .await
        .expect("Failed to download last yt-dlp executable file.");

    let yt_dlp_bin_path = std::env::current_dir()
        .expect("Failed retrieving current dir in update_yt_dlp")
        .join(yt_dlp_filename);

    println!("Downloading from: {:?}", url);

    println!("To path: {:?}", yt_dlp_bin_path);

    let mut dest = File::create(yt_dlp_bin_path.clone())
        .expect("Failed to create the destination file for yt-dl;p executable");
    let mut content = Cursor::new(
        response
            .bytes()
            .await
            .expect("Fecthing yt-dlp bytes download failed."),
    );
    std::io::copy(&mut content, &mut dest).expect("Failed to write in the dest file.");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&yt_dlp_bin_path)
            .expect("Permissions get issue.")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&yt_dlp_bin_path, perms).expect("Permissions set issue.");
    }

    let old_path = env::var("PATH").unwrap_or_default();

    let path_sep = if cfg!(windows) { ";" } else { ":" };

    let new_path = format!(
        "{}{}{}",
        std::env::current_dir().expect("").to_str().expect(""),
        path_sep,
        old_path
    );
    env::set_var("PATH", new_path);

    println!("Done updating yt-dlp");
}

#[tokio::main]
async fn main() {
    update_yt_dlp().await;

    dotenv().ok();
    let token = env::var("DISCORD_BOT_TOKEN").expect("DISCORD_BOT_TOKEN err in .env");
    let spoty_cred = Credentials::new(
        env::var("SPOTIFY_ID")
            .expect("SPOTIFY_ID err in .env")
            .as_str(),
        env::var("SPOTIFY_TOKEN")
            .expect("SPOTIFY_TOKE err in .env")
            .as_str(),
    );
    let spotify_client = ClientCredsSpotify::new(spoty_cred);
    spotify_client.request_token().await.unwrap();

    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                play(),
                resume(),
                skip(),
                pause(),
                soundboard(),
                clear(),
                seek(),
                list(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    last_interaction: Arc::new(Mutex::new(Instant::now())),
                    soundboard_data: soundboard::get_soundboard_data(AUDIO_PATH)
                        .expect("Failed to load soundboard data"),
                    tracks: Arc::new(Mutex::new(HashMap::new())),
                    spotify_client,
                    playlist_cancellation: Mutex::new(HashMap::new()),
                })
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await;
    client.unwrap().start().await.unwrap();
}
