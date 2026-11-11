use crate::client::Client;
use crate::domain::{PlaybackState, PlaybackStatus, SongInfo};
use crate::library::{Library, LibraryTrack};
use crate::tags::is_audio_file;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
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
        let mut artists = vec!["[All Artists]".to_string()];
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
        let mut albums = vec!["[All Albums]".to_string()];
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
        let name = self.artists.get(idx)?;
        if name == "[All Artists]" {
            None
        } else {
            Some(name.as_str())
        }
    }

    pub fn selected_album(&self) -> Option<&str> {
        let idx = self.album_state.selected()?;
        let name = self.albums.get(idx)?;
        if name == "[All Albums]" {
            None
        } else {
            Some(name.as_str())
        }
    }

    pub fn rescan_library(&mut self) {
        self.set_message("Rescanning library from disk...");
        self.library = Library::scan(&self.library.music_dir);
        let _ = self.library.save_cache();
        self.init_library_data();
        let count = self.library.tracks.len();
        self.set_message(&format!("Library scan complete! Found {} tracks", count));
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

            dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

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
                            self.set_message("Playing selected library tracks");
                            self.refresh_playlist();
                            self.refresh_status();
                        }
                    }
                    LibraryFocus::Tracks => {
                        if let Some(idx) = self.track_state.selected() {
                            if let Some(track) = self.library_tracks.get(idx) {
                                let _ = self.client.play_path(track.path.clone());
                                self.set_message(&format!("Playing {}", track.title));
                                self.refresh_playlist();
                                self.refresh_status();
                            }
                        }
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
                        self.set_message(&format!("Added {} tracks to playlist", count));
                        self.refresh_playlist();
                    }
                    LibraryFocus::Tracks => {
                        if let Some(idx) = self.track_state.selected() {
                            if let Some(track) = self.library_tracks.get(idx) {
                                let _ = self.client.append(vec![track.path.clone()]);
                                self.set_message(&format!("Added {}", track.title));
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
                                self.set_message(&format!("Added {}", entry.name));
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
                self.set_message("Cleared playlist");
                self.refresh_playlist();
                self.refresh_status();
            }
            _ => {}
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
            Some(i) => if i + 1 < self.artists.len() { i + 1 } else { 0 },
            None => 0,
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
            Some(i) => if i + 1 < self.albums.len() { i + 1 } else { 0 },
            None => 0,
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
            Some(i) => if i + 1 < self.library_tracks.len() { i + 1 } else { 0 },
            None => 0,
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
            Some(i) => if i + 1 < self.dir_entries.len() { i + 1 } else { 0 },
            None => 0,
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
            Some(i) => if i + 1 < self.playlist_items.len() { i + 1 } else { 0 },
            None => 0,
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

pub fn run_ui(mut app: App) -> Result<(), String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| e.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let res = main_loop(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
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
            if let Ok(Event::Key(key)) = event::read() {
                app.handle_key(key);
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

    match app.current_view {
        CurrentView::Library => draw_library_view(frame, chunks[1], app),
        CurrentView::Files => draw_files_view(frame, chunks[1], app),
        CurrentView::Playlist => draw_playlist_view(frame, chunks[1], app),
    }

    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    if app.show_help {
        draw_help_modal(frame, size);
    }
}

fn draw_header(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let lib_style = if app.current_view == CurrentView::Library {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let files_style = if app.current_view == CurrentView::Files {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pl_style = if app.current_view == CurrentView::Playlist {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let header_line = Line::from(vec![
        Span::styled(" MOCP ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" [Views: "),
        Span::styled(" 1: Library ", lib_style),
        Span::raw(" | "),
        Span::styled(" 2: Files ", files_style),
        Span::raw(" | "),
        Span::styled(" 3: Playlist ", pl_style),
        Span::raw(" ]"),
        Span::raw("  MusicDir: "),
        Span::styled(format!("{}", app.library.music_dir.display()), Style::default().fg(Color::Cyan)),
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

    // 1. Artists Column
    let is_artist_focused = app.library_focus == LibraryFocus::Artists;
    let artist_border = if is_artist_focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
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
                .title(Span::styled(" Artists ", artist_border)),
        )
        .highlight_style(
            Style::default()
                .bg(if is_artist_focused { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(artist_list, columns[0], &mut app.artist_state);

    // 2. Albums Column
    let is_album_focused = app.library_focus == LibraryFocus::Albums;
    let album_border = if is_album_focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
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
                .title(Span::styled(" Albums ", album_border)),
        )
        .highlight_style(
            Style::default()
                .bg(if is_album_focused { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(album_list, columns[1], &mut app.album_state);

    // 3. Tracks Column
    let is_track_focused = app.library_focus == LibraryFocus::Tracks;
    let track_border = if is_track_focused || app.is_searching {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let track_title = if app.is_searching {
        format!(" Search: {}_ ", app.search_query)
    } else if !app.search_query.is_empty() {
        format!(" Filter: '{}' ({} tracks) ", app.search_query, app.library_tracks.len())
    } else {
        format!(" Tracks ({} songs) ", app.library_tracks.len())
    };

    let track_items: Vec<ListItem> = app
        .library_tracks
        .iter()
        .map(|t| {
            let dur = t.formatted_duration();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<30}", t.title), Style::default().fg(Color::White)),
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
        .highlight_style(
            Style::default()
                .bg(if is_track_focused { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(track_list, columns[2], &mut app.track_state);
}

fn draw_files_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let panel_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: Directory
    let is_dir_active = app.active_panel == ActivePanel::Directory;
    let dir_border = if is_dir_active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let dir_title = format!(
        " Directory: {} {}",
        app.current_dir.to_string_lossy(),
        if is_dir_active { "*" } else { "" }
    );

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
                    Span::styled(format!(" {}", entry.name), Style::default().fg(Color::White)),
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
        .highlight_style(
            Style::default()
                .bg(if is_dir_active { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(dir_list, panel_chunks[0], &mut app.dir_state);

    // Right: Playlist
    let is_pl_active = app.active_panel == ActivePanel::Playlist;
    let pl_border = if is_pl_active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let pl_title = format!(
        " Playlist ({} tracks) {}",
        app.playlist_items.len(),
        if is_pl_active { "*" } else { "" }
    );

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
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
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
        .highlight_style(
            Style::default()
                .bg(if is_pl_active { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(pl_list, panel_chunks[1], &mut app.playlist_state);
}

fn draw_playlist_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let border_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let title = format!(" Playlist Queue ({} tracks) ", app.playlist_items.len());

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
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
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
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut app.playlist_state);
}

fn draw_status(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(" Player Status ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

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

    // Row 1: State & Title
    let (state_text, state_style) = match app.status.state {
        PlaybackState::Play => (" PLAY ", Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD)),
        PlaybackState::Pause => (" PAUSE ", Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)),
        PlaybackState::Stop => (" STOP ", Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)),
    };

    let song_text = match &app.status.current_file {
        Some(song) => song.display_name(),
        None => "No song playing".to_string(),
    };

    let title_line = Line::from(vec![
        Span::styled(state_text, state_style),
        Span::raw(" "),
        Span::styled(song_text, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(title_line), sub_chunks[0]);

    // Row 2: Progress Gauge
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
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .ratio(ratio)
        .label(Span::styled(progress_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
    frame.render_widget(gauge, sub_chunks[1]);

    // Row 3: Volume & Mode toggles
    let vol_text = format!("Vol: {:>3}%", app.status.volume);
    let shuffle_text = if app.status.shuffle { "[SHUFFLE: ON]" } else { "[SHUFFLE: OFF]" };
    let repeat_text = if app.status.repeat { "[REPEAT: ON]" } else { "[REPEAT: OFF]" };
    let autonext_text = if app.status.autonext { "[AUTONEXT: ON]" } else { "[AUTONEXT: OFF]" };

    let shuffle_style = if app.status.shuffle {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let repeat_style = if app.status.repeat {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let autonext_style = if app.status.autonext {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
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
            Span::raw(":View "),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Col "),
            Span::styled("/", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Search "),
            Span::styled("ENTER", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Play "),
            Span::styled("SPACE", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Pause "),
            Span::styled("a", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Add "),
            Span::styled("u", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Scan "),
            Span::styled("q", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Detach "),
            Span::styled("Q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(":Quit "),
            Span::styled("?:Help", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ])
    };

    frame.render_widget(Paragraph::new(text), area);
}

fn draw_help_modal(frame: &mut ratatui::Frame, area: Rect) {
    let width = 68.min(area.width);
    let height = 24.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let help_text = vec![
        Line::from(vec![Span::styled("MOCP (Music On Console Player) - Keyboard Shortcuts", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled("1 / 2 / 3   ", Style::default().fg(Color::Cyan)), Span::raw("Switch View (1: Library, 2: Files, 3: Playlist)")]),
        Line::from(vec![Span::styled("Tab         ", Style::default().fg(Color::Cyan)), Span::raw("Switch column (Artists/Albums/Tracks) or pane")]),
        Line::from(vec![Span::styled("/           ", Style::default().fg(Color::Cyan)), Span::raw("Search / Filter in Library in real-time")]),
        Line::from(vec![Span::styled("u / F5      ", Style::default().fg(Color::Cyan)), Span::raw("Rescan Music Library from disk")]),
        Line::from(vec![Span::styled("Enter       ", Style::default().fg(Color::Cyan)), Span::raw("Play selected track, album, artist, or file")]),
        Line::from(vec![Span::styled("Space       ", Style::default().fg(Color::Cyan)), Span::raw("Toggle Pause / Unpause")]),
        Line::from(vec![Span::styled("s           ", Style::default().fg(Color::Cyan)), Span::raw("Stop playback")]),
        Line::from(vec![Span::styled("n / >       ", Style::default().fg(Color::Cyan)), Span::raw("Next track")]),
        Line::from(vec![Span::styled("b / <       ", Style::default().fg(Color::Cyan)), Span::raw("Previous track")]),
        Line::from(vec![Span::styled("Left / ,    ", Style::default().fg(Color::Cyan)), Span::raw("Seek backward 5s")]),
        Line::from(vec![Span::styled("Right / .   ", Style::default().fg(Color::Cyan)), Span::raw("Seek forward 5s")]),
        Line::from(vec![Span::styled("+ / -       ", Style::default().fg(Color::Cyan)), Span::raw("Volume Up / Down")]),
        Line::from(vec![Span::styled("a           ", Style::default().fg(Color::Cyan)), Span::raw("Add selected item to playlist")]),
        Line::from(vec![Span::styled("d / Del     ", Style::default().fg(Color::Cyan)), Span::raw("Remove track from playlist")]),
        Line::from(vec![Span::styled("C           ", Style::default().fg(Color::Cyan)), Span::raw("Clear playlist")]),
        Line::from(vec![Span::styled("S / R / X   ", Style::default().fg(Color::Cyan)), Span::raw("Toggle Shuffle / Repeat / AutoNext")]),
        Line::from(vec![Span::styled("q           ", Style::default().fg(Color::Cyan)), Span::raw("Detach TUI (Server keeps playing!)")]),
        Line::from(vec![Span::styled("Q           ", Style::default().fg(Color::Red)), Span::raw("Shutdown Server and Quit")]),
        Line::from(""),
        Line::from(vec![Span::styled("Press 'q', 'Esc', or '?' to close this help screen.", Style::default().fg(Color::DarkGray))]),
    ];

    let help_paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Help "),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(help_paragraph, modal_area);
}
