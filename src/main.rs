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
use std::time::Duration;


/*TODO 
 * 1) Afk disconnection.
 * 2) Fix random blocking (navie solution is calling the delete and recreate buttons
 *    after a certain number of failed interactions).
 * 3) Add button to make it quit.
 * 4) Join a channel when a valid button is clicked and the bot is not in the channel. 
*/

const SOUNDBOARD: [(&str, &str, &str);5] = [
    ("1", "🏴DVCE", "duce.mp3"),
    ("2", "🍊Swiggity Swag", "file.mp3"),
    ("3", "💩Negro", "negro.mp3"),
    ("4", "😵Stacci Dentro", "stacci_dentro.mp3"),
    ("5", "💀Im gon shoot you", "young_metro.mp3"),
];

const AUDIO_PATH: &str = "./audio/";

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!soundboard" {
            if let Some(channel_id) = get_channel_id(&ctx, &msg).await {
                delete_messages(&ctx, &msg.channel_id).await;

                if let Some(guild_id) = msg.guild_id {
                    soundboard_handler(&ctx, &msg.channel_id, guild_id, &channel_id).await;
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

async fn get_channel_id(ctx: &Context, msg: &Message) -> Option<ChannelId> {
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
    ctx: &Context,
    channel_id: &ChannelId,
) -> serenity::model::channel::Message {

    let mut msg = CreateMessage::new().content("Soundboard\n");

    for (id, button_label, _ ) in &SOUNDBOARD {
       msg = msg.button(CreateButton::new(*id).label(*button_label));
    }

    channel_id.send_message(ctx, msg).await.unwrap()
}

//TODO the timeout in the stream is the issue, create an handler
//     for when the timeout expires and make the bot quit, when
//     an interaction is received the timeout should be reset.
async fn soundboard_handler(
    ctx: &Context,
    msg_channel_id: &ChannelId,
    guild_id: GuildId,
    voice_channel_id: &ChannelId,
) {
    join_channel(&ctx, voice_channel_id, &guild_id).await;

    let msg = send_soundboard_msg(ctx, msg_channel_id).await;

    let mut interaction_stream = msg
        .await_component_interaction(&ctx.shard)
        .timeout(Duration::from_secs(60*15))
        .stream();

    while let Some(interaction) = interaction_stream.next().await {

        if let Some((_, _, found_path)) = SOUNDBOARD.iter().find(|&&(item_id, _, _)| item_id == interaction.data.custom_id.as_str()) {

            let path = PathBuf::from(AUDIO_PATH.to_owned() + found_path);
            play_from_source(&ctx, &guild_id, path).await;
        }
        interaction
            .create_response(&ctx, CreateInteractionResponse::Acknowledge)
            .await
            .unwrap();
    }

    println!("Intearction is none i guess");
}

async fn play_from_source(ctx: &Context, guild_id: &GuildId, path: PathBuf) {
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

async fn join_channel(ctx: &Context, channel_id: &ChannelId, guild_id: &GuildId) {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    match manager.join(*guild_id, *channel_id).await {
        Ok(_) => {}
        Err(e) => {
            println!("Failed to join the channel: {:?}", e);
        }
    }

}

async fn delete_messages(ctx: &Context, channel_id: &ChannelId) {
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

#[tokio::main]
async fn main() {
    let token = "ODg4MDAyNzAxMjE5MjAxMDQ0.GqAMqt.SmwYDbnVdpK9VDr0-Z5dOpVC5Z_ZB4eg6kCEnw";

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .register_songbird()
        .await
        .expect("Error creating the client");

    if let Err(why) = client.start().await {
        println!("Error on start {why:?}");
    }
}
