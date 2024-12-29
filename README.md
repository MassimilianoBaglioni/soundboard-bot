
# Discord Music & Soundboard Bot

A Discord bot built in Rust that can play music from YouTube, control playback, and create a soundboard with custom audio files. This bot supports both playing music from direct YouTube links or by searching song titles. Additionally, users can create a soundboard by adding audio files to a specific folder and using buttons to play sounds.

## Features

- **Music Playback:**
  - Play music from YouTube using either a direct link or a song title.
  - Commands for controlling playback:
    - `!pause`: Pauses the music.
    - `!resume`: Resumes the paused music.
    - `!skip`: Skips the current song.
    - `!enqueue <song>`: Adds a song to the queue.

- **Soundboard:**
  - Users can add MP3 files to the `audio` folder.
  - The bot generates buttons for each audio file, with button labels based on the file names.
  - The `!soundboard` command sends a message with all the buttons. When clicked, the bot joins the user's voice channel and plays the corresponding sound.
  - The bot will also delete its last 10 messages before sending the soundboard buttons.

## Requirements

To run this bot, you will need:

- **Rust** (for building and running the project)
- **yt-dlp** (for downloading music from YouTube)
- **A Discord bot token** (stored in a `.env` file)

### 1. Install Rust

If you don't have Rust installed on your machine, you can install it by following these steps:

1. Download and install **Rust** using the [official guide](https://www.rust-lang.org/tools/install).
2. Verify the installation by running the following command in your terminal:

   ```bash
   rustc --version
   ```

### 2. Install yt-dlp

yt-dlp is a powerful tool used to download videos and music from YouTube.

- On **Linux** (Ubuntu/Debian), you can install it via `apt`:

  ```bash
  sudo apt install yt-dlp
  ```

- On **macOS**, use `brew`:

  ```bash
  brew install yt-dlp
  ```

- On **Windows**, you can download the latest release from the [yt-dlp GitHub releases](https://github.com/yt-dlp/yt-dlp/releases).

Once yt-dlp is installed, verify it by running:

```bash
yt-dlp --version
```

### 3. Set Up the Project

1. Clone the repository:

   ```bash
   git clone https://github.com/yourusername/yourbotname.git
   cd yourbotname
   ```

2. Create a `.env` file inside the `src` folder and add your Discord bot token like this:

   ```bash
   DISCORD_TOKEN=your_discord_bot_token
   ```

   Replace `your_discord_bot_token` with the actual token for your bot.

3. Install the necessary Rust dependencies:

   ```bash
   cargo build
   ```

4. Run the bot:

   ```bash
   cargo run
   ```

   The bot should now be running and available on your Discord server.

## Usage

### Music Commands

- `!play <song title or YouTube link>`: Play a song from YouTube.
- `!pause`: Pause the currently playing music.
- `!resume`: Resume the paused music.
- `!skip`: Skip the current song.
- `!enqueue <song title or YouTube link>`: Add a song to the queue.

### Soundboard Command

- `!soundboard`: Send a message with buttons corresponding to the audio files in the `audio` folder.
  - Each button will play the associated sound when clicked.
  - The bot will join the voice channel of the user who clicked the button and play the sound.

### Adding Audio to the Soundboard

1. Place your `.mp3` files inside the `audio` folder.
2. When you use the `!soundboard` command, the bot will automatically create buttons for each audio file.

### Example:

```plaintext
!soundboard
```

This will display buttons for all the `.mp3` files in the `audio` folder.

## Contributing

Feel free to fork this repository, submit issues, and contribute improvements! Make sure to follow the project’s licensing terms.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
