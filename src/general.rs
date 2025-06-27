use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use futures::{future::join_all, StreamExt};

use rspotify::{
    model::{PlayableItem, PlaylistId},
    prelude::BaseClient,
    ClientCredsSpotify,
};

use serenity::{
    all::{ChannelId, Context, GuildId, Http, UserId},
    async_trait,
    builder::GetMessages,
};

use songbird::{
    input::{Compose, File, YoutubeDl},
    tracks::TrackQueue,
    Event, EventContext, TrackEvent,
};

use songbird::EventHandler as VoiceEventHandler;

use tokio::time;

use serde_json::{self, Value};

use crate::{spotify, Data, HttpKey};

use tokio_util::sync::CancellationToken;

const PLAYLIST_LIMIT: Option<usize> = Some(200);

enum MultipleSongs {
    YtPlaylist(Vec<String>),
    SpotiPlaylist(Vec<String>),
    SpotiAlbum(Vec<String>),
}

struct SongStartNotifier {
    chan_id: ChannelId,
    http: Arc<Http>,
    title: String,
    video_url: String,
}

#[async_trait]
impl VoiceEventHandler for SongStartNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(_track_list) = ctx {
            let formatted_message =
                format!("**Now playing:** [{}]({})", self.title, self.video_url);
            if let Err(err) = self.chan_id.say(&self.http, formatted_message).await {
                eprintln!("Failed to send message: {}", err);
            }
        }

        None
    }
}

pub async fn delete_messages(ctx: &Context, channel_id: &ChannelId) {
    let bot_user_id = ctx.cache.current_user().id;
    let builder = GetMessages::new().limit(10);

    match channel_id.messages(&ctx.http, builder).await {
        Err(_e) => println!("Error on retireving messages"),
        Ok(result) => {
            let delete_futures: Vec<_> = result
                .iter()
                .filter(|data| data.author.id == bot_user_id)
                .map(|data| channel_id.delete_message(&ctx.http, data.id))
                .collect();

            let _ = join_all(delete_futures).await;
        }
    }
}

pub async fn join_channel(ctx: &Context, guild_id: &GuildId, author_id: &UserId, data: &Data) {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let voice_channel_id = match get_user_voice_channel(ctx, author_id, guild_id).await {
        Some(id) => id,
        None => return,
    };

    match manager.join(*guild_id, voice_channel_id).await {
        Ok(_) => {
            {
                let mut last_interaction = data.last_interaction.lock().await;
                //let mut last_interaction = data.last_interaction.lock().unwrap();
                *last_interaction = Instant::now();
            }
            start_inactivity_checker(ctx, guild_id, data).await;
        }
        Err(e) => {
            println!("Failed to join the channel: {:?}", e);
        }
    }
}

pub async fn get_user_voice_channel(
    ctx: &Context,
    user_id: &UserId,
    guild_id: &GuildId,
) -> Option<ChannelId> {
    let guild = guild_id.to_guild_cached(ctx).unwrap();

    if let Some(voice_state) = guild.voice_states.get(user_id) {
        if let Some(channel_id) = voice_state.channel_id {
            return Some(channel_id);
        } else {
            return None;
        }
    }

    None
}

async fn start_inactivity_checker(ctx: &Context, guild_id: &GuildId, data: &Data) {
    let last_interaction = Arc::clone(&data.last_interaction);
    let tracks_hash_map = Arc::clone(&data.tracks);

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation")
        .clone();
    let c_guild_id = *guild_id;

    tokio::spawn(async move {
        let timeout_duration = Duration::from_secs(15 * 60);
        let mut interval = time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;
            let last_interaction_time = *last_interaction.lock().await;
            //Here we get the track handle for the current server.
            let guard = tracks_hash_map.lock().await;

            let mut queued_cnt = 0;

            if let Some(crt_queue) = guard.get(&c_guild_id) {
                queued_cnt = crt_queue.len();
            };

            //If no song is playing and the interaction time is over, quit.
            if last_interaction_time.elapsed() >= timeout_duration && queued_cnt == 0 {
                let _ = manager.leave(c_guild_id).await;
                break;
            }
        }
    });
}

pub async fn play_from_source(ctx: &Context, guild_id: &GuildId, path: PathBuf) {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation")
        .clone();

    if let Some(handler_lock) = manager.get(*guild_id) {
        let mut handler = handler_lock.lock().await;

        let source = File::new(path);
        let _ = handler.play_input(source.into());
    } else {
        println!("No handler dayum");
    }
}

pub async fn stop_reproduction(ctx: &Context, guild_id: &GuildId, data: &Data) {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation")
        .clone();

    let guard = data.tracks.lock().await;
    if let Some(token) = data.playlist_cancellation.lock().await.remove(guild_id) {
        token.cancel();
    }
    if let Some(queue) = guard.get(guild_id) {
        queue.stop();
    }

    if let Some(handler_lock) = manager.get(*guild_id) {
        let mut handler = handler_lock.lock().await;

        handler.stop();
    }
}

pub async fn play_songs(
    ctx: &Context,
    url: String,
    guild_id: GuildId,
    msg_channel_id: ChannelId,
    author_id: &UserId,
    data: &Data,
) {
    data.tracks
        .lock()
        .await
        .entry(guild_id)
        .or_insert(TrackQueue::new());

    join_channel(ctx, &guild_id, author_id, data).await;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if let Some(_handler_lock) = manager.get(guild_id) {
        handle_song_request(ctx, url, data, msg_channel_id, &guild_id).await;
    } else {
        println!("Not in a channel");
    }
}

pub async fn handle_song_request(
    ctx: &Context,
    url: String,
    data: &Data,
    msg_channel_id: ChannelId,
    guild_id: &GuildId,
) {
    let http_client = {
        let data = ctx.data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    // Check if this is a playlist first
    match get_multiple_songs(data, url.clone()).await {
        Some(playlist) => {
            // Handle playlist
            let (tracks, _is_youtube) = match playlist {
                MultipleSongs::YtPlaylist(tracks) => (tracks, true),
                MultipleSongs::SpotiAlbum(tracks) | MultipleSongs::SpotiPlaylist(tracks) => {
                    (tracks, false)
                }
            };

            println!("Playlist handling");

            let token = CancellationToken::new();

            {
                // Add the cancellation token for the current Guild
                let cancels = data.playlist_cancellation.lock();
                cancels.await.insert(*guild_id, token.clone());
            }

            send_message(
                &msg_channel_id,
                ctx,
                format!("**Enqueuing:** [{}] songs.", tracks.len()),
            )
            .await;

            // Process each track in the playlist
            for track_url in tracks {
                if token.is_cancelled() {
                    break;
                }
                process_single_track(
                    ctx,
                    track_url,
                    data,
                    msg_channel_id,
                    guild_id,
                    &http_client,
                    true,
                    true,
                    Some(&token),
                )
                .await;
            }
        }
        None => {
            // Handle single song
            process_single_track(
                ctx,
                url.clone(),
                data,
                msg_channel_id,
                guild_id,
                &http_client,
                !url.starts_with("http"),
                false,
                None,
            )
            .await;
        }
    }
}

async fn process_single_track(
    ctx: &Context,
    url: String,
    data: &Data,
    msg_channel_id: ChannelId,
    guild_id: &GuildId,
    http_client: &reqwest::Client,
    do_search: bool,
    is_playlist: bool,
    token: Option<&CancellationToken>,
) {
    let mut searching = do_search;
    let url = match spotify::get_spoti_track_id(&url) {
        Some(track_id) => {
            searching = true;
            spotify::get_spoti_track_title(&track_id.to_string(), data).await
        }
        None => url,
    };

    let mut src = if searching {
        YoutubeDl::new_search(http_client.clone(), url)
    } else {
        YoutubeDl::new(http_client.clone(), url)
    };

    let metadata = src.aux_metadata().await;

    if let Ok(aux_metadata) = metadata {
        let title = aux_metadata.title.as_deref().unwrap_or("Unknown Title");
        let video_url = aux_metadata
            .source_url
            .as_deref()
            .unwrap_or("URL not available");

        let guard = data.tracks.lock().await;

        if let Some(token) = token {
            if token.is_cancelled() {
                println!("Track addition cancelled.");
                return;
            }
        }

        if let Some(queue) = guard.get(&guild_id) {
            if !is_playlist && !queue.is_empty() {
                send_message(
                    &msg_channel_id,
                    ctx,
                    format!("**Added to the queue:** [{}]({})", title, video_url),
                )
                .await;
            }
            let manager = songbird::get(ctx)
                .await
                .expect("Songbird Voice client placed in at initialisation")
                .clone();

            if let Some(handler_lock) = manager.get(*guild_id) {
                let mut handler = handler_lock.lock().await;
                let track_handler = queue.add_source(src.clone().into(), &mut *handler).await;
                let send_http = ctx.http.clone();

                let _ = track_handler.add_event(
                    Event::Track(TrackEvent::Play),
                    SongStartNotifier {
                        chan_id: msg_channel_id,
                        http: send_http,
                        title: title.to_string(),
                        video_url: video_url.to_string(),
                    },
                );
            }
        }
    } else {
        println!("Failed to fetch aux metadata: {:?}", metadata.unwrap_err());
    }
}

pub async fn clear(guild_id: &GuildId, data: &Data) {
    let guard = data.tracks.lock().await;
    if let Some(queue) = guard.get(&guild_id) {
        if let Some(token) = data.playlist_cancellation.lock().await.remove(guild_id) {
            token.cancel();
        }
        queue.stop();
    }
}

pub async fn skip_song(guild_id: &GuildId, data: &Data) {
    if let Some(track_handle) = data.tracks.lock().await.get(guild_id) {
        let _ = track_handle.skip();
    }
}

pub async fn pause_song(guild_id: &GuildId, data: &Data) {
    if let Some(track_handle) = data.tracks.lock().await.get(guild_id) {
        let _ = track_handle.pause();
    }
}

pub async fn resume_song(guild_id: &GuildId, data: &Data) {
    if let Some(track_handle) = data.tracks.lock().await.get(guild_id) {
        let _ = track_handle.resume();
    }
}

async fn get_urls_playlist(
    url: String,
    limit: Option<usize>,
    spotify_client: &ClientCredsSpotify,
) -> Vec<String> {
    // url as a name of the parameter is mis leading, in this case it is parsed as an id already!
    match PlaylistId::from_id(&url) {
        Ok(playlist_id) => {
            println!("Spotify playlist found: {:?}", playlist_id);
            let mut urls = Vec::new();
            let mut items = spotify_client.playlist_items(playlist_id, None, None);

            while let Some(url) = items.next().await {
                match url {
                    Ok(playlist_item) => {
                        if let Some(PlayableItem::Track(track)) = playlist_item.track {
                            urls.push(track.name + " " + &track.artists[0].name);
                        }
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                }
            }

            urls
        }
        Err(e) => {
            println!("Error on parsing spotify: {:?}", e);
            let mut result = Vec::<String>::new();

            let mut cmd = Command::new("yt-dlp");
            cmd.arg("-j").arg("--flat-playlist").arg(&url);

            // Add yt-dlp limit if specified
            if let Some(max_items) = limit {
                cmd.arg("--playlist-end").arg(max_items.to_string());
            }

            let output = cmd
                .stdout(Stdio::piped())
                .output()
                .expect("Failed to execute yt-dlp command");

            let output_str = std::str::from_utf8(&output.stdout).expect("Invalid UTF-8 output");

            for line in output_str.lines() {
                // Double-check limit in case yt-dlp didn't respect it
                if let Some(max_items) = limit {
                    if result.len() >= max_items {
                        break;
                    }
                }

                // Parse each line as a JSON object
                let video_info: Value =
                    serde_json::from_str(line).expect("Failed to parse JSON line");

                // Extract the 'url' field from the JSON object
                if let Some(url) = video_info.get("url").and_then(|u| u.as_str()) {
                    result.push(url.to_string());
                } else {
                    eprintln!("No URL found in video entry");
                }
            }

            result
        }
    }
}

async fn get_multiple_songs(data: &Data, url: String) -> Option<MultipleSongs> {
    if url.contains("list=") {
        Some(MultipleSongs::YtPlaylist(
            get_urls_playlist(url.clone(), PLAYLIST_LIMIT, &data.spotify_client).await,
        ))
    } else if let Some(spoty_id) = spotify::get_spoti_playlist_id(&url) {
        Some(MultipleSongs::SpotiPlaylist(
            get_urls_playlist(spoty_id.to_string(), PLAYLIST_LIMIT, &data.spotify_client).await,
        ))
    } else if let Some(album_id) = spotify::get_spoti_album_id(&url) {
        Some(MultipleSongs::SpotiAlbum(
            (spotify::get_urls_album(album_id.to_string(), &data.spotify_client)).await,
        ))
    } else {
        None
    }
}

async fn send_message(msg_channel_id: &ChannelId, ctx: &Context, formatted_message: String) {
    if let Err(err) = msg_channel_id.say(&ctx.http, formatted_message).await {
        eprintln!("Failed to send message: {}", err);
    }
}
