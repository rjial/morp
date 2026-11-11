use crate::audio::{AudioCommand, AudioEngine, AudioEvent};
use crate::domain::{ClientRequest, PlaybackState, PlaybackStatus, ServerResponse};
use crate::playlist::Playlist;
use crate::scrobble::{ScrobbleConfig, ScrobbleEvent, Scrobbler};
use rust_i18n::t;
use notify_rust::Notification;
use souvlaki::{
    MediaControlEvent, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, MediaControls,
    SeekDirection,
};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn get_socket_path() -> PathBuf {
    if let Ok(morp_dir) = std::env::var("MORP_DIR") {
        PathBuf::from(morp_dir).join("socket")
    } else if let Some(home) = dirs::home_dir() {
        home.join(".morp").join("socket")
    } else {
        PathBuf::from("/tmp/morp.sock")
    }
}

pub struct Server {
    audio: AudioEngine,
    playlist: Arc<Mutex<Playlist>>,
    event_receiver: Receiver<AudioEvent>,
    socket_path: PathBuf,
}

impl Server {
    pub fn new() -> Self {
        let (event_sender, event_receiver) = channel();
        let audio = AudioEngine::new(event_sender);
        let playlist = Arc::new(Mutex::new(Playlist::new()));
        let socket_path = get_socket_path();

        Self {
            audio,
            playlist,
            event_receiver,
            socket_path,
        }
    }

    pub fn run(self) -> Result<(), String> {
        let socket_path = self.socket_path.clone();

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Clean up stale socket if any
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_err() {
                let _ = fs::remove_file(&socket_path);
            } else {
                return Err("Server is already running on socket".to_string());
            }
        }

        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| format!("Failed to bind socket {}: {}", socket_path.display(), e))?;

        listener
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        let audio = Arc::new(self.audio);
        let playlist = Arc::clone(&self.playlist);
        let event_receiver = self.event_receiver;

        // Initialize MPRIS controls
        let mpris_config = PlatformConfig {
            dbus_name: "org.mpris.MediaPlayer2.morp",
            display_name: "Music On Rust",
            hwnd: None,
        };
        let mut controls = MediaControls::new(mpris_config).ok();

        if let Some(ref mut c) = controls {
            let audio_for_mpris = Arc::clone(&audio);
            let playlist_for_mpris = Arc::clone(&playlist);
            let _ = c.attach(move |event| {
                match event {
                    MediaControlEvent::Play => {
                        let state = *audio_for_mpris.status.state.lock().unwrap();
                        if state == PlaybackState::Pause {
                            audio_for_mpris.send(AudioCommand::Resume);
                        } else if state == PlaybackState::Stop {
                            let mut pl = playlist_for_mpris.lock().unwrap();
                            let current = pl.current_song().cloned().or_else(|| pl.next().cloned());
                            if let Some(song) = current {
                                audio_for_mpris.send(AudioCommand::Play(song.path));
                            }
                        }
                    }
                    MediaControlEvent::Pause => {
                        audio_for_mpris.send(AudioCommand::Pause);
                    }
                    MediaControlEvent::Toggle => {
                        let state = *audio_for_mpris.status.state.lock().unwrap();
                        if state == PlaybackState::Play {
                            audio_for_mpris.send(AudioCommand::Pause);
                        } else if state == PlaybackState::Pause {
                            audio_for_mpris.send(AudioCommand::Resume);
                        } else {
                            let mut pl = playlist_for_mpris.lock().unwrap();
                            let current = pl.current_song().cloned().or_else(|| pl.next().cloned());
                            if let Some(song) = current {
                                audio_for_mpris.send(AudioCommand::Play(song.path));
                            }
                        }
                    }
                    MediaControlEvent::Stop => {
                        audio_for_mpris.send(AudioCommand::Stop);
                    }
                    MediaControlEvent::Next => {
                        let mut pl = playlist_for_mpris.lock().unwrap();
                        if let Some(song) = pl.next() {
                            let p = song.path.clone();
                            audio_for_mpris.send(AudioCommand::Play(p));
                        } else {
                            audio_for_mpris.send(AudioCommand::Stop);
                        }
                    }
                    MediaControlEvent::Previous => {
                        let mut pl = playlist_for_mpris.lock().unwrap();
                        if let Some(song) = pl.prev() {
                            let p = song.path.clone();
                            audio_for_mpris.send(AudioCommand::Play(p));
                        }
                    }
                    MediaControlEvent::Seek(direction) => {
                        let delta = match direction {
                            SeekDirection::Forward => 5,
                            SeekDirection::Backward => -5,
                        };
                        audio_for_mpris.send(AudioCommand::Seek(delta));
                    }
                    MediaControlEvent::SeekBy(direction, duration) => {
                        let secs = duration.as_secs() as i64;
                        let delta = match direction {
                            SeekDirection::Forward => secs,
                            SeekDirection::Backward => -secs,
                        };
                        audio_for_mpris.send(AudioCommand::Seek(delta));
                    }
                    MediaControlEvent::SetPosition(position) => {
                        audio_for_mpris.send(AudioCommand::Jump(position.0.as_secs()));
                    }
                    _ => {}
                }
            });
        }

        let controls = Arc::new(Mutex::new(controls));

        // Setup scrobbler
        let scrobble_tx: Option<Sender<ScrobbleEvent>> = {
            let cfg = ScrobbleConfig::load();
            if cfg.is_ready() {
                let (tx, rx) = channel::<ScrobbleEvent>();
                let scrobbler = Scrobbler::new(cfg);
                thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        match event {
                            ScrobbleEvent::NowPlaying { artist, title, album, duration } => {
                                scrobbler.now_playing(&artist, &title, &album, duration);
                            }
                            ScrobbleEvent::Scrobble { artist, title, album, duration, timestamp } => {
                                scrobbler.scrobble(&artist, &title, &album, duration, timestamp);
                            }
                        }
                    }
                });
                Some(tx)
            } else {
                None
            }
        };

        // Start background observer for MPRIS updates and Notifications
        let audio_for_obs = Arc::clone(&audio);
        let playlist_for_obs = Arc::clone(&playlist);
        let controls_for_obs = Arc::clone(&controls);
        let scrobble_tx_obs = scrobble_tx;
        thread::spawn(move || {
            let mut last_state = PlaybackState::Stop;
            let mut last_path = PathBuf::new();
            let mut track_start: Option<u64> = None;
            let mut scrobbled = true;

            loop {
                thread::sleep(Duration::from_millis(250));

                let current_state = *audio_for_obs.status.state.lock().unwrap();
                let current_sec = audio_for_obs.status.current_sec.load(Ordering::SeqCst);
                let total_sec = audio_for_obs.status.total_sec.load(Ordering::SeqCst);

                let (current_path, current_title, current_artist, current_album, song_duration) = {
                    let pl = playlist_for_obs.lock().unwrap();
                    if let Some(song) = pl.current_song() {
                        (
                            song.path.clone(),
                            song.title.clone(),
                            song.artist.clone(),
                            song.album.clone(),
                            song.duration_sec,
                        )
                    } else {
                        (PathBuf::new(), String::new(), String::new(), String::new(), None)
                    }
                };

                // Scrobble tracking
                if current_path != last_path {
                    // Track changed: reset scrobble state, send NowPlaying
                    track_start = None;
                    scrobbled = true;

                    if !current_path.as_os_str().is_empty()
                        && current_state == PlaybackState::Play
                        && !current_title.is_empty()
                    {
                        // Don't scrobble tracks shorter than 30 seconds
                        let too_short = song_duration.map(|d| d < 30).unwrap_or(false);
                        if !too_short {
                            track_start = Some(crate::scrobble::unix_now());
                            scrobbled = false;
                            if let Some(ref tx) = scrobble_tx_obs {
                                let _ = tx.send(ScrobbleEvent::NowPlaying {
                                    artist: current_artist.clone(),
                                    title: current_title.clone(),
                                    album: current_album.clone(),
                                    duration: song_duration,
                                });
                            }
                        }
                    }
                } else if current_state == PlaybackState::Play && !scrobbled {
                    if let (Some(start), Some(duration)) = (track_start, song_duration) {
                        let elapsed = crate::scrobble::unix_now().saturating_sub(start);
                        // Scrobble at 50% or 4 minutes, whichever comes first
                        let threshold = (duration / 2).min(240);
                        if elapsed >= threshold && elapsed >= 30 {
                            scrobbled = true;
                            if let Some(ref tx) = scrobble_tx_obs {
                                let _ = tx.send(ScrobbleEvent::Scrobble {
                                    artist: current_artist.clone(),
                                    title: current_title.clone(),
                                    album: current_album.clone(),
                                    duration: Some(duration),
                                    timestamp: start,
                                });
                            }
                        }
                    }
                }

                let mut guard = controls_for_obs.lock().unwrap();
                if let Some(ref mut c) = *guard {
                    // 1. Update metadata if track changed
                    if current_path != last_path {
                        if !current_path.as_os_str().is_empty() {
                            let metadata = MediaMetadata {
                                title: Some(&current_title),
                                artist: if current_artist.is_empty() { None } else { Some(&current_artist) },
                                album: if current_album.is_empty() { None } else { Some(&current_album) },
                                duration: song_duration.map(Duration::from_secs).or_else(|| {
                                    if total_sec > 0 { Some(Duration::from_secs(total_sec)) } else { None }
                                }),
                                cover_url: None,
                            };
                            let _ = c.set_metadata(metadata);

                            let body = if !current_artist.is_empty() {
                                format!("{} - {}", current_artist, current_title)
                            } else {
                                current_title.clone()
                            };
                            let _ = Notification::new()
                                .summary(&t!("now_playing"))
                                .body(&body)
                                .icon("audio-x-generic")
                                .show();
                        } else {
                            let _ = c.set_metadata(MediaMetadata::default());
                        }
                        last_path = current_path.clone();
                    }

                    // 2. Update Playback state
                    if current_state != last_state || current_path != last_path {
                        let mpris_pos = Some(MediaPosition(Duration::from_secs(current_sec)));
                        let playback_state = match current_state {
                            PlaybackState::Play => MediaPlayback::Playing { progress: mpris_pos },
                            PlaybackState::Pause => MediaPlayback::Paused { progress: mpris_pos },
                            PlaybackState::Stop => MediaPlayback::Stopped,
                        };
                        let _ = c.set_playback(playback_state);
                        last_state = current_state;
                    } else if current_state == PlaybackState::Play {
                        let mpris_pos = Some(MediaPosition(Duration::from_secs(current_sec)));
                        let _ = c.set_playback(MediaPlayback::Playing { progress: mpris_pos });
                    }
                }
            }
        });

        // Background thread to handle TrackFinished events
        let audio_for_events = Arc::clone(&audio);
        let playlist_for_events = Arc::clone(&playlist);
        thread::spawn(move || {
            while let Ok(event) = event_receiver.recv() {
                if event == AudioEvent::TrackFinished {
                    let mut pl = playlist_for_events.lock().unwrap();
                    if pl.autonext {
                        if let Some(next_song) = pl.next() {
                            let path = next_song.path.clone();
                            audio_for_events.send(AudioCommand::Play(path));
                        } else {
                            audio_for_events.send(AudioCommand::Stop);
                        }
                    } else {
                        audio_for_events.send(AudioCommand::Stop);
                    }
                }
            }
        });

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let audio_clone = Arc::clone(&audio);
                    let playlist_clone = Arc::clone(&playlist);
                    let socket_path_clone = socket_path.clone();

                    thread::spawn(move || {
                        handle_client(stream, audio_clone, playlist_clone, socket_path_clone);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("Socket accept error: {}", e);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

fn handle_client(
    mut stream: UnixStream,
    audio: Arc<AudioEngine>,
    playlist: Arc<Mutex<Playlist>>,
    socket_path: PathBuf,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();

    while let Ok(bytes) = reader.read_line(&mut line) {
        if bytes == 0 {
            break;
        }

        let req_str = line.trim();
        if req_str.is_empty() {
            line.clear();
            continue;
        }

        let req: Result<ClientRequest, _> = serde_json::from_str(req_str);
        let resp = match req {
            Ok(request) => match request {
                ClientRequest::GetStatus => {
                    let pl = playlist.lock().unwrap();
                    let st = *audio.status.state.lock().unwrap();
                    let cur_sec = audio.status.current_sec.load(Ordering::SeqCst);
                    let total_sec = {
                        let t = audio.status.total_sec.load(Ordering::SeqCst);
                        if t > 0 {
                            Some(t)
                        } else {
                            pl.current_song().and_then(|s| s.duration_sec)
                        }
                    };
                    let vol = audio.status.volume.load(Ordering::SeqCst);
                    let mut cur_file = pl.current_song().cloned();
                    if let Some(ref mut file) = cur_file {
                        if file.bitrate_kbps.is_none() {
                            let b = audio.status.bitrate.load(Ordering::SeqCst);
                            if b > 0 {
                                file.bitrate_kbps = Some(b);
                            }
                        }
                    }

                    let status = PlaybackStatus {
                        state: st,
                        current_file: cur_file,
                        current_sec: cur_sec,
                        total_sec,
                        volume: vol,
                        shuffle: pl.shuffle,
                        repeat: pl.repeat,
                        autonext: pl.autonext,
                        playlist_len: pl.len(),
                        current_index: pl.current_index,
                    };

                    ServerResponse::Status(status)
                }
                ClientRequest::GetPlaylist => {
                    let pl = playlist.lock().unwrap();
                    ServerResponse::Playlist(pl.items.clone())
                }
                ClientRequest::Play => {
                    let mut pl = playlist.lock().unwrap();
                    let current = pl.current_song().cloned().or_else(|| pl.next().cloned());
                    if let Some(song) = current {
                        audio.send(AudioCommand::Play(song.path));
                    }
                    ServerResponse::Ok
                }
                ClientRequest::PlayIndex(idx) => {
                    let mut pl = playlist.lock().unwrap();
                    if let Some(song) = pl.play_index(idx) {
                        let path = song.path.clone();
                        audio.send(AudioCommand::Play(path));
                    }
                    ServerResponse::Ok
                }
                ClientRequest::PlayPath(path) => {
                    let mut pl = playlist.lock().unwrap();
                    pl.add_path(&path);
                    let last_idx = pl.len() - 1;
                    if let Some(song) = pl.play_index(last_idx) {
                        let p = song.path.clone();
                        audio.send(AudioCommand::Play(p));
                    }
                    ServerResponse::Ok
                }
                ClientRequest::Pause => {
                    audio.send(AudioCommand::Pause);
                    ServerResponse::Ok
                }
                ClientRequest::Unpause => {
                    audio.send(AudioCommand::Resume);
                    ServerResponse::Ok
                }
                ClientRequest::TogglePause => {
                    let state = *audio.status.state.lock().unwrap();
                    if state == PlaybackState::Play {
                        audio.send(AudioCommand::Pause);
                    } else if state == PlaybackState::Pause {
                        audio.send(AudioCommand::Resume);
                    } else {
                        let mut pl = playlist.lock().unwrap();
                        let song_path = pl
                            .current_song()
                            .cloned()
                            .or_else(|| pl.next().cloned())
                            .map(|s| s.path);
                        if let Some(p) = song_path {
                            audio.send(AudioCommand::Play(p));
                        }
                    }
                    ServerResponse::Ok
                }
                ClientRequest::Stop => {
                    audio.send(AudioCommand::Stop);
                    ServerResponse::Ok
                }
                ClientRequest::Next => {
                    let mut pl = playlist.lock().unwrap();
                    if let Some(song) = pl.next() {
                        let p = song.path.clone();
                        audio.send(AudioCommand::Play(p));
                    } else {
                        audio.send(AudioCommand::Stop);
                    }
                    ServerResponse::Ok
                }
                ClientRequest::Previous => {
                    let mut pl = playlist.lock().unwrap();
                    if let Some(song) = pl.prev() {
                        let p = song.path.clone();
                        audio.send(AudioCommand::Play(p));
                    }
                    ServerResponse::Ok
                }
                ClientRequest::Seek(seconds) => {
                    audio.send(AudioCommand::Seek(seconds));
                    ServerResponse::Ok
                }
                ClientRequest::JumpSec(sec) => {
                    audio.send(AudioCommand::Jump(sec));
                    ServerResponse::Ok
                }
                ClientRequest::JumpPercent(pct) => {
                    let total = audio.status.total_sec.load(Ordering::SeqCst);
                    if total > 0 {
                        let target = (total * pct.min(100) as u64) / 100;
                        audio.send(AudioCommand::Jump(target));
                    }
                    ServerResponse::Ok
                }
                ClientRequest::SetVolume(vol) => {
                    audio.send(AudioCommand::SetVolume(vol.min(100)));
                    ServerResponse::Ok
                }
                ClientRequest::AdjustVolume(delta) => {
                    let current = audio.status.volume.load(Ordering::SeqCst) as i32;
                    let target = (current + delta).clamp(0, 100) as u32;
                    audio.send(AudioCommand::SetVolume(target));
                    ServerResponse::Ok
                }
                ClientRequest::Append(paths) => {
                    let mut pl = playlist.lock().unwrap();
                    pl.append_paths(&paths);
                    ServerResponse::Ok
                }
                ClientRequest::Enqueue(paths) => {
                    let mut pl = playlist.lock().unwrap();
                    pl.append_paths(&paths);
                    ServerResponse::Ok
                }
                ClientRequest::Clear => {
                    let mut pl = playlist.lock().unwrap();
                    pl.clear();
                    audio.send(AudioCommand::Stop);
                    ServerResponse::Ok
                }
                ClientRequest::Remove(idx) => {
                    let mut pl = playlist.lock().unwrap();
                    let was_curr = pl.current_index == Some(idx);
                    pl.remove(idx);
                    if was_curr {
                        if let Some(next_song) = pl.current_song() {
                            let p = next_song.path.clone();
                            audio.send(AudioCommand::Play(p));
                        } else {
                            audio.send(AudioCommand::Stop);
                        }
                    }
                    ServerResponse::Ok
                }
                ClientRequest::MoveItem { from, to } => {
                    let mut pl = playlist.lock().unwrap();
                    pl.move_item(from, to);
                    ServerResponse::Ok
                }
                ClientRequest::ToggleControl(ctrl) => {
                    let mut pl = playlist.lock().unwrap();
                    match ctrl.to_lowercase().as_str() {
                        "shuffle" => {
                            pl.toggle_shuffle();
                        }
                        "repeat" => {
                            pl.toggle_repeat();
                        }
                        "autonext" => {
                            pl.toggle_autonext();
                        }
                        _ => {}
                    }
                    ServerResponse::Ok
                }
                ClientRequest::SetControl { name, value } => {
                    let mut pl = playlist.lock().unwrap();
                    match name.to_lowercase().as_str() {
                        "shuffle" => {
                            pl.shuffle = value;
                        }
                        "repeat" => {
                            pl.repeat = value;
                        }
                        "autonext" => {
                            pl.autonext = value;
                        }
                        _ => {}
                    }
                    ServerResponse::Ok
                }
                ClientRequest::QuitServer => {
                    audio.send(AudioCommand::Stop);
                    let _ = fs::remove_file(&socket_path);
                    let _ = writeln!(stream, "{}", serde_json::to_string(&ServerResponse::Ok).unwrap());
                    std::process::exit(0);
                }
            },
            Err(e) => ServerResponse::Error(format!("Invalid request: {}", e)),
        };

        if let Ok(resp_json) = serde_json::to_string(&resp) {
            let _ = writeln!(stream, "{}", resp_json);
        }

        line.clear();
    }
}
