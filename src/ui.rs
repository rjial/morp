use crate::client::Client;
use crate::domain::{PlaybackState, PlaybackStatus, SongInfo};
use rust_i18n::t;
use crate::library::{Library, LibraryTrack};
use crate::lyrics::Lyrics;
use crate::tags::is_audio_file;
use crate::theme::Theme;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::Terminal;
use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentView {
    Library,
    Files,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFocus {
    Artists,
    Albums,
    Tracks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Directory,
    Playlist,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_parent: bool,
}

/// Tracks the screen regions of interactive lists so mouse clicks/scroll
/// can be routed to the right widget without re-computing the layout.
#[derive(Debug, Clone, Copy, Default)]
struct HitRegions {
    artists: Option<Rect>,
    albums: Option<Rect>,
    tracks: Option<Rect>,
    directory: Option<Rect>,
    playlist: Option<Rect>,
    playlist_full: Option<Rect>,
    progress_gauge: Option<Rect>,
    view_tab_library: Option<Rect>,
    view_tab_files: Option<Rect>,
    view_tab_playlist: Option<Rect>,
}

pub struct App {
    pub client: Client,
    pub current_view: CurrentView,
    // File view state
    pub active_panel: ActivePanel,
    pub current_dir: PathBuf,
    pub dir_entries: Vec<FileEntry>,
    pub dir_state: ListState,
    pub playlist_items: Vec<SongInfo>,
    pub playlist_state: ListState,
    // Library view state
    pub library: Library,
    pub library_focus: LibraryFocus,
    pub artists: Vec<String>,
    pub artist_state: ListState,
    pub albums: Vec<String>,
    pub album_state: ListState,
    pub library_tracks: Vec<LibraryTrack>,
    pub track_state: ListState,
    pub search_query: String,
    pub is_searching: bool,
    // Lyrics state (4.1 feature)
    pub lyrics: Lyrics,
    pub lyrics_path: Option<PathBuf>,
    pub show_lyrics: bool,
    // Theme config
    pub theme: Theme,
    // Mouse hit-test regions (recomputed every frame)
    hit_regions: HitRegions,
    // General app state
    pub status: PlaybackStatus,
    pub show_help: bool,
    pub should_quit: bool,
    pub should_shutdown_server: bool,
    pub message: Option<(String, std::time::Instant)>,
}

impl App {
    pub fn new(client: Client, music_dir: Option<PathBuf>) -> Self {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let target_music_dir = music_dir.unwrap_or_else(Library::get_default_music_dir);
        let library = Library::load_or_scan(&target_music_dir);

        let mut app = Self {
            client,
            current_view: CurrentView::Library,
            active_panel: ActivePanel::Directory,
            current_dir,
            dir_entries: Vec::new(),
            dir_state: ListState::default(),
            playlist_items: Vec::new(),
            playlist_state: ListState::default(),
            library,
            library_focus: LibraryFocus::Artists,
            artists: Vec::new(),
            artist_state: ListState::default(),
            albums: Vec::new(),
            album_state: ListState::default(),
            library_tracks: Vec::new(),
            track_state: ListState::default(),
            search_query: String::new(),
            is_searching: false,
            lyrics: Lyrics::default(),
            lyrics_path: None,
            show_lyrics: false,
            theme: Theme::load_or_default(),
            hit_regions: HitRegions::default(),
            status: PlaybackStatus::default(),
            show_help: false,
            should_quit: false,
            should_shutdown_server: false,
            message: None,
        };

        app.refresh_dir();
        app.refresh_status();
        app.refresh_playlist();
        app.init_library_data();
        app
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = Some((msg.to_string(), std::time::Instant::now()));
    }

    pub fn init_library_data(&mut self) {
        let mut artists = vec![t!("all_artists").to_string()];
        artists.extend(self.library.artists());
        self.artists = artists;
        if !self.artists.is_empty() {
            self.artist_state.select(Some(0));
        }

        self.update_albums();
        self.update_tracks();
    }

    pub fn update_albums(&mut self) {
        let selected_artist = self.selected_artist();
        let mut albums = vec![t!("all_albums").to_string()];
        albums.extend(self.library.albums_for_artist(selected_artist));
        self.albums = albums;
        self.album_state.select(Some(0));
    }

    pub fn update_tracks(&mut self) {
        if !self.search_query.is_empty() {
            let found = self.library.search(&self.search_query);
            self.library_tracks = found.into_iter().cloned().collect();
        } else {
            let selected_artist = self.selected_artist();
            let selected_album = self.selected_album();
            let tracks = self.library.tracks_for(selected_artist, selected_album);
            self.library_tracks = tracks.into_iter().cloned().collect();
        }

        if self.library_tracks.is_empty() {
            self.track_state.select(None);
        } else if self.track_state.selected().is_none() {
            self.track_state.select(Some(0));
        } else if let Some(sel) = self.track_state.selected() {
            if sel >= self.library_tracks.len() {
                self.track_state.select(Some(self.library_tracks.len() - 1));
            }
        }
    }

    pub fn selected_artist(&self) -> Option<&str> {
        let idx = self.artist_state.selected()?;
        if idx == 0 {
            return None; // first item is always "All Artists"
        }
        self.artists.get(idx).map(|s| s.as_str())
    }

    pub fn selected_album(&self) -> Option<&str> {
        let idx = self.album_state.selected()?;
        if idx == 0 {
            return None; // first item is always "All Albums"
        }
        self.albums.get(idx).map(|s| s.as_str())
    }

    pub fn rescan_library(&mut self) {
        self.set_message(&t!("msg.rescanning"));
        self.library = Library::scan(&self.library.music_dir);
        let _ = self.library.save_cache();
        self.init_library_data();
        let count = self.library.tracks.len();
        self.set_message(&t!("msg.scan_complete", count = count));
    }

    pub fn refresh_dir(&mut self) {
        let mut entries = Vec::new();

        if let Some(parent) = self.current_dir.parent() {
            entries.push(FileEntry {
                path: parent.to_path_buf(),
                name: "/..".to_string(),
                is_dir: true,
                is_parent: true,
            });
        }

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in read_dir.filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if name.starts_with('.') {
                    continue;
                }

                if path.is_dir() {
                    dirs.push(FileEntry {
                        path: path.clone(),
                        name: format!("/{}", name),
                        is_dir: true,
                        is_parent: false,
                    });
                } else if is_audio_file(&path) {
                    files.push(FileEntry {
                        path: path.clone(),
                        name,
                        is_dir: false,
                        is_parent: false,
                    });
                }
            }

            dirs.sort_by_key(|e| e.name.to_lowercase());
            files.sort_by_key(|e| e.name.to_lowercase());

            entries.extend(dirs);
            entries.extend(files);
        }

        self.dir_entries = entries;
        if self.dir_entries.is_empty() {
            self.dir_state.select(None);
        } else if self.dir_state.selected().is_none() {
            self.dir_state.select(Some(0));
        } else if let Some(selected) = self.dir_state.selected() {
            if selected >= self.dir_entries.len() {
                self.dir_state.select(Some(self.dir_entries.len() - 1));
            }
        }
    }

    pub fn refresh_status(&mut self) {
        if let Ok(st) = self.client.get_status() {
            self.status = st;
        }
        self.sync_lyrics_with_current_song();
    }

    pub fn refresh_playlist(&mut self) {
        if let Ok(items) = self.client.get_playlist() {
            self.playlist_items = items;
            if self.playlist_items.is_empty() {
                self.playlist_state.select(None);
            } else if self.playlist_state.selected().is_none() {
                self.playlist_state.select(Some(0));
            } else if let Some(sel) = self.playlist_state.selected() {
                if sel >= self.playlist_items.len() {
                    self.playlist_state.select(Some(self.playlist_items.len() - 1));
                }
            }
        }
    }

    /// Loads a `.lrc` file matching the currently playing song (same path,
    /// `.lrc` extension) whenever the track changes. Clears lyrics if the
    /// song has no sidecar lyrics file or nothing is playing.
    fn sync_lyrics_with_current_song(&mut self) {
        let current_path = self.status.current_file.as_ref().map(|f| f.path.clone());

        match current_path {
            Some(path) if Some(&path) != self.lyrics_path.as_ref() => {
                let lrc_path = path.with_extension("lrc");
                if lrc_path.exists() {
                    self.lyrics = Lyrics::parse_lrc(&lrc_path);
                } else {
                    self.lyrics = Lyrics::default();
                }
                self.lyrics_path = Some(path);
            }
            None => {
                self.lyrics = Lyrics::default();
                self.lyrics_path = None;
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // If search typing mode is active in Library view
        if self.is_searching {
            match key.code {
                KeyCode::Esc => {
                    self.is_searching = false;
                    self.search_query.clear();
                    self.update_tracks();
                }
                KeyCode::Enter => {
                    self.is_searching = false;
                    self.library_focus = LibraryFocus::Tracks;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_tracks();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_tracks();
                }
                _ => {}
            }
            return;
        }

        if self.show_help {
            if key.code == KeyCode::Char('q')
                || key.code == KeyCode::Esc
                || key.code == KeyCode::Char('?')
                || key.code == KeyCode::Char('h')
            {
                self.show_help = false;
            }
            return;
        }

        match (key.modifiers, key.code) {
            // General shortcuts
            (_, KeyCode::Char('?')) | (_, KeyCode::Char('h')) => {
                self.show_help = true;
            }
            (_, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            (_, KeyCode::Char('Q')) => {
                self.should_quit = true;
                self.should_shutdown_server = true;
            }
            // View Switching
            (_, KeyCode::Char('1')) | (_, KeyCode::F(1)) => {
                self.current_view = CurrentView::Library;
            }
            (_, KeyCode::Char('2')) | (_, KeyCode::F(2)) => {
                self.current_view = CurrentView::Files;
            }
            (_, KeyCode::Char('3')) | (_, KeyCode::F(3)) => {
                self.current_view = CurrentView::Playlist;
            }
            (_, KeyCode::Char('v')) => {
                self.current_view = match self.current_view {
                    CurrentView::Library => CurrentView::Files,
                    CurrentView::Files => CurrentView::Playlist,
                    CurrentView::Playlist => CurrentView::Library,
                };
            }
            // Lyrics toggle (feature 4.1)
            (_, KeyCode::Char('L')) => {
                self.show_lyrics = !self.show_lyrics;
                if self.show_lyrics && self.lyrics.lines.is_empty() {
                    self.set_message(&t!("msg.no_lyrics"));
                }
            }
            // Tab handling
            (_, KeyCode::Tab) => match self.current_view {
                CurrentView::Library => {
                    self.library_focus = match self.library_focus {
                        LibraryFocus::Artists => LibraryFocus::Albums,
                        LibraryFocus::Albums => LibraryFocus::Tracks,
                        LibraryFocus::Tracks => LibraryFocus::Artists,
                    };
                }
                CurrentView::Files => {
                    self.active_panel = match self.active_panel {
                        ActivePanel::Directory => ActivePanel::Playlist,
                        ActivePanel::Playlist => ActivePanel::Directory,
                    };
                }
                CurrentView::Playlist => {}
            },
            // Search in library
            (_, KeyCode::Char('/')) => {
                if self.current_view == CurrentView::Library {
                    self.is_searching = true;
                    self.search_query.clear();
                }
            }
            // Rescan library
            (_, KeyCode::Char('u')) | (_, KeyCode::F(5)) => {
                if self.current_view == CurrentView::Library {
                    self.rescan_library();
                }
            }
            // Playback controls
            (_, KeyCode::Char(' ')) => {
                let _ = self.client.toggle_pause();
                self.refresh_status();
            }
            (_, KeyCode::Char('s')) => {
                let _ = self.client.stop();
                self.refresh_status();
            }
            (_, KeyCode::Char('n')) | (_, KeyCode::Char('>')) => {
                let _ = self.client.next();
                self.refresh_status();
            }
            (_, KeyCode::Char('b')) | (_, KeyCode::Char('<')) => {
                let _ = self.client.previous();
                self.refresh_status();
            }
            (_, KeyCode::Left) | (_, KeyCode::Char(',')) => {
                let _ = self.client.seek(-5);
                self.refresh_status();
            }
            (_, KeyCode::Right) | (_, KeyCode::Char('.')) => {
                let _ = self.client.seek(5);
                self.refresh_status();
            }
            (_, KeyCode::Char('[')) => {
                let _ = self.client.seek(-15);
                self.refresh_status();
            }
            (_, KeyCode::Char(']')) => {
                let _ = self.client.seek(15);
                self.refresh_status();
            }
            // Volume
            (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => {
                let _ = self.client.adjust_volume(5);
                self.refresh_status();
            }
            (_, KeyCode::Char('-')) | (_, KeyCode::Char('_')) => {
                let _ = self.client.adjust_volume(-5);
                self.refresh_status();
            }
            // Toggles
            (_, KeyCode::Char('S')) => {
                let _ = self.client.toggle_control("shuffle");
                self.refresh_status();
            }
            (_, KeyCode::Char('R')) => {
                let _ = self.client.toggle_control("repeat");
                self.refresh_status();
            }
            (_, KeyCode::Char('X')) => {
                let _ = self.client.toggle_control("autonext");
                self.refresh_status();
            }
            // Navigation
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => match self.current_view {
                CurrentView::Library => match self.library_focus {
                    LibraryFocus::Artists => {
                        self.move_artist_up();
                        self.update_albums();
                        self.update_tracks();
                    }
                    LibraryFocus::Albums => {
                        self.move_album_up();
                        self.update_tracks();
                    }
                    LibraryFocus::Tracks => self.move_track_up(),
                },
                CurrentView::Files => match self.active_panel {
                    ActivePanel::Directory => self.dir_up(),
                    ActivePanel::Playlist => self.playlist_up(),
                },
                CurrentView::Playlist => self.playlist_up(),
            },
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => match self.current_view {
                CurrentView::Library => match self.library_focus {
                    LibraryFocus::Artists => {
                        self.move_artist_down();
                        self.update_albums();
                        self.update_tracks();
                    }
                    LibraryFocus::Albums => {
                        self.move_album_down();
                        self.update_tracks();
                    }
                    LibraryFocus::Tracks => self.move_track_down(),
                },
                CurrentView::Files => match self.active_panel {
                    ActivePanel::Directory => self.dir_down(),
                    ActivePanel::Playlist => self.playlist_down(),
                },
                CurrentView::Playlist => self.playlist_down(),
            },
            // Enter actions
            (_, KeyCode::Enter) => match self.current_view {
                CurrentView::Library => match self.library_focus {
                    LibraryFocus::Artists | LibraryFocus::Albums => {
                        // Play all tracks currently displayed in the tracks column
                        if !self.library_tracks.is_empty() {
                            let paths: Vec<PathBuf> = self.library_tracks.iter().map(|t| t.path.clone()).collect();
                            let _ = self.client.clear();
                            let _ = self.client.append(paths);
                            let _ = self.client.play();
                            self.set_message(&t!("msg.playing_tracks"));
                            self.refresh_playlist();
                            self.refresh_status();
                        }
                    }
                    LibraryFocus::Tracks => {
                        self.play_selected_library_track();
                    }
                },
                CurrentView::Files => match self.active_panel {
                    ActivePanel::Directory => self.enter_directory_item(),
                    ActivePanel::Playlist => {
                        if let Some(idx) = self.playlist_state.selected() {
                            let _ = self.client.play_index(idx);
                            self.refresh_status();
                        }
                    }
                },
                CurrentView::Playlist => {
                    if let Some(idx) = self.playlist_state.selected() {
                        let _ = self.client.play_index(idx);
                        self.refresh_status();
                    }
                }
            },
            // Add to playlist ('a')
            (_, KeyCode::Char('a')) => match self.current_view {
                CurrentView::Library => match self.library_focus {
                    LibraryFocus::Artists | LibraryFocus::Albums => {
                        let paths: Vec<PathBuf> = self.library_tracks.iter().map(|t| t.path.clone()).collect();
                        let count = paths.len();
                        let _ = self.client.append(paths);
                        self.set_message(&t!("msg.added_tracks", count = count));
                        self.refresh_playlist();
                    }
                    LibraryFocus::Tracks => {
                        if let Some(idx) = self.track_state.selected() {
                            if let Some(track) = self.library_tracks.get(idx) {
                                let _ = self.client.append(vec![track.path.clone()]);
                                self.set_message(&t!("msg.added", name = track.title.clone()));
                                self.refresh_playlist();
                            }
                        }
                    }
                },
                CurrentView::Files => {
                    if let Some(idx) = self.dir_state.selected() {
                        if let Some(entry) = self.dir_entries.get(idx) {
                            if !entry.is_parent {
                                let _ = self.client.append(vec![entry.path.clone()]);
                                self.set_message(&t!("msg.added", name = entry.name.clone()));
                                self.refresh_playlist();
                            }
                        }
                    }
                }
                CurrentView::Playlist => {}
            },
            // Playlist operations (Delete / Clear)
            (_, KeyCode::Char('d')) | (_, KeyCode::Delete) => {
                if self.current_view == CurrentView::Playlist
                    || (self.current_view == CurrentView::Files && self.active_panel == ActivePanel::Playlist)
                {
                    if let Some(idx) = self.playlist_state.selected() {
                        let _ = self.client.remove(idx);
                        self.refresh_playlist();
                        self.refresh_status();
                    }
                }
            }
            (_, KeyCode::Char('C')) => {
                let _ = self.client.clear();
                self.set_message(&t!("msg.cleared"));
                self.refresh_playlist();
                self.refresh_status();
            }
            _ => {}
        }
    }

    /// Routes mouse events (clicks on lists/tabs/progress-bar and scroll-wheel
    /// navigation) based on the hit regions captured during the last render.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.show_help || self.is_searching {
            return;
        }

        let point = Rect::new(mouse.column, mouse.row, 1, 1);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if hit(self.hit_regions.view_tab_library, mouse) {
                    self.current_view = CurrentView::Library;
                    return;
                }
                if hit(self.hit_regions.view_tab_files, mouse) {
                    self.current_view = CurrentView::Files;
                    return;
                }
                if hit(self.hit_regions.view_tab_playlist, mouse) {
                    self.current_view = CurrentView::Playlist;
                    return;
                }

                if let Some(rect) = self.hit_regions.artists {
                    if hit(Some(rect), mouse) {
                        self.library_focus = LibraryFocus::Artists;
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.artists.len() {
                                self.artist_state.select(Some(idx));
                                self.update_albums();
                                self.update_tracks();
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.hit_regions.albums {
                    if hit(Some(rect), mouse) {
                        self.library_focus = LibraryFocus::Albums;
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.albums.len() {
                                self.album_state.select(Some(idx));
                                self.update_tracks();
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.hit_regions.tracks {
                    if hit(Some(rect), mouse) {
                        self.library_focus = LibraryFocus::Tracks;
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.library_tracks.len() {
                                self.track_state.select(Some(idx));
                                self.play_selected_library_track();
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.hit_regions.directory {
                    if hit(Some(rect), mouse) {
                        self.active_panel = ActivePanel::Directory;
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.dir_entries.len() {
                                self.dir_state.select(Some(idx));
                                self.enter_directory_item();
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.hit_regions.playlist {
                    if hit(Some(rect), mouse) {
                        self.active_panel = ActivePanel::Playlist;
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.playlist_items.len() {
                                self.playlist_state.select(Some(idx));
                                let _ = self.client.play_index(idx);
                                self.refresh_status();
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.hit_regions.playlist_full {
                    if hit(Some(rect), mouse) {
                        if let Some(idx) = row_index_in(rect, mouse) {
                            if idx < self.playlist_items.len() {
                                self.playlist_state.select(Some(idx));
                                let _ = self.client.play_index(idx);
                                self.refresh_status();
                            }
                        }
                        return;
                    }
                }
                // Clicking the progress gauge seeks to that position (jump-to-click)
                if let Some(rect) = self.hit_regions.progress_gauge {
                    if hit(Some(rect), mouse) && rect.width > 0 {
                        let total = self.status.total_sec.unwrap_or(0);
                        if total > 0 {
                            let offset = mouse.column.saturating_sub(rect.x) as u64;
                            let pct = ((offset as f64 / rect.width as f64) * 100.0).clamp(0.0, 100.0) as u8;
                            let _ = self.client.jump_percent(pct);
                            self.refresh_status();
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if hit(self.hit_regions.artists, mouse) {
                    self.move_artist_up();
                    self.update_albums();
                    self.update_tracks();
                } else if hit(self.hit_regions.albums, mouse) {
                    self.move_album_up();
                    self.update_tracks();
                } else if hit(self.hit_regions.tracks, mouse) {
                    self.move_track_up();
                } else if hit(self.hit_regions.directory, mouse) {
                    self.dir_up();
                } else if hit(self.hit_regions.playlist, mouse)
                    || hit(self.hit_regions.playlist_full, mouse)
                {
                    self.playlist_up();
                } else {
                    let _ = self.client.adjust_volume(5);
                    self.refresh_status();
                }
            }
            MouseEventKind::ScrollDown => {
                if hit(self.hit_regions.artists, mouse) {
                    self.move_artist_down();
                    self.update_albums();
                    self.update_tracks();
                } else if hit(self.hit_regions.albums, mouse) {
                    self.move_album_down();
                    self.update_tracks();
                } else if hit(self.hit_regions.tracks, mouse) {
                    self.move_track_down();
                } else if hit(self.hit_regions.directory, mouse) {
                    self.dir_down();
                } else if hit(self.hit_regions.playlist, mouse)
                    || hit(self.hit_regions.playlist_full, mouse)
                {
                    self.playlist_down();
                } else {
                    let _ = self.client.adjust_volume(-5);
                    self.refresh_status();
                }
            }
            _ => {}
        }

        let _ = point; // silence unused warning if no branch above used it directly
    }

    fn play_selected_library_track(&mut self) {
        if let Some(idx) = self.track_state.selected() {
            if let Some(track) = self.library_tracks.get(idx) {
                let path = track.path.clone();
                let title = track.title.clone();
                let _ = self.client.play_path(path);
                self.set_message(&t!("msg.playing", title = title));
                self.refresh_playlist();
                self.refresh_status();
            }
        }
    }

    fn move_artist_up(&mut self) {
        if self.artists.is_empty() {
            return;
        }
        let sel = match self.artist_state.selected() {
            Some(i) => if i > 0 { i - 1 } else { self.artists.len() - 1 },
            None => 0,
        };
        self.artist_state.select(Some(sel));
    }

    fn move_artist_down(&mut self) {
        if self.artists.is_empty() {
            return;
        }
        let sel = match self.artist_state.selected() {
            Some(i) if i + 1 < self.artists.len() => i + 1,
            _ => 0,
        };
        self.artist_state.select(Some(sel));
    }

    fn move_album_up(&mut self) {
        if self.albums.is_empty() {
            return;
        }
        let sel = match self.album_state.selected() {
            Some(i) => if i > 0 { i - 1 } else { self.albums.len() - 1 },
            None => 0,
        };
        self.album_state.select(Some(sel));
    }

    fn move_album_down(&mut self) {
        if self.albums.is_empty() {
            return;
        }
        let sel = match self.album_state.selected() {
            Some(i) if i + 1 < self.albums.len() => i + 1,
            _ => 0,
        };
        self.album_state.select(Some(sel));
    }

    fn move_track_up(&mut self) {
        if self.library_tracks.is_empty() {
            return;
        }
        let sel = match self.track_state.selected() {
            Some(i) => if i > 0 { i - 1 } else { self.library_tracks.len() - 1 },
            None => 0,
        };
        self.track_state.select(Some(sel));
    }

    fn move_track_down(&mut self) {
        if self.library_tracks.is_empty() {
            return;
        }
        let sel = match self.track_state.selected() {
            Some(i) if i + 1 < self.library_tracks.len() => i + 1,
            _ => 0,
        };
        self.track_state.select(Some(sel));
    }

    fn dir_up(&mut self) {
        if self.dir_entries.is_empty() {
            return;
        }
        let sel = match self.dir_state.selected() {
            Some(i) => if i > 0 { i - 1 } else { self.dir_entries.len() - 1 },
            None => 0,
        };
        self.dir_state.select(Some(sel));
    }

    fn dir_down(&mut self) {
        if self.dir_entries.is_empty() {
            return;
        }
        let sel = match self.dir_state.selected() {
            Some(i) if i + 1 < self.dir_entries.len() => i + 1,
            _ => 0,
        };
        self.dir_state.select(Some(sel));
    }

    fn playlist_up(&mut self) {
        if self.playlist_items.is_empty() {
            return;
        }
        let sel = match self.playlist_state.selected() {
            Some(i) => if i > 0 { i - 1 } else { self.playlist_items.len() - 1 },
            None => 0,
        };
        self.playlist_state.select(Some(sel));
    }

    fn playlist_down(&mut self) {
        if self.playlist_items.is_empty() {
            return;
        }
        let sel = match self.playlist_state.selected() {
            Some(i) if i + 1 < self.playlist_items.len() => i + 1,
            _ => 0,
        };
        self.playlist_state.select(Some(sel));
    }

    fn enter_directory_item(&mut self) {
        if let Some(idx) = self.dir_state.selected() {
            if let Some(entry) = self.dir_entries.get(idx).cloned() {
                if entry.is_dir {
                    if let Ok(canonical) = fs::canonicalize(&entry.path) {
                        self.current_dir = canonical;
                    } else {
                        self.current_dir = entry.path;
                    }
                    self.dir_state.select(Some(0));
                    self.refresh_dir();
                } else {
                    let _ = self.client.play_path(entry.path);
                    self.refresh_playlist();
                    self.refresh_status();
                }
            }
        }
    }
}

/// Returns true if the mouse event's column/row falls inside `rect`.
fn hit(rect: Option<Rect>, mouse: MouseEvent) -> bool {
    match rect {
        Some(r) => {
            mouse.column >= r.x
                && mouse.column < r.x + r.width
                && mouse.row >= r.y
                && mouse.row < r.y + r.height
        }
        None => false,
    }
}

/// Converts a mouse row inside a bordered list widget into a 0-based item
/// index, accounting for the top border line.
fn row_index_in(rect: Rect, mouse: MouseEvent) -> Option<usize> {
    if mouse.row <= rect.y {
        return None; // clicked on the border
    }
    Some((mouse.row - rect.y - 1) as usize)
}

pub fn run_ui(mut app: App) -> Result<(), String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture).map_err(|e| e.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let res = main_loop(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen, event::DisableMouseCapture).ok();
    terminal.show_cursor().ok();

    if app.should_shutdown_server {
        let _ = app.client.quit_server();
    }

    res
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    let tick_rate = Duration::from_millis(100);
    let mut last_refresh = std::time::Instant::now();

    while !app.should_quit {
        terminal
            .draw(|f| draw_ui(f, app))
            .map_err(|e| e.to_string())?;

        let timeout = tick_rate
            .checked_sub(last_refresh.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));

        if event::poll(timeout).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => app.handle_key(key),
                Ok(Event::Mouse(mouse)) => app.handle_mouse(mouse),
                _ => {}
            }
        }

        if last_refresh.elapsed() >= tick_rate {
            app.refresh_status();
            last_refresh = std::time::Instant::now();
        }

        if let Some((_, time)) = app.message {
            if time.elapsed() > Duration::from_secs(3) {
                app.message = None;
            }
        }
    }

    Ok(())
}

fn draw_ui(frame: &mut ratatui::Frame, app: &mut App) {
    let size = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top Header with Views
            Constraint::Min(8),    // Main View
            Constraint::Length(5), // Status & Progress
            Constraint::Length(1), // Keybindings Footer
        ])
        .split(size);

    draw_header(frame, chunks[0], app);

    if app.show_lyrics {
        draw_lyrics_view(frame, chunks[1], app);
    } else {
        match app.current_view {
            CurrentView::Library => draw_library_view(frame, chunks[1], app),
            CurrentView::Files => draw_files_view(frame, chunks[1], app),
            CurrentView::Playlist => draw_playlist_view(frame, chunks[1], app),
        }
    }

    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    if app.show_help {
        draw_help_modal(frame, size);
    }
}

fn draw_header(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;

    let tab_style = |active: bool| {
        if active {
            Style::default()
                .fg(Color::Black)
                .bg(theme.parse_color(&theme.border_active))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.parse_color(&theme.border_inactive))
        }
    };

    // Compute horizontal offsets so we can register mouse hit-regions for each tab.
    let prefix = " MORP  [Views: ";
    let lib_label = " 1: Library ";
    let sep1 = " | ";
    let files_label = " 2: Files ";
    let sep2 = " | ";
    let pl_label = " 3: Playlist ";

    let x0 = area.x + prefix.len() as u16;
    let lib_rect = Rect::new(x0, area.y, lib_label.len() as u16, 1);
    let x1 = x0 + lib_label.len() as u16 + sep1.len() as u16;
    let files_rect = Rect::new(x1, area.y, files_label.len() as u16, 1);
    let x2 = x1 + files_label.len() as u16 + sep2.len() as u16;
    let pl_rect = Rect::new(x2, area.y, pl_label.len() as u16, 1);

    app.hit_regions.view_tab_library = Some(lib_rect);
    app.hit_regions.view_tab_files = Some(files_rect);
    app.hit_regions.view_tab_playlist = Some(pl_rect);

    let header_line = Line::from(vec![
        Span::styled(" MORP ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" [Views: "),
        Span::styled(lib_label, tab_style(app.current_view == CurrentView::Library)),
        Span::raw(sep1),
        Span::styled(files_label, tab_style(app.current_view == CurrentView::Files)),
        Span::raw(sep2),
        Span::styled(pl_label, tab_style(app.current_view == CurrentView::Playlist)),
        Span::raw(" ]"),
        Span::raw("  MusicDir: "),
        Span::styled(format!("{}", app.library.music_dir.display()), Style::default().fg(Color::Cyan)),
        Span::raw(if app.show_lyrics { "  [Lyrics: ON]" } else { "" }),
    ]);
    frame.render_widget(Paragraph::new(header_line), area);
}

fn draw_library_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Artists
            Constraint::Percentage(30), // Albums
            Constraint::Percentage(45), // Tracks
        ])
        .split(area);

    app.hit_regions.artists = Some(columns[0]);
    app.hit_regions.albums = Some(columns[1]);
    app.hit_regions.tracks = Some(columns[2]);

    let theme = &app.theme;

    // 1. Artists Column
    let is_artist_focused = app.library_focus == LibraryFocus::Artists;
    let artist_border = theme.border_style(is_artist_focused);
    let artist_items: Vec<ListItem> = app
        .artists
        .iter()
        .map(|art| ListItem::new(Line::from(vec![Span::raw(format!(" {}", art))])))
        .collect();
    let artist_list = List::new(artist_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(artist_border)
                .title(Span::styled(t!("panel.artists").to_string(), artist_border)),
        )
        .highlight_style(theme.selected_item_style(is_artist_focused));
    frame.render_stateful_widget(artist_list, columns[0], &mut app.artist_state);

    // 2. Albums Column
    let is_album_focused = app.library_focus == LibraryFocus::Albums;
    let album_border = theme.border_style(is_album_focused);
    let album_items: Vec<ListItem> = app
        .albums
        .iter()
        .map(|alb| ListItem::new(Line::from(vec![Span::raw(format!(" {}", alb))])))
        .collect();
    let album_list = List::new(album_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(album_border)
                .title(Span::styled(t!("panel.albums").to_string(), album_border)),
        )
        .highlight_style(theme.selected_item_style(is_album_focused));
    frame.render_stateful_widget(album_list, columns[1], &mut app.album_state);

    // 3. Tracks Column
    let is_track_focused = app.library_focus == LibraryFocus::Tracks;
    let track_border = if is_track_focused || app.is_searching {
        theme.border_style(true)
    } else {
        theme.border_style(false)
    };

    let track_title = if app.is_searching {
        t!("panel.tracks_search", query = app.search_query.clone()).to_string()
    } else if !app.search_query.is_empty() {
        t!(
            "panel.tracks_filter",
            query = app.search_query.clone(),
            count = app.library_tracks.len()
        ).to_string()
    } else {
        t!("panel.tracks", count = app.library_tracks.len()).to_string()
    };

    let track_items: Vec<ListItem> = app
        .library_tracks
        .iter()
        .map(|t| {
            let dur = t.formatted_duration();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<30}", t.title), theme.text_style()),
                Span::styled(format!(" - {:<20}", t.artist), Style::default().fg(Color::DarkGray)),
                Span::styled(format!(" [{}]", dur), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let track_list = List::new(track_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(track_border)
                .title(Span::styled(track_title, track_border)),
        )
        .highlight_style(theme.selected_item_style(is_track_focused));
    frame.render_stateful_widget(track_list, columns[2], &mut app.track_state);
}

fn draw_files_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let panel_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    app.hit_regions.directory = Some(panel_chunks[0]);
    app.hit_regions.playlist = Some(panel_chunks[1]);

    let theme = &app.theme;

    // Left: Directory
    let is_dir_active = app.active_panel == ActivePanel::Directory;
    let dir_border = theme.border_style(is_dir_active);

    let dir_title = t!(
        "panel.directory",
        path = app.current_dir.to_string_lossy().to_string(),
        active = if is_dir_active { "*" } else { "" }
    ).to_string();

    let dir_items: Vec<ListItem> = app
        .dir_entries
        .iter()
        .map(|entry| {
            if entry.is_dir {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<30}", entry.name), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {}", entry.name), theme.text_style()),
                ]))
            }
        })
        .collect();

    let dir_list = List::new(dir_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(dir_border)
                .title(Span::styled(dir_title, dir_border)),
        )
        .highlight_style(theme.selected_item_style(is_dir_active));
    frame.render_stateful_widget(dir_list, panel_chunks[0], &mut app.dir_state);

    // Right: Playlist
    let is_pl_active = app.active_panel == ActivePanel::Playlist;
    let pl_border = theme.border_style(is_pl_active);

    let pl_title = t!(
        "panel.playlist",
        count = app.playlist_items.len(),
        active = if is_pl_active { "*" } else { "" }
    ).to_string();

    let pl_items: Vec<ListItem> = app
        .playlist_items
        .iter()
        .enumerate()
        .map(|(idx, song)| {
            let is_current = app.status.current_index == Some(idx);
            let prefix = if is_current { ">" } else { " " };
            let dur = song.formatted_duration();
            let name = song.display_name();

            let style = if is_current {
                theme.highlight_text_style()
            } else {
                theme.text_style()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} {:>2}. ", prefix, idx + 1), style),
                Span::styled(name, style),
                Span::styled(format!(" [{}]", dur), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let pl_list = List::new(pl_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(pl_border)
                .title(Span::styled(pl_title, pl_border)),
        )
        .highlight_style(theme.selected_item_style(is_pl_active));
    frame.render_stateful_widget(pl_list, panel_chunks[1], &mut app.playlist_state);
}

fn draw_playlist_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    app.hit_regions.playlist_full = Some(area);

    let theme = &app.theme;
    let border_style = theme.border_style(true);
    let title = t!("panel.playlist_queue", count = app.playlist_items.len()).to_string();

    let items: Vec<ListItem> = app
        .playlist_items
        .iter()
        .enumerate()
        .map(|(idx, song)| {
            let is_current = app.status.current_index == Some(idx);
            let prefix = if is_current { "▶ " } else { "  " };
            let dur = song.formatted_duration();
            let name = song.display_name();

            let style = if is_current {
                theme.highlight_text_style()
            } else {
                theme.text_style()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} {:>2}. ", prefix, idx + 1), style),
                Span::styled(name, style),
                Span::styled(format!("  [{}]", dur), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(title, border_style)),
        )
        .highlight_style(theme.selected_item_style(true));

    frame.render_stateful_widget(list, area, &mut app.playlist_state);
}

/// Renders synced lyrics (feature 4.1): the current line is highlighted and
/// centered, with a few lines of context above/below scrolling automatically.
fn draw_lyrics_view(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let border_style = theme.border_style(true);

    let title = match &app.status.current_file {
        Some(song) => t!("panel.lyrics_song", song = song.display_name()).to_string(),
        None => t!("panel.lyrics").to_string(),
    };

    if app.lyrics.lines.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                t!("lyrics.no_lyrics").to_string(),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                t!("lyrics.no_lyrics_hint").to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(title, border_style)),
        );
        frame.render_widget(placeholder, area);
        return;
    }

    let current_ms = app.status.current_sec * 1000;
    let current_idx = app.lyrics.current_index(current_ms).unwrap_or(0);

    let visible_height = area.height.saturating_sub(2) as usize; // minus borders
    let half = visible_height / 2;
    let start = current_idx.saturating_sub(half);

    let lines: Vec<Line> = app
        .lyrics
        .lines
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_height.max(1))
        .map(|(idx, l)| {
            let style = if idx == current_idx {
                theme.highlight_text_style()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(l.text.clone(), style)).alignment(ratatui::layout::Alignment::Center)
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .title(Span::styled(title, border_style)),
    );
    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.player_status_border_style())
        .title(Span::styled(t!("panel.player_status").to_string(), theme.player_status_title_style()));

    let inner = status_block.inner(area);
    frame.render_widget(status_block, area);

    let sub_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    app.hit_regions.progress_gauge = Some(sub_chunks[1]);

    // Row 1: State & Title
    let (state_text, state_style) = match app.status.state {
        PlaybackState::Play => (t!("status.play").to_string(), Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD)),
        PlaybackState::Pause => (t!("status.pause").to_string(), Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)),
        PlaybackState::Stop => (t!("status.stop").to_string(), Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)),
    };

    let song_text = match &app.status.current_file {
        Some(song) => song.display_name(),
        None => t!("status.no_song").to_string(),
    };

    let title_line = Line::from(vec![
        Span::styled(state_text, state_style),
        Span::raw(" "),
        Span::styled(song_text, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(title_line), sub_chunks[0]);

    // Row 2: Progress Gauge (click to seek, feature 3 mouse support)
    let cur_sec = app.status.current_sec;
    let tot_sec = app.status.total_sec.unwrap_or(0);
    let ratio = if tot_sec > 0 {
        (cur_sec as f64 / tot_sec as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let cur_str = format!("{:02}:{:02}", cur_sec / 60, cur_sec % 60);
    let tot_str = if tot_sec > 0 {
        format!("{:02}:{:02}", tot_sec / 60, tot_sec % 60)
    } else {
        "--:--".to_string()
    };
    let left_str = if tot_sec >= cur_sec && tot_sec > 0 {
        let left = tot_sec - cur_sec;
        format!("(-{:02}:{:02})", left / 60, left % 60)
    } else {
        String::new()
    };

    let progress_label = format!("{} / {} {}", cur_str, tot_str, left_str);

    let gauge = Gauge::default()
        .gauge_style(theme.progress_style())
        .ratio(ratio)
        .label(Span::styled(progress_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
    frame.render_widget(gauge, sub_chunks[1]);

    // Row 3: Volume & Mode toggles
    let vol_text = t!("status.vol", vol = app.status.volume).to_string();
    let shuffle_text = if app.status.shuffle { t!("status.shuffle_on").to_string() } else { t!("status.shuffle_off").to_string() };
    let repeat_text = if app.status.repeat { t!("status.repeat_on").to_string() } else { t!("status.repeat_off").to_string() };
    let autonext_text = if app.status.autonext { t!("status.autonext_on").to_string() } else { t!("status.autonext_off").to_string() };

    let shuffle_style = if app.status.shuffle {
        theme.highlight_text_style()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let repeat_style = if app.status.repeat {
        theme.highlight_text_style()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let autonext_style = if app.status.autonext {
        theme.highlight_text_style()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let info_line = Line::from(vec![
        Span::styled(vol_text, Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(shuffle_text, shuffle_style),
        Span::raw(" "),
        Span::styled(repeat_text, repeat_style),
        Span::raw(" "),
        Span::styled(autonext_text, autonext_style),
    ]);
    frame.render_widget(Paragraph::new(info_line), sub_chunks[2]);
}

fn draw_footer(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let text = if let Some((msg, _)) = &app.message {
        Line::from(vec![
            Span::styled(format!(" >>> {} ", msg), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ])
    } else {
        Line::from(vec![
            Span::styled("1/2/3", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.view").to_string()),
            Span::styled("L", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.lyrics_key").to_string()),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.col").to_string()),
            Span::styled("/", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.search").to_string()),
            Span::styled("ENTER", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.play").to_string()),
            Span::styled("SPACE", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.pause_key").to_string()),
            Span::styled("a", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.add").to_string()),
            Span::styled("u", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.scan").to_string()),
            Span::styled("Mouse", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.click").to_string()),
            Span::styled("q", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.detach").to_string()),
            Span::styled("Q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(t!("footer.quit").to_string()),
            Span::styled("?:Help", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ])
    };

    frame.render_widget(Paragraph::new(text), area);
}

fn draw_help_modal(frame: &mut ratatui::Frame, area: Rect) {
    let width = 70.min(area.width);
    let height = 26.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let help_text = vec![
        Line::from(vec![Span::styled(t!("help.title").to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled("1 / 2 / 3   ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.switch_view").to_string())]),
        Line::from(vec![Span::styled("L           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.toggle_lyrics").to_string())]),
        Line::from(vec![Span::styled("Tab         ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.switch_col").to_string())]),
        Line::from(vec![Span::styled("/           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.search").to_string())]),
        Line::from(vec![Span::styled("u / F5      ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.rescan").to_string())]),
        Line::from(vec![Span::styled("Enter       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.play_enter").to_string())]),
        Line::from(vec![Span::styled("Space       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.pause_space").to_string())]),
        Line::from(vec![Span::styled("s           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.stop").to_string())]),
        Line::from(vec![Span::styled("n / >       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.next").to_string())]),
        Line::from(vec![Span::styled("b / <       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.prev").to_string())]),
        Line::from(vec![Span::styled("Left / ,    ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.seek_back").to_string())]),
        Line::from(vec![Span::styled("Right / .   ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.seek_fwd").to_string())]),
        Line::from(vec![Span::styled("+ / -       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.volume").to_string())]),
        Line::from(vec![Span::styled("a           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.add").to_string())]),
        Line::from(vec![Span::styled("d / Del     ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.remove").to_string())]),
        Line::from(vec![Span::styled("C           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.clear").to_string())]),
        Line::from(vec![Span::styled("S / R / X   ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.toggles").to_string())]),
        Line::from(vec![Span::styled("Mouse       ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.mouse").to_string())]),
        Line::from(vec![Span::styled("q           ", Style::default().fg(Color::Cyan)), Span::raw(t!("help.detach").to_string())]),
        Line::from(vec![Span::styled("Q           ", Style::default().fg(Color::Red)), Span::raw(t!("help.quit").to_string())]),
        Line::from(""),
        Line::from(vec![Span::styled(t!("help.close").to_string(), Style::default().fg(Color::DarkGray))]),
    ];

    let help_paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
                .title(t!("panel.help_panel").to_string()),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(help_paragraph, modal_area);
}
