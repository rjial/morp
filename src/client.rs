use crate::domain::{ClientRequest, PlaybackStatus, ServerResponse, SongInfo};
use crate::server::get_socket_path;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub struct Client {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Client {
    pub fn connect() -> Result<Self, String> {
        let socket_path = get_socket_path();
        let stream = UnixStream::connect(&socket_path)
            .map_err(|e| format!("Could not connect to server at {}: {}", socket_path.display(), e))?;
        let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);

        Ok(Self { stream, reader })
    }

    pub fn ensure_server() -> Result<Self, String> {
        match Self::connect() {
            Ok(client) => Ok(client),
            Err(_) => {
                // Spawn server in the background
                let current_exe = env::current_exe()
                    .map_err(|e| format!("Failed to get current executable: {}", e))?;

                Command::new(&current_exe)
                    .arg("--server")
                    .spawn()
                    .map_err(|e| format!("Failed to spawn morp server: {}", e))?;

                // Wait for socket to become available (up to 2.5 seconds)
                let start = std::time::Instant::now();
                while start.elapsed() < Duration::from_millis(2500) {
                    thread::sleep(Duration::from_millis(50));
                    if let Ok(client) = Self::connect() {
                        return Ok(client);
                    }
                }

                Err("Timed out waiting for morp server to start".to_string())
            }
        }
    }

    pub fn send_request(&mut self, req: &ClientRequest) -> Result<ServerResponse, String> {
        let req_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        writeln!(self.stream, "{}", req_json).map_err(|e| e.to_string())?;
        self.stream.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        self.reader.read_line(&mut line).map_err(|e| e.to_string())?;

        let resp: ServerResponse =
            serde_json::from_str(line.trim()).map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(resp)
    }

    pub fn get_status(&mut self) -> Result<PlaybackStatus, String> {
        match self.send_request(&ClientRequest::GetStatus)? {
            ServerResponse::Status(status) => Ok(status),
            ServerResponse::Error(e) => Err(e),
            _ => Err("Unexpected server response".to_string()),
        }
    }

    pub fn get_playlist(&mut self) -> Result<Vec<SongInfo>, String> {
        match self.send_request(&ClientRequest::GetPlaylist)? {
            ServerResponse::Playlist(items) => Ok(items),
            ServerResponse::Error(e) => Err(e),
            _ => Err("Unexpected server response".to_string()),
        }
    }

    pub fn play(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Play)
    }

    pub fn play_index(&mut self, idx: usize) -> Result<(), String> {
        self.send_ok(&ClientRequest::PlayIndex(idx))
    }

    pub fn play_path(&mut self, path: PathBuf) -> Result<(), String> {
        self.send_ok(&ClientRequest::PlayPath(path))
    }

    pub fn pause(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Pause)
    }

    pub fn unpause(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Unpause)
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::TogglePause)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Stop)
    }

    pub fn next(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Next)
    }

    pub fn previous(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Previous)
    }

    pub fn seek(&mut self, sec: i64) -> Result<(), String> {
        self.send_ok(&ClientRequest::Seek(sec))
    }

    pub fn jump_sec(&mut self, sec: u64) -> Result<(), String> {
        self.send_ok(&ClientRequest::JumpSec(sec))
    }

    pub fn jump_percent(&mut self, pct: u8) -> Result<(), String> {
        self.send_ok(&ClientRequest::JumpPercent(pct))
    }

    pub fn set_volume(&mut self, vol: u32) -> Result<(), String> {
        self.send_ok(&ClientRequest::SetVolume(vol))
    }

    pub fn adjust_volume(&mut self, delta: i32) -> Result<(), String> {
        self.send_ok(&ClientRequest::AdjustVolume(delta))
    }

    pub fn append(&mut self, paths: Vec<PathBuf>) -> Result<(), String> {
        self.send_ok(&ClientRequest::Append(paths))
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.send_ok(&ClientRequest::Clear)
    }

    pub fn remove(&mut self, idx: usize) -> Result<(), String> {
        self.send_ok(&ClientRequest::Remove(idx))
    }

    pub fn toggle_control(&mut self, ctrl: &str) -> Result<(), String> {
        self.send_ok(&ClientRequest::ToggleControl(ctrl.to_string()))
    }

    pub fn set_control(&mut self, name: &str, value: bool) -> Result<(), String> {
        self.send_ok(&ClientRequest::SetControl {
            name: name.to_string(),
            value,
        })
    }

    pub fn quit_server(&mut self) -> Result<(), String> {
        let _ = self.send_request(&ClientRequest::QuitServer);
        Ok(())
    }

    fn send_ok(&mut self, req: &ClientRequest) -> Result<(), String> {
        match self.send_request(req)? {
            ServerResponse::Ok => Ok(()),
            ServerResponse::Error(e) => Err(e),
            _ => Ok(()),
        }
    }
}
