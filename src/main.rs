use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Mutex as tokioMutex;
use tokio::time;

use futures::future::join_all;
use futures::stream::{self};

use reqwest::Client as HttpClient;

use serenity::all::CreateAttachment;
use serenity::all::EditProfile;
use serenity::all::{
    ButtonStyle, ChannelId, CreateInteractionResponse, CreateMessage, Error, GuildId, Http,
};
use serenity::async_trait;
use serenity::builder::{CreateButton, GetMessages};
use serenity::futures::StreamExt;
use serenity::model::channel::Message;
use serenity::prelude::*;

use songbird::input::{Compose, File, YoutubeDl};
use songbird::tracks::TrackQueue;
use songbird::{Event, SerenityInit, TrackEvent};
use songbird::{EventContext, EventHandler as VoiceEventHandler};

const AUDIO_PATH: &str = "./audio/";

fn get_soundboard_data(location: &str) -> Result<Vec<(String, String, String)>, io::Error> {
    let path = Path::new(location);
    let mut result = Vec::<(String, String, String)>::new();

    if path.is_dir() {
        for (index, file) in fs::read_dir(path)?.enumerate() {
            let path = file?.path();

            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
                    if let Some(tmp_path) = Path::new(file_name)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                    {
                        result.push((
                            index.to_string(),
                            tmp_path.to_string(),
                            file_name.to_string(),
                        ));
                    };
                }
            }
        }
    }
    Ok(result)
}

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

struct Handler {
    last_interaction: Arc<Mutex<Instant>>,
    soundboard_data: Vec<(String, String, String)>,
    tracks: Arc<tokioMutex<HashMap<GuildId, TrackQueue>>>,
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

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!soundboard" {
            if let Some(channel_id) = self.get_channel_id(&ctx, &msg).await {
                self.delete_messages(&ctx, &msg.channel_id).await;

                if let Some(guild_id) = msg.guild_id {
                    self.soundboard_handler(&ctx, &msg.channel_id, guild_id, &channel_id)
                        .await;
                }
            } else {
                if let Err(_) = msg
                    .channel_id
                    .say(&ctx.http, "You must join a channel first")
                    .await
                {
                    println!("Error on sending message.");
                };
            }
        } else if msg.content.starts_with("!play ") {
            let mut song_name = msg.content.clone();
            song_name.drain(..6);
            if let Some(channel_id) = self.get_channel_id(&ctx, &msg).await {
                if let Some(guild_id) = msg.guild_id {
                    self.play_song_yt(&ctx, song_name, guild_id, channel_id, msg.channel_id)
                        .await;
                }
            } else {
                if let Err(_) = msg
                    .channel_id
                    .say(&ctx.http, "You must join a channel first")
                    .await
                {
                    println!("Error on sending message.");
                };
            }
        } else if msg.content.starts_with("!skip") {
            if let Some(_channel_id) = self.get_channel_id(&ctx, &msg).await {
                if let Some(guild_id) = msg.guild_id {
                    self.skip_song(&guild_id).await;
                }
            } else {
                if let Err(_) = msg
                    .channel_id
                    .say(&ctx.http, "You must join a channel first")
                    .await
                {
                    println!("Error on sending message.");
                };
            }
        } else if msg.content.starts_with("!pause") {
            if let Some(_channel_id) = self.get_channel_id(&ctx, &msg).await {
                if let Some(guild_id) = msg.guild_id {
                    self.pause_song(&guild_id).await;
                }
            } else {
                if let Err(_) = msg
                    .channel_id
                    .say(&ctx.http, "You must join a channel first")
                    .await
                {
                    println!("Error on sending message.");
                };
            }
        } else if msg.content.starts_with("!resume") {
            if let Some(_channel_id) = self.get_channel_id(&ctx, &msg).await {
                if let Some(guild_id) = msg.guild_id {
                    self.resume_song(&guild_id).await;
                }
            } else {
                if let Err(_) = msg
                    .channel_id
                    .say(&ctx.http, "You must join a channel first")
                    .await
                {
                    println!("Error on sending message.");
                };
            }
        }
    }
}

impl Handler {
    async fn update_avatar(&self, ctx: &Context) -> Result<(), serenity::Error> {
        let avatar = CreateAttachment::path("./avatar.png").await?;

        let mut current_user = ctx.cache.current_user().clone(); // Ensure it is cloned here to avoid borrowing issues.

        current_user
            .edit(&ctx.http, EditProfile::new().avatar(&avatar))
            .await?;

        Ok(())
    }

    async fn get_channel_id(&self, ctx: &Context, msg: &Message) -> Option<ChannelId> {
        if let Some(guild_id) = msg.guild_id {
            if let Some(guild) = guild_id.to_guild_cached(ctx) {
                return match guild.voice_states.get(&msg.author.id) {
                    Some(channel) => channel.channel_id,

                    None => None,
                };
            }
        };

        None
    }

    async fn play_song_yt(
        &self,
        ctx: &Context,
        url: String,
        guild_id: GuildId,
        channel_id: ChannelId,
        msg_channel_id: ChannelId,
    ) {
        self.tracks
            .lock()
            .await
            .entry(guild_id)
            .or_insert(TrackQueue::new());

        let do_search = !url.starts_with("http");
        self.join_channel(&ctx, &channel_id, &guild_id).await;

        let http_client = {
            let data = ctx.data.read().await;
            data.get::<HttpKey>()
                .cloned()
                .expect("Guaranteed to exist in the typemap.")
        };

        /*if let Err(e) = self.update_avatar(ctx).await {
            eprintln!("Failed to update avatar: {}", e);
        }*/

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

                let guard = self.tracks.lock().await;
                if let Some(queue) = guard.get(&guild_id) {
                    if queue.len() > 0 {
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
                            chan_id: msg_channel_id.clone(),
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

    async fn skip_song(&self, guild_id: &GuildId) {
        if let Some(track_handle) = self.tracks.lock().await.get(guild_id) {
            let _ = track_handle.skip();
        }
    }

    async fn pause_song(&self, guild_id: &GuildId) {
        if let Some(track_handle) = self.tracks.lock().await.get(guild_id) {
            let _ = track_handle.pause();
        }
    }

    async fn resume_song(&self, guild_id: &GuildId) {
        if let Some(track_handle) = self.tracks.lock().await.get(guild_id) {
            let _ = track_handle.resume();
        }
    }

    // Returns a list of messages.
    // We are not sending a single big message since Discord
    // does not allow to send messages with more than 25 buttons.
    // Here we send a list of messages with 25 buttons each.
    async fn send_soundboard_msg(
        &self,
        ctx: &Context,
        channel_id: &ChannelId,
    ) -> Vec<serenity::model::channel::Message> {
        let mut messages_vec = Vec::<serenity::model::channel::Message>::new();

        let mut msg = CreateMessage::new().content("Soundboard\n");

        for (id, button_label, _) in self
            .soundboard_data
            .iter()
            .map(|tuple| (tuple.0.as_str(), tuple.1.as_str(), tuple.2.as_str()))
        {
            let parsed_int = id.parse::<usize>().expect("error parsiung index");
            if parsed_int % 25 == 0 && parsed_int > 1 {
                messages_vec.push(channel_id.send_message(ctx, msg).await.unwrap());
                msg = CreateMessage::new();
            }

            msg = msg.button(CreateButton::new(id).label(button_label));
        }

        messages_vec.push(channel_id.send_message(ctx, msg).await.unwrap());
        msg = CreateMessage::new();

        msg = msg.button(
            CreateButton::new("stop")
                .label("STOP")
                .style(ButtonStyle::Danger),
        );
        msg = msg.button(
            CreateButton::new("quit")
                .label("QUIT")
                .style(ButtonStyle::Danger),
        );

        messages_vec.push(channel_id.send_message(ctx, msg).await.unwrap());
        messages_vec
    }

    async fn soundboard_handler(
        &self,
        ctx: &Context,
        msg_channel_id: &ChannelId,
        guild_id: GuildId,
        voice_channel_id: &ChannelId,
    ) {
        self.join_channel(&ctx, voice_channel_id, &guild_id).await;

        // Vector with streams of event for each button.
        let streams_vec: Vec<_> = self
            .send_soundboard_msg(ctx, msg_channel_id)
            .await
            .iter()
            .map(|message| message.await_component_interaction(&ctx.shard).stream())
            .collect();

        // Combine all the streams together in one single stream.
        let mut combined_stream = stream::select_all(streams_vec);

        // Listen the combined stream to get interactions.
        while let Some(interaction) = combined_stream.next().await {
            if let Some((_, _, found_path)) = self
                .soundboard_data
                .iter()
                .find(|&(item_id, _, _)| item_id == interaction.data.custom_id.as_str())
            {
                self.join_channel(&ctx, &voice_channel_id, &guild_id).await;
                let path = PathBuf::from(AUDIO_PATH.to_owned() + found_path);
                self.play_from_source(&ctx, &guild_id, path).await;

                let mut last_interaction = self.last_interaction.lock().unwrap();
                *last_interaction = Instant::now();
            } else if interaction.data.custom_id == "stop" {
                self.stop_reproduction(&ctx, &guild_id).await;
            } else if interaction.data.custom_id == "quit" {
                let manager = songbird::get(ctx)
                    .await
                    .expect("Songbird Voice client placed in at initialisation")
                    .clone();
                let _ = manager.leave(guild_id).await;
            }

            interaction
                .create_response(&ctx, CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();
        }

        self.delete_messages(&ctx, voice_channel_id).await;
    }

    async fn play_from_source(&self, ctx: &Context, guild_id: &GuildId, path: PathBuf) {
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

    async fn stop_reproduction(&self, ctx: &Context, guild_id: &GuildId) {
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird Voice client placed in at initialisation")
            .clone();

        if let Some(handler_lock) = manager.get(*guild_id) {
            let mut handler = handler_lock.lock().await;

            handler.stop();
        }
    }

    async fn join_channel(&self, ctx: &Context, channel_id: &ChannelId, guild_id: &GuildId) {
        let manager = songbird::get(ctx)
            .await
            .expect("Songbird Voice client placed in at initialisation.")
            .clone();

        match manager.join(*guild_id, *channel_id).await {
            Ok(_) => {
                {
                    let mut last_interaction = self.last_interaction.lock().unwrap();
                    *last_interaction = Instant::now();
                }
                self.start_inactivity_checker(ctx, guild_id).await;
            }
            Err(e) => {
                println!("Failed to join the channel: {:?}", e);
            }
        }
    }

    async fn delete_messages(&self, ctx: &Context, channel_id: &ChannelId) {
        let bot_user_id = ctx.cache.current_user().id;
        let builder = GetMessages::new().limit(50);

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

    async fn start_inactivity_checker(&self, ctx: &Context, guild_id: &GuildId) {
        let last_interaction = Arc::clone(&self.last_interaction);
        let tracks_hash_map = Arc::clone(&self.tracks);

        let manager = songbird::get(ctx)
            .await
            .expect("Songbird Voice client placed in at initialisation")
            .clone();
        let c_guild_id = guild_id.clone();

        tokio::spawn(async move {
            let timeout_duration = Duration::from_secs(15*60);
            let mut interval = time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;
                let last_interaction_time = *last_interaction.lock().unwrap();
                //Here we get the track handle for the current server.
                let guard = tracks_hash_map.lock().await;

                let mut queued_cnt = 0;

                if let Some(crt_queue) = guard.get(&c_guild_id){
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
}

#[tokio::main]
async fn main() {
    let token = "ODg4MDAyNzAxMjE5MjAxMDQ0.GqAMqt.SmwYDbnVdpK9VDr0-Z5dOpVC5Z_ZB4eg6kCEnw";

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            last_interaction: Arc::new(Mutex::new(Instant::now())),
            soundboard_data: get_soundboard_data(AUDIO_PATH)
                .expect("Failed to load soundboard data"),
            tracks: Arc::new(tokioMutex::new(HashMap::new())),
        })
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await
        .expect("Error creating the client");

    if let Err(why) = client.start().await {
        println!("Error on start {why:?}");
    }
}
