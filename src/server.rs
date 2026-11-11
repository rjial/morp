use crate::audio::{AudioCommand, AudioEngine, AudioEvent};
use crate::domain::{ClientRequest, PlaybackState, PlaybackStatus, ServerResponse};
use crate::playlist::Playlist;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn get_socket_path() -> PathBuf {
    if let Ok(moc_dir) = std::env::var("MOC_DIR") {
        PathBuf::from(moc_dir).join("socket")
    } else if let Some(home) = dirs::home_dir() {
        home.join(".moc").join("socket")
    } else {
        PathBuf::from("/tmp/mocp.sock")
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
