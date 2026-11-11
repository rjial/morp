mod audio;
mod cli;
mod client;
mod domain;
mod library;
mod playlist;
mod server;
mod tags;
mod ui;

use clap::Parser;
use cli::CliArgs;
use client::Client;
use domain::PlaybackState;
use server::Server;
use std::fs;
use std::path::PathBuf;
use ui::{run_ui, App};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

    // 1. Run as server if requested
    if args.server || args.foreground {
        let srv = Server::new();
        if let Err(e) = srv.run() {
            eprintln!("Error starting server: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

    // 2. Scan library if requested via CLI
    if args.scan_library {
        let music_dir = args.music_dir.unwrap_or_else(library::Library::get_default_music_dir);
        println!("Scanning music library at: {}", music_dir.display());
        let lib = library::Library::scan(&music_dir);
        lib.save_cache().map_err(|e| format!("Failed to save library cache: {}", e))?;
        println!(
            "Scan complete! Indexed {} tracks across {} artists.",
            lib.tracks.len(),
            lib.artists().len()
        );
        return Ok(());
    }

    // 2. Determine if any CLI command flags or positional arguments were provided
    let has_cmd = args.play
        || args.pause
        || args.unpause
        || args.toggle_pause
        || args.stop
        || args.next
        || args.previous
        || args.seek.is_some()
        || args.jump.is_some()
        || args.volume.is_some()
        || args.append
        || args.enqueue
        || args.clear
        || args.exit
        || args.info
        || args.format.is_some()
        || args.toggle.is_some()
        || args.on.is_some()
        || args.off.is_some()
        || (!args.files.is_empty() && (args.append || args.enqueue));

    if has_cmd {
        handle_cli_commands(args)?;
        return Ok(());
    }

    // If positional files are given without flags, add them and play
    if !args.files.is_empty() {
        let mut client = Client::ensure_server().map_err(|e| format!("Failed to connect to mocp server: {}", e))?;
        let abs_paths: Vec<PathBuf> = args
            .files
            .iter()
            .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
            .collect();
        client.append(abs_paths).map_err(|e| format!("Append failed: {}", e))?;
        client.play().map_err(|e| format!("Play failed: {}", e))?;
        return Ok(());
    }

    // 4. Otherwise launch interactive TUI
    let client = Client::ensure_server().map_err(|e| format!("Failed to start/connect mocp server: {}", e))?;
    let app = App::new(client, args.music_dir);
    if let Err(e) = run_ui(app) {
        eprintln!("TUI Error: {}", e);
    }

    Ok(())
}

fn handle_cli_commands(args: CliArgs) -> Result<(), String> {
    let mut client = Client::ensure_server().map_err(|e| format!("Failed to connect to mocp server: {}", e))?;

    // Clear playlist
    if args.clear {
        client.clear()?;
    }

    // Append or enqueue files
    if !args.files.is_empty() && (args.append || args.enqueue) {
        let abs_paths: Vec<PathBuf> = args
            .files
            .iter()
            .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
            .collect();
        client.append(abs_paths)?;
    }

    // Playback control
    if args.play {
        client.play()?;
    }
    if args.pause {
        client.pause()?;
    }
    if args.unpause {
        client.unpause()?;
    }
    if args.toggle_pause {
        client.toggle_pause()?;
    }
    if args.stop {
        client.stop()?;
    }
    if args.next {
        client.next()?;
    }
    if args.previous {
        client.previous()?;
    }

    // Seek
    if let Some(sec) = args.seek {
        client.seek(sec)?;
    }

    // Jump
    if let Some(pos) = args.jump {
        let pos_trim = pos.trim();
        if pos_trim.ends_with('%') {
            if let Ok(pct) = pos_trim.trim_end_matches('%').parse::<u8>() {
                client.jump_percent(pct)?;
            }
        } else if pos_trim.ends_with('s') {
            if let Ok(sec) = pos_trim.trim_end_matches('s').parse::<u64>() {
                client.jump_sec(sec)?;
            }
        } else if let Ok(sec) = pos_trim.parse::<u64>() {
            client.jump_sec(sec)?;
        }
    }

    // Volume
    if let Some(vol_str) = args.volume {
        let s = vol_str.trim();
        if s.starts_with('+') {
            if let Ok(delta) = s[1..].parse::<i32>() {
                client.adjust_volume(delta)?;
            }
        } else if s.starts_with('-') {
            if let Ok(delta) = s[1..].parse::<i32>() {
                client.adjust_volume(-delta)?;
            }
        } else if let Ok(level) = s.parse::<u32>() {
            client.set_volume(level)?;
        }
    }

    // Control toggles
    if let Some(ctrl) = args.toggle {
        client.toggle_control(&ctrl)?;
    }
    if let Some(ctrl) = args.on {
        client.set_control(&ctrl, true)?;
    }
    if let Some(ctrl) = args.off {
        client.set_control(&ctrl, false)?;
    }

    // Print info (-i)
    if args.info {
        let status = client.get_status()?;
        print_info(&status);
    }

    // Formatted info (-Q)
    if let Some(fmt) = args.format {
        let status = client.get_status()?;
        print_formatted(&status, &fmt);
    }

    // Shutdown server (-x)
    if args.exit {
        client.quit_server()?;
    }

    Ok(())
}

fn print_info(status: &domain::PlaybackStatus) {
    println!("State: {}", status.state);
    if status.state == PlaybackState::Stop {
        return;
    }

    let file_path = status
        .current_file
        .as_ref()
        .map(|f| f.path.to_string_lossy().to_string())
        .unwrap_or_default();

    let title = status
        .current_file
        .as_ref()
        .map(|f| f.display_name())
        .unwrap_or_default();

    let artist = status
        .current_file
        .as_ref()
        .map(|f| f.artist.clone())
        .unwrap_or_default();

    let song_title = status
        .current_file
        .as_ref()
        .map(|f| f.title.clone())
        .unwrap_or_default();

    let album = status
        .current_file
        .as_ref()
        .map(|f| f.album.clone())
        .unwrap_or_default();

    let cur_sec = status.current_sec;
    let tot_sec = status.total_sec.unwrap_or(0);
    let left_sec = if tot_sec >= cur_sec { tot_sec - cur_sec } else { 0 };

    let total_time_str = if tot_sec > 0 {
        format!("{:02}:{:02}", tot_sec / 60, tot_sec % 60)
    } else {
        String::new()
    };

    let time_left_str = if tot_sec > 0 {
        format!("{:02}:{:02}", left_sec / 60, left_sec % 60)
    } else {
        String::new()
    };

    let cur_time_str = format!("{:02}:{:02}", cur_sec / 60, cur_sec % 60);

    let bitrate_str = status
        .current_file
        .as_ref()
        .and_then(|f| f.bitrate_kbps)
        .map(|b| format!("{}kbps", b))
        .unwrap_or_else(|| "".to_string());

    let rate_str = status
        .current_file
        .as_ref()
        .and_then(|f| f.sample_rate)
        .map(|r| format!("{}kHz", r / 1000))
        .unwrap_or_else(|| "".to_string());

    println!("File: {}", file_path);
    println!("Title: {}", title);
    println!("Artist: {}", artist);
    println!("SongTitle: {}", song_title);
    println!("Album: {}", album);
    println!("TotalTime: {}", total_time_str);
    println!("TimeLeft: {}", time_left_str);
    println!("TotalSec: {}", tot_sec);
    println!("CurrentTime: {}", cur_time_str);
    println!("CurrentSec: {}", cur_sec);
    println!("Bitrate: {}", bitrate_str);
    println!("AvgBitrate: {}", bitrate_str);
    println!("Rate: {}", rate_str);
}

fn print_formatted(status: &domain::PlaybackStatus, fmt: &str) {
    let file_path = status
        .current_file
        .as_ref()
        .map(|f| f.path.to_string_lossy().to_string())
        .unwrap_or_default();

    let title = status
        .current_file
        .as_ref()
        .map(|f| f.display_name())
        .unwrap_or_default();

    let artist = status
        .current_file
        .as_ref()
        .map(|f| f.artist.clone())
        .unwrap_or_default();

    let song_title = status
        .current_file
        .as_ref()
        .map(|f| f.title.clone())
        .unwrap_or_default();

    let album = status
        .current_file
        .as_ref()
        .map(|f| f.album.clone())
        .unwrap_or_default();

    let cur_sec = status.current_sec;
    let tot_sec = status.total_sec.unwrap_or(0);
    let left_sec = if tot_sec >= cur_sec { tot_sec - cur_sec } else { 0 };

    let total_time_str = if tot_sec > 0 {
        format!("{:02}:{:02}", tot_sec / 60, tot_sec % 60)
    } else {
        "--:--".to_string()
    };

    let time_left_str = if tot_sec > 0 {
        format!("{:02}:{:02}", left_sec / 60, left_sec % 60)
    } else {
        "--:--".to_string()
    };

    let cur_time_str = format!("{:02}:{:02}", cur_sec / 60, cur_sec % 60);

    let bitrate_str = status
        .current_file
        .as_ref()
        .and_then(|f| f.bitrate_kbps)
        .map(|b| format!("{}kbps", b))
        .unwrap_or_default();

    let rate_str = status
        .current_file
        .as_ref()
        .and_then(|f| f.sample_rate)
        .map(|r| format!("{}kHz", r / 1000))
        .unwrap_or_default();

    let result = fmt
        .replace("%state", &status.state.to_string())
        .replace("%file", &file_path)
        .replace("%title", &title)
        .replace("%artist", &artist)
        .replace("%song", &song_title)
        .replace("%album", &album)
        .replace("%tt", &total_time_str)
        .replace("%tl", &time_left_str)
        .replace("%ts", &tot_sec.to_string())
        .replace("%ct", &cur_time_str)
        .replace("%cs", &cur_sec.to_string())
        .replace("%b", &bitrate_str)
        .replace("%r", &rate_str);

    println!("{}", result);
}
