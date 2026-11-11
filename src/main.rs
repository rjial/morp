rust_i18n::i18n!("locales", fallback = "en");

mod audio;
mod cli;
mod client;
mod domain;
mod library;
mod lyrics;
mod playlist;
mod scrobble;
mod server;
mod tags;
mod theme;
mod ui;

use clap::Parser;
use cli::CliArgs;
use client::Client;
use domain::PlaybackState;
use rust_i18n::t;
use server::Server;
use std::fs;
use std::path::PathBuf;
use ui::{run_ui, App};

fn detect_locale() -> String {
    if let Ok(lang) = std::env::var("MORP_LANG") {
        return lang;
    }
    if let Ok(lang) = std::env::var("LANG") {
        if lang.starts_with("id") {
            return "id".to_string();
        }
    }
    "en".to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rust_i18n::set_locale(&detect_locale());
    let args = CliArgs::parse();

    // Handle Last.fm/Libre.fm auth via browser callback (no server connection needed)
    if args.lastfm_auth {
        return handle_scrobble_auth("lastfm");
    }
    if args.librefm_auth {
        return handle_scrobble_auth("librefm");
    }

    // 1. Run as server if requested
    if args.server || args.foreground {
        let srv = Server::new();
        if let Err(e) = srv.run() {
            eprintln!("{}", t!("error.start_server", error = e.to_string()));
            std::process::exit(1);
        }
        return Ok(());
    }

    // 2. Scan library if requested via CLI
    if args.scan_library {
        let music_dir = args.music_dir.unwrap_or_else(library::Library::get_default_music_dir);
        println!("{}", t!("scan.at", path = music_dir.display().to_string()));
        let lib = library::Library::scan(&music_dir);
        lib.save_cache().map_err(|e| t!("error.scan_cache", error = e.to_string()).to_string())?;
        println!(
            "{}",
            t!("scan.complete", tracks = lib.tracks.len(), artists = lib.artists().len())
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
        let mut client = Client::ensure_server()
            .map_err(|e| t!("error.connect", error = e.to_string()).to_string())?;
        let abs_paths: Vec<PathBuf> = args
            .files
            .iter()
            .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
            .collect();
        client.append(abs_paths).map_err(|e| t!("error.append", error = e.to_string()).to_string())?;
        client.play().map_err(|e| t!("error.play", error = e.to_string()).to_string())?;
        return Ok(());
    }

    // 4. Otherwise launch interactive TUI
    let client = Client::ensure_server()
        .map_err(|e| t!("error.start_connect", error = e.to_string()).to_string())?;
    let app = App::new(client, args.music_dir);
    if let Err(e) = run_ui(app) {
        eprintln!("{}", t!("error.tui", error = e));
    }

    Ok(())
}

fn handle_cli_commands(args: CliArgs) -> Result<(), String> {
    let mut client = Client::ensure_server()
        .map_err(|e| t!("error.connect", error = e.to_string()).to_string())?;

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

fn handle_scrobble_auth(service: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = scrobble::ScrobbleConfig::load();
    config.service = service.to_string();

    if !config.can_authenticate() {
        eprintln!("{}", t!("auth.no_key"));
        eprintln!("{}", t!("auth.opt1"));
        if service == "librefm" {
            eprintln!("{}", t!("auth.opt2_librefm"));
        } else {
            eprintln!("{}", t!("auth.opt2_lastfm"));
        }
        eprintln!(
            "{}",
            t!("auth.opt3", path = scrobble::ScrobbleConfig::config_path().display().to_string())
        );
        eprintln!("{}", t!("auth.api_key_hint"));
        eprintln!("{}", t!("auth.api_secret_hint"));
        std::process::exit(1);
    }

    // Step 1: get temporary token
    print!("{}", t!("auth.connecting", service = service));
    let token = scrobble::get_token(&config)
        .map_err(|e| t!("error.get_token", error = e.to_string()).to_string())?;
    println!("{}", t!("auth.ok"));

    // Step 2: build URL and open browser
    let auth_url = scrobble::build_browser_url(&config, &token);
    println!("\n{}", t!("auth.open_url", service = service));
    println!("\n  {}\n", auth_url);

    if let Err(e) = open::that(&auth_url) {
        eprintln!("{}", t!("auth.browser_failed", error = e.to_string()));
        eprintln!("{}", t!("auth.copy_url"));
    } else {
        println!("{}", t!("auth.browser_opened"));
    }

    // Step 3: poll until user clicks Allow (timeout 5 minutes, interval 3 seconds)
    println!("{}\n", t!("auth.waiting"));
    let (session_key, username) = scrobble::poll_for_session(&config, &token, 300, 3)
        .map_err(|e| t!("error.poll", error = e.to_string()).to_string())?;

    // Save config
    config.session_key = session_key;
    if !username.is_empty() {
        config.username = username.clone();
    }
    config.enabled = true;
    config.save().map_err(|e| t!("error.save_config", error = e.to_string()).to_string())?;

    let suffix = if config.username.is_empty() {
        String::new()
    } else {
        t!("auth.as_user", username = config.username.clone()).to_string()
    };
    println!(
        "\n{}",
        t!(
            "auth.success",
            suffix = suffix,
            path = scrobble::ScrobbleConfig::config_path().display().to_string()
        )
    );

    Ok(())
}
