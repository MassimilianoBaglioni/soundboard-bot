use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use futures::stream;
use serenity::all::{ButtonStyle, ChannelId, Context, CreateInteractionResponse, CreateMessage, GuildId, UserId};
use serenity::builder::CreateButton;
use serenity::futures::StreamExt;

use crate::{general, Data, AUDIO_PATH};


pub fn get_soundboard_data(location: &str) -> Result<Vec<(String, String, String)>, io::Error> {
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

pub async fn soundboard_handler(
    ctx: &Context,
    msg_channel_id: &ChannelId,
    guild_id: GuildId,
    voice_channel_id: &ChannelId,
    author_id: &UserId,
    data: &Data,
) {
    general::join_channel(ctx, &guild_id, author_id, data).await;
    // Vector with streams of event for each button.
    let streams_vec: Vec<_> = send_soundboard_msg(ctx, msg_channel_id, data)
        .await
        .iter()
        .map(|message| message.await_component_interaction(&ctx.shard).stream())
        .collect();

    // Combine all the streams together in one single stream.
    let mut combined_stream = stream::select_all(streams_vec);

    // Listen the combined stream to get interactions.
    while let Some(interaction) = combined_stream.next().await {
        if let Some((_, _, found_path)) = data
            .soundboard_data
            .iter()
            .find(|&(item_id, _, _)| item_id == interaction.data.custom_id.as_str())
        {
            general::join_channel(ctx, &guild_id, author_id, data).await;
            let path = PathBuf::from(AUDIO_PATH.to_owned() + found_path);
            general::play_from_source(ctx, &guild_id, path).await;

            let mut last_interaction = data.last_interaction.lock().await;
            *last_interaction = Instant::now();
        } else if interaction.data.custom_id == "stop" {
            general::stop_reproduction(ctx, &guild_id, data).await;
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

    general::delete_messages(ctx, voice_channel_id).await;
}

// Returns a list of messages.
// We are not sending a single big message since Discord
// does not allow to send messages with more than 25 buttons.
// Here we send a list of messages with 25 buttons each.
pub async fn send_soundboard_msg(
    ctx: &Context,
    channel_id: &ChannelId,
    data: &Data,
) -> Vec<serenity::model::channel::Message> {
    let mut messages_vec = Vec::<serenity::model::channel::Message>::new();

    let mut msg = CreateMessage::new().content("Soundboard\n");

    for (id, button_label, _) in data
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
