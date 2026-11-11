use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mocp",
    author = "Damian Pietras / Rust rewrite",
    version = "0.1.0",
    about = "Music On Console Player - Rust Rewrite",
    long_about = "A console audio player for Linux/Unix designed to be powerful and easy to use."
)]
pub struct CliArgs {
    /// Start in MusicDir or use specified music library directory
    #[arg(short = 'm', long = "music-dir")]
    pub music_dir: Option<PathBuf>,

    /// Scan music library and update cache
    #[arg(long = "scan-library")]
    pub scan_library: bool,

    /// Start in server mode (daemonized or foreground)
    #[arg(short = 'S', long = "server")]
    pub server: bool,

    /// Run the server in foreground
    #[arg(short = 'F', long = "foreground")]
    pub foreground: bool,

    /// Start playing from the playlist
    #[arg(short = 'p', long = "play")]
    pub play: bool,

    /// Pause playback
    #[arg(short = 'P', long = "pause")]
    pub pause: bool,

    /// Unpause playback
    #[arg(short = 'U', long = "unpause")]
    pub unpause: bool,

    /// Toggle pause / play
    #[arg(short = 'G', long = "toggle-pause")]
    pub toggle_pause: bool,

    /// Stop playback
    #[arg(short = 's', long = "stop")]
    pub stop: bool,

    /// Play next song
    #[arg(short = 'f', long = "next")]
    pub next: bool,

    /// Play previous song
    #[arg(short = 'r', long = "previous")]
    pub previous: bool,

    /// Seek by N seconds (can be negative)
    #[arg(short = 'k', long = "seek", allow_hyphen_values = true)]
    pub seek: Option<i64>,

    /// Jump to position in current track (e.g. 50% or 120s)
    #[arg(short = 'j', long = "jump")]
    pub jump: Option<String>,

    /// Adjust volume (e.g. +5, -5, or 80)
    #[arg(short = 'v', long = "volume", allow_hyphen_values = true)]
    pub volume: Option<String>,

    /// Append files/directories to playlist
    #[arg(short = 'a', long = "append")]
    pub append: bool,

    /// Add files to queue
    #[arg(short = 'q', long = "enqueue")]
    pub enqueue: bool,

    /// Clear playlist
    #[arg(short = 'c', long = "clear")]
    pub clear: bool,

    /// Shutdown the server
    #[arg(short = 'x', long = "exit")]
    pub exit: bool,

    /// Print information about currently playing song
    #[arg(short = 'i', long = "info")]
    pub info: bool,

    /// Print formatted information about currently playing song
    #[arg(short = 'Q', long = "format")]
    pub format: Option<String>,

    /// Toggle a control (shuffle, repeat, autonext)
    #[arg(short = 't', long = "toggle")]
    pub toggle: Option<String>,

    /// Turn on a control (shuffle, repeat, autonext)
    #[arg(short = 'o', long = "on")]
    pub on: Option<String>,

    /// Turn off a control (shuffle, repeat, autonext)
    #[arg(short = 'u', long = "off")]
    pub off: Option<String>,

    /// Files or directories to play or append
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,
}
