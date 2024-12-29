use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::join_all;

use serenity::async_trait;
use serenity::all::{ChannelId, Context, GuildId, Http, UserId};
use serenity::builder::GetMessages;

use songbird::input::{Compose, File, YoutubeDl};
use songbird::tracks::TrackQueue;
use songbird::{Event, TrackEvent};
use songbird::{EventContext, EventHandler as VoiceEventHandler};

use tokio::time;

use crate::{Data, HttpKey};


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

    if let Some(queue) = guard.get(guild_id) {
        queue.stop();
    }

    if let Some(handler_lock) = manager.get(*guild_id) {
        let mut handler = handler_lock.lock().await;

        handler.stop();
    }
}

pub async fn play_song_yt(
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

    let do_search = !url.starts_with("http");
    join_channel(ctx, &guild_id, author_id, data).await;

    let http_client = {
        let data = ctx.data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        let mut src = if do_search {
            YoutubeDl::new_search(http_client, url)
        } else {
            YoutubeDl::new(http_client, url)
        };

        let metadata = src.aux_metadata().await;

        if let Ok(aux_metadata) = metadata {
            let title = aux_metadata.title.as_deref().unwrap_or("Unknown Title");
            let video_url = aux_metadata
                .source_url
                .as_deref()
                .unwrap_or("URL not available");

            let guard = data.tracks.lock().await;
            if let Some(queue) = guard.get(&guild_id) {
                if !queue.is_empty() {
                    let formatted_message =
                        format!("**Added to the queue:** [{}]({})", title, video_url);
                    if let Err(err) = msg_channel_id.say(&ctx.http, formatted_message).await {
                        eprintln!("Failed to send message: {}", err);
                    }
                }

                let track_handler = queue.add_source(src.clone().into(), &mut handler).await;

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
        } else {
            println!("Failed to fetch aux metadata: {:?}", metadata.unwrap_err());
        }
    } else {
        println!("Not in a channel");
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
