use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackState {
    Play,
    Pause,
    Stop,
}

impl std::fmt::Display for PlaybackState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackState::Play => write!(f, "PLAY"),
            PlaybackState::Pause => write!(f, "PAUSE"),
            PlaybackState::Stop => write!(f, "STOP"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongInfo {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: Option<u64>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
}

impl SongInfo {
    pub fn new(path: PathBuf) -> Self {
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        Self {
            path,
            title: file_stem,
            artist: String::new(),
            album: String::new(),
            duration_sec: None,
            bitrate_kbps: None,
            sample_rate: None,
            channels: None,
        }
    }

    pub fn display_name(&self) -> String {
        if !self.artist.is_empty() && !self.title.is_empty() {
            format!("{} - {}", self.artist, self.title)
        } else if !self.title.is_empty() {
            self.title.clone()
        } else {
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string()
        }
    }

    pub fn formatted_duration(&self) -> String {
        match self.duration_sec {
            Some(sec) => {
                let m = sec / 60;
                let s = sec % 60;
                format!("{:02}:{:02}", m, s)
            }
            None => "--:--".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackStatus {
    pub state: PlaybackState,
    pub current_file: Option<SongInfo>,
    pub current_sec: u64,
    pub total_sec: Option<u64>,
    pub volume: u32,
    pub shuffle: bool,
    pub repeat: bool,
    pub autonext: bool,
    pub playlist_len: usize,
    pub current_index: Option<usize>,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stop,
            current_file: None,
            current_sec: 0,
            total_sec: None,
            volume: 80,
            shuffle: false,
            repeat: false,
            autonext: true,
            playlist_len: 0,
            current_index: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientRequest {
    GetStatus,
    GetPlaylist,
    Play,
    PlayIndex(usize),
    PlayPath(PathBuf),
    Pause,
    Unpause,
    TogglePause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    JumpSec(u64),
    JumpPercent(u8),
    SetVolume(u32),
    AdjustVolume(i32),
    Append(Vec<PathBuf>),
    Enqueue(Vec<PathBuf>),
    Clear,
    Remove(usize),
    MoveItem { from: usize, to: usize },
    ToggleControl(String),
    SetControl { name: String, value: bool },
    QuitServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerResponse {
    Ok,
    Status(PlaybackStatus),
    Playlist(Vec<SongInfo>),
    Error(String),
}
