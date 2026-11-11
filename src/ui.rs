use crate::client::Client;
use crate::domain::{PlaybackState, PlaybackStatus, SongInfo};
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub active_panel: ActivePanel,
    pub current_dir: PathBuf,
    pub dir_entries: Vec<FileEntry>,
    pub dir_state: ListState,
    pub playlist_items: Vec<SongInfo>,
    pub playlist_state: ListState,
    pub status: PlaybackStatus,
    pub show_help: bool,
    pub should_quit: bool,
    pub should_shutdown_server: bool,
    pub message: Option<(String, std::time::Instant)>,
}

impl App {
    pub fn new(client: Client) -> Self {
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self {
            client,
            active_panel: ActivePanel::Directory,
            current_dir,
            dir_entries: Vec::new(),
            dir_state: ListState::default(),
            playlist_items: Vec::new(),
            playlist_state: ListState::default(),
            status: PlaybackStatus::default(),
            show_help: false,
            should_quit: false,
            should_shutdown_server: false,
            message: None,
        };

        app.refresh_dir();
        app.refresh_status();
        app.refresh_playlist();
        app
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = Some((msg.to_string(), std::time::Instant::now()));
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
                // Detach UI
                self.should_quit = true;
            }
            (_, KeyCode::Char('Q')) => {
                // Quit UI & Shutdown server
                self.should_quit = true;
                self.should_shutdown_server = true;
            }
            (_, KeyCode::Tab) => {
                self.active_panel = match self.active_panel {
                    ActivePanel::Directory => ActivePanel::Playlist,
                    ActivePanel::Playlist => ActivePanel::Directory,
                };
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
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => match self.active_panel {
                ActivePanel::Directory => self.dir_up(),
                ActivePanel::Playlist => self.playlist_up(),
            },
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => match self.active_panel {
                ActivePanel::Directory => self.dir_down(),
                ActivePanel::Playlist => self.playlist_down(),
            },
            (_, KeyCode::PageUp) => match self.active_panel {
                ActivePanel::Directory => {
                    for _ in 0..10 {
                        self.dir_up();
                    }
                }
                ActivePanel::Playlist => {
                    for _ in 0..10 {
                        self.playlist_up();
                    }
                }
            },
            (_, KeyCode::PageDown) => match self.active_panel {
                ActivePanel::Directory => {
                    for _ in 0..10 {
                        self.dir_down();
                    }
                }
                ActivePanel::Playlist => {
                    for _ in 0..10 {
                        self.playlist_down();
                    }
                }
            },
            (_, KeyCode::Home) => match self.active_panel {
                ActivePanel::Directory => {
                    if !self.dir_entries.is_empty() {
                        self.dir_state.select(Some(0));
                    }
                }
                ActivePanel::Playlist => {
                    if !self.playlist_items.is_empty() {
                        self.playlist_state.select(Some(0));
                    }
                }
            },
            (_, KeyCode::End) => match self.active_panel {
                ActivePanel::Directory => {
                    if !self.dir_entries.is_empty() {
                        self.dir_state.select(Some(self.dir_entries.len() - 1));
                    }
                }
                ActivePanel::Playlist => {
                    if !self.playlist_items.is_empty() {
                        self.playlist_state.select(Some(self.playlist_items.len() - 1));
                    }
                }
            },
            // Enter actions
            (_, KeyCode::Enter) => match self.active_panel {
                ActivePanel::Directory => self.enter_directory_item(),
                ActivePanel::Playlist => {
                    if let Some(idx) = self.playlist_state.selected() {
                        let _ = self.client.play_index(idx);
                        self.refresh_status();
                    }
                }
            },
            // Playlist operations
            (_, KeyCode::Char('a')) => {
                if let Some(idx) = self.dir_state.selected() {
                    if let Some(entry) = self.dir_entries.get(idx) {
                        if !entry.is_parent {
                            let _ = self.client.append(vec![entry.path.clone()]);
                            self.set_message(&format!("Added {}", entry.name));
                            self.refresh_playlist();
                        }
                    }
                }
            },
            (_, KeyCode::Char('A')) => {
                // Add all audio files in current directory
                let _ = self.client.append(vec![self.current_dir.clone()]);
                self.set_message("Added all audio files from current directory");
                self.refresh_playlist();
            },
            (_, KeyCode::Char('d')) | (_, KeyCode::Delete) => {
                if self.active_panel == ActivePanel::Playlist {
                    if let Some(idx) = self.playlist_state.selected() {
                        let _ = self.client.remove(idx);
                        self.refresh_playlist();
                        self.refresh_status();
                    }
                }
            },
            (_, KeyCode::Char('C')) => {
                if self.active_panel == ActivePanel::Playlist {
                    let _ = self.client.clear();
                    self.set_message("Cleared playlist");
                    self.refresh_playlist();
                    self.refresh_status();
                }
            },
            _ => {}
        }
    }

    fn dir_up(&mut self) {
        if self.dir_entries.is_empty() {
            return;
        }
        let sel = match self.dir_state.selected() {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    self.dir_entries.len() - 1
                }
            }
            None => 0,
        };
        self.dir_state.select(Some(sel));
    }

    fn dir_down(&mut self) {
        if self.dir_entries.is_empty() {
            return;
        }
        let sel = match self.dir_state.selected() {
            Some(i) => {
                if i + 1 < self.dir_entries.len() {
                    i + 1
                } else {
                    0
                }
            }
            None => 0,
        };
        self.dir_state.select(Some(sel));
    }

    fn playlist_up(&mut self) {
        if self.playlist_items.is_empty() {
            return;
        }
        let sel = match self.playlist_state.selected() {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    self.playlist_items.len() - 1
                }
            }
            None => 0,
        };
        self.playlist_state.select(Some(sel));
    }

    fn playlist_down(&mut self) {
        if self.playlist_items.is_empty() {
            return;
        }
        let sel = match self.playlist_state.selected() {
            Some(i) => {
                if i + 1 < self.playlist_items.len() {
                    i + 1
                } else {
                    0
                }
            }
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
                    // Play this file immediately
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

    // Restore terminal
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

        // Clear expired message after 3 seconds
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

    // Vertical layout: Top Bar, Main Panels, Status Area, Bottom Key Bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top Header
            Constraint::Min(8),    // Split Panels (Directory & Playlist)
            Constraint::Length(5), // Player Status & Progress
            Constraint::Length(1), // Keybindings Footer
        ])
        .split(size);

    draw_header(frame, chunks[0]);
    draw_panels(frame, chunks[1], app);
    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    if app.show_help {
        draw_help_modal(frame, size);
    }
}

fn draw_header(frame: &mut ratatui::Frame, area: Rect) {
    let header_line = Line::from(vec![
        Span::styled(" MOCP ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" Music On Console Player (Rust Rewrite)"),
    ]);
    frame.render_widget(Paragraph::new(header_line), area);
}

fn draw_panels(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let panel_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // 1. Directory Panel (Left)
    let is_dir_active = app.active_panel == ActivePanel::Directory;
    let dir_border_style = if is_dir_active {
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
                .border_style(dir_border_style)
                .title(Span::styled(dir_title, dir_border_style)),
        )
        .highlight_style(
            Style::default()
                .bg(if is_dir_active { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(dir_list, panel_chunks[0], &mut app.dir_state);

    // 2. Playlist Panel (Right)
    let is_pl_active = app.active_panel == ActivePanel::Playlist;
    let pl_border_style = if is_pl_active {
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
                .border_style(pl_border_style)
                .title(Span::styled(pl_title, pl_border_style)),
        )
        .highlight_style(
            Style::default()
                .bg(if is_pl_active { Color::Cyan } else { Color::DarkGray })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(pl_list, panel_chunks[1], &mut app.playlist_state);
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
            Constraint::Length(1), // State + Song Title
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Stats & Controls
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
            Span::styled("ENTER", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Play "),
            Span::styled("SPACE", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Pause "),
            Span::styled("s", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Stop "),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Switch "),
            Span::styled("a", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Add "),
            Span::styled("d", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Del "),
            Span::styled("C", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":Clear "),
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
    let width = 64.min(area.width);
    let height = 22.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let help_text = vec![
        Line::from(vec![Span::styled("MOCP (Music On Console Player) - Keyboard Shortcuts", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(vec![Span::styled("Enter       ", Style::default().fg(Color::Cyan)), Span::raw("Play selected file / Enter directory")]),
        Line::from(vec![Span::styled("Space       ", Style::default().fg(Color::Cyan)), Span::raw("Toggle Pause / Unpause")]),
        Line::from(vec![Span::styled("s           ", Style::default().fg(Color::Cyan)), Span::raw("Stop playback")]),
        Line::from(vec![Span::styled("n / >       ", Style::default().fg(Color::Cyan)), Span::raw("Next track")]),
        Line::from(vec![Span::styled("b / <       ", Style::default().fg(Color::Cyan)), Span::raw("Previous track")]),
        Line::from(vec![Span::styled("Left / ,    ", Style::default().fg(Color::Cyan)), Span::raw("Seek backward 5s")]),
        Line::from(vec![Span::styled("Right / .   ", Style::default().fg(Color::Cyan)), Span::raw("Seek forward 5s")]),
        Line::from(vec![Span::styled("[ / ]       ", Style::default().fg(Color::Cyan)), Span::raw("Seek backward / forward 15s")]),
        Line::from(vec![Span::styled("+ / -       ", Style::default().fg(Color::Cyan)), Span::raw("Volume Up / Down")]),
        Line::from(vec![Span::styled("Tab         ", Style::default().fg(Color::Cyan)), Span::raw("Switch active pane (Directory / Playlist)")]),
        Line::from(vec![Span::styled("a           ", Style::default().fg(Color::Cyan)), Span::raw("Add selected item to playlist")]),
        Line::from(vec![Span::styled("A           ", Style::default().fg(Color::Cyan)), Span::raw("Add all audio in current dir to playlist")]),
        Line::from(vec![Span::styled("d / Del     ", Style::default().fg(Color::Cyan)), Span::raw("Remove track from playlist")]),
        Line::from(vec![Span::styled("C           ", Style::default().fg(Color::Cyan)), Span::raw("Clear playlist")]),
        Line::from(vec![Span::styled("S           ", Style::default().fg(Color::Cyan)), Span::raw("Toggle Shuffle mode")]),
        Line::from(vec![Span::styled("R           ", Style::default().fg(Color::Cyan)), Span::raw("Toggle Repeat mode")]),
        Line::from(vec![Span::styled("X           ", Style::default().fg(Color::Cyan)), Span::raw("Toggle AutoNext mode")]),
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
