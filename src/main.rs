use futures::future::join_all;
use serenity::all::ChannelId;
use serenity::all::CreateInteractionResponse;
use serenity::all::CreateMessage;
use serenity::all::GuildId;
use serenity::async_trait;
use serenity::builder::CreateButton;
use serenity::builder::GetMessages;
use serenity::futures::StreamExt;
use serenity::model::channel::Message;
use serenity::prelude::*;
use songbird::input::File;
use songbird::SerenityInit;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time;

/*TODO
 * 2) Fix random blocking (navie solution is calling the delete and recreate buttons
 *    after a certain number of failed interactions).
 * 3) Add button to make it quit.
 * */

const SOUNDBOARD: [(&str, &str, &str); 21] = [
    ("1", "🏴DVCE", "duce.mp3"),
    ("2", "🍊Swiggity Swag", "file.mp3"),
    ("3", "💩Negro", "negro.mp3"),
    ("4", "😵Stacci Dentro", "stacci_dentro.mp3"),
    ("5", "💀Im gon shoot you", "young_metro.mp3"),
    ("6", " Sette", "sette.mp3"),
    ("7", "3 piotte", "scarpe.mp3"),
    ("8", "Muzuu oh", "mado.mp3"),
    ("9", "Frah guarda che...", "visto_che.mp3"),
    ("10", "Lolicon janai", "feministo.mp3"),
    ("11", "Schiaffi", "schiaffi.mp3"),
    ("12", "Regolare", "regolare.mp3"),
    ("13", "Pieffo", "pieffo.mp3"),
    ("14", "Futuro", "futuro.mp3"),
    ("15", "Presente", "presente.mp3"),
    ("16", "ullachi", "ullachi.mp3"),
    ("17", "smesh", "smesh.mp3"),
    ("18", "cervello fruh", "cervello.mp3"),
    ("19", "Negroooo", "negro_tiz.mp3"),
    ("20", "It's quandale dingle here", "dingle.mp3"),
    ("21", "Ti odio", "odio.mp3"),
];

const AUDIO_PATH: &str = "./audio/";

struct Handler {
    last_interaction: Arc<Mutex<Instant>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self , ctx: Context, msg: Message) {
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
        }
    }
}

impl Handler {
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

    async fn send_soundboard_msg(
        &self,
        ctx: &Context,
        channel_id: &ChannelId,
    ) -> serenity::model::channel::Message {
        let mut msg = CreateMessage::new().content("Soundboard\n");

        for (id, button_label, _) in &SOUNDBOARD {
            msg = msg.button(CreateButton::new(*id).label(*button_label));
        }

        channel_id.send_message(ctx, msg).await.unwrap()
    }

    async fn soundboard_handler(
        &self,
        ctx: &Context,
        msg_channel_id: &ChannelId,
        guild_id: GuildId,
        voice_channel_id: &ChannelId,
    ) {
        self.join_channel(&ctx, voice_channel_id, &guild_id).await;

        let msg = self.send_soundboard_msg(ctx, msg_channel_id).await;

        let mut interaction_stream = msg.await_component_interaction(&ctx.shard).stream();


        while let Some(interaction) = interaction_stream.next().await {
            if let Some((_, _, found_path)) = SOUNDBOARD
                .iter()
                .find(|&&(item_id, _, _)| item_id == interaction.data.custom_id.as_str())
            {
                self.join_channel(&ctx, &voice_channel_id, &guild_id).await;
                let path = PathBuf::from(AUDIO_PATH.to_owned() + found_path);
                self.play_from_source(&ctx, &guild_id, path).await;

                let mut last_interaction = self.last_interaction.lock().unwrap();
                *last_interaction = Instant::now();
                println!("{:?}", *self.last_interaction);
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
        println!("Starting the thread");
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
                if last_interaction_time.elapsed() >= timeout_duration {
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
        })
        .register_songbird()
        .await
        .expect("Error creating the client");

    if let Err(why) = client.start().await {
        println!("Error on start {why:?}");
    }
}
