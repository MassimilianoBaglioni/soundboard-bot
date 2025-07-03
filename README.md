# Discord Music & Soundboard Bot

A Discord bot built in Rust that can play music from YouTube and Spotify links, control playback, and create a soundboard with custom audio files. The bot supports both direct YouTube links and searching song titles, as well as Spotify albums, playlists, and single tracks (by searching YouTube for the audio). Users can also create a soundboard by adding audio files to a specific folder and using interactive buttons to play sounds.

---

## Features

- **Music Playback:**
  - Play music from YouTube using a direct link, playlist, or by searching a song title.
  - Play music from Spotify public playlists, albums, and single tracks (the bot searches YouTube for the corresponding audio; it does **not** play music directly from Spotify).
  - Commands for controlling playback:
    - `/play <song or link>`: Play a song or playlist from YouTube or Spotify.
    - `/pause`: Pauses the music.
    - `/resume`: Resumes the paused music.
    - `/skip`: Skips the current song.
    - `/clear`: Clears the queue and stops playback.
    - `/seek <seconds>`: Seek forward in the current track (backward seeking is not supported).
- **Soundboard:**
  - Users can add MP3 files to the `audio` folder.
  - The bot generates buttons for each audio file, with button labels based on the file names.
  - The `/soundboard` command sends a message with all the buttons. When clicked, the bot joins the user's voice channel and plays the corresponding sound.
  - The bot will also delete its previous soundboard messages before sending new soundboard buttons.
  - **Note:** Soundboard buttons must be refreshed between different executions of the bot, as Discord invalidates old buttons.
  - The bot joins the same voice channel as the user who used the `/soundboard` command.

---

## Requirements

- **Rust** (for building and running the project)
- **yt-dlp** (for downloading music from YouTube)
- **Python** (yt-dlp requires Python to be installed)
- **A Discord bot token and Spotify API credentials** (stored in a `.env` file)

### 1. Install Rust

If you don't have Rust installed, follow the [official guide](https://www.rust-lang.org/tools/install):

```bash
rustc --version
```

### 2. Install Python

yt-dlp requires Python (3.7+).  
Install Python from [python.org](https://www.python.org/downloads/) or use your system package manager:

- On **Ubuntu/Debian**:
  ```bash
  sudo apt install python3
  ```
- On **macOS** (with Homebrew):
  ```bash
  brew install python
  ```
- On **Windows**: Download and install from [python.org](https://www.python.org/downloads/).

Verify installation:
```bash
python3 --version
```

### 3. Install yt-dlp

- On **Linux** (Ubuntu/Debian):

  ```bash
  sudo apt install yt-dlp
  ```

- On **macOS**:

  ```bash
  brew install yt-dlp
  ```

- On **Windows**: Download from the [yt-dlp GitHub releases](https://github.com/yt-dlp/yt-dlp/releases).

Verify installation:

```bash
yt-dlp --version
```

### 4. Set Up the Project

1. Clone the repository:

   ```bash
   git clone https://github.com/yourusername/yourbotname.git
   cd yourbotname
   ```

2. Create a `.env` file in the **root folder** with the following content:

   ```env
   DISCORD_BOT_TOKEN = your_discord_bot_token
   SPOTIFY_ID = your_spotify_client_id
   SPOTIFY_TOKEN = your_spotify_client_secret
   ```

   Replace the values with your actual credentials.

3. Build and run the bot:

   ```bash
   cargo build
   cargo run
   ```

---

## Usage

### Music Commands (Slash Commands)

- `/play <song title, YouTube link, or Spotify link>`: Play a song, playlist, or album.
- `/pause`: Pause the currently playing music.
- `/resume`: Resume the paused music.
- `/skip`: Skip the current song.
- `/clear`: Clear the queue and stop playback.
- `/seek <seconds>`: Seek a point forward in the current track expressed in seconds. Seeking backwards will stop the bot from playing.

### Soundboard Command

- `/soundboard`: Send a message with buttons for each audio file in the `audio` folder.
  - Each button will play the associated sound when clicked.
  - The bot will join the voice channel of the user who clicked the button and play the sound.
  - Old soundboard messages are deleted automatically. If the bot goes offline and returns online, the last soundboard message will persist, but it won't be usable and the soundboard command should be used again. 

### Adding Audio to the Soundboard

1. Place your `.mp3` files (all audio format accepted by Songbird are supported) inside the `audio` folder.
2. Use the `/soundboard` command to generate buttons for each audio file.

---

## Important Notes

- **Spotify support:** The bot does **not** play music directly from Spotify. Instead, it searches YouTube for the corresponding track, album, or playlist and plays the result.
- **Soundboard buttons:** Must be refreshed between bot restarts (use `/soundboard` again after restarting).
- **Slash commands:** All commands are available as Discord slash commands. Make sure your bot has the necessary permissions.
- **Voice channel:** The bot will join the same voice channel as the user who invokes the `/soundboard` command.
- **Old soundboard messages:** The bot will automatically delete its previous soundboard messages when `/soundboard` is used again.

---

## Troubleshooting

- If the bot does not respond, ensure it has the correct permissions and that slash commands are registered.
- If playback does not work for Spotify links, ensure your Spotify credentials are correct and the tracks are public.
- If you encounter issues with soundboard buttons, refresh them by running the `/soundboard` command again.

---

## License

MIT

---

## Credits

- [Songbird](https://github.com/serenity-rs/songbird) for voice and audio playback.
- [rspotify](https://github.com/ramsayleung/rspotify) for Spotify
