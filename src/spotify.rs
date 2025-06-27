use rspotify::{
    model::{AlbumId, TrackId},
    prelude::BaseClient,
    ClientCredsSpotify,
};

use crate::Data;

pub async fn get_urls_album(
    passed_album_id: String,
    spotify_client: &ClientCredsSpotify,
) -> Vec<String> {
    match AlbumId::from_id(&passed_album_id) {
        Ok(album_id) => {
            let mut urls = Vec::new();
            let items = spotify_client.album(album_id, None);

            for track in items.await.expect("msg").tracks.items {
                urls.push(track.name + " " + &track.artists[0].name);
            }

            urls
        }
        Err(e) => {
            println!("Error on parsing spotify album: {:?}", e);
            Vec::new()
        }
    }
}

pub fn get_spoti_playlist_id(url: &str) -> Option<&str> {
    url.split("/playlist/").nth(1)?.split('?').next()
}

pub fn get_spoti_album_id(url: &str) -> Option<&str> {
    url.split("/album/").nth(1)?.split('?').next()
}

pub fn get_spoti_track_id(url: &str) -> Option<&str> {
    url.split("/track/").nth(1)?.split('?').next()
}

pub async fn get_spoti_track_title(track_id: &str, data: &Data) -> String {
    let spoti_client = &data.spotify_client;
    let parsed_id =
        TrackId::from_id(track_id).expect("Could not convertid on single track spotify");

    let track = spoti_client
        .track(parsed_id, None)
        .await
        .expect("Error on get_spoti_track_title");

    track.name + " " + &track.artists[0].name
}
