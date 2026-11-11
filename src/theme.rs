use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub border_active: String,
    pub border_inactive: String,
    pub title_active: String,
    pub title_inactive: String,
    pub text_normal: String,
    pub text_highlight: String,
    pub text_selected_bg: String,
    pub text_selected_fg: String,
    pub player_status_border: String,
    pub player_status_title: String,
    pub player_progress_fg: String,
    pub player_progress_bg: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_active: "cyan".to_string(),
            border_inactive: "dark_gray".to_string(),
            title_active: "cyan".to_string(),
            title_inactive: "dark_gray".to_string(),
            text_normal: "white".to_string(),
            text_highlight: "green".to_string(),
            text_selected_bg: "cyan".to_string(),
            text_selected_fg: "black".to_string(),
            player_status_border: "yellow".to_string(),
            player_status_title: "yellow".to_string(),
            player_progress_fg: "cyan".to_string(),
            player_progress_bg: "black".to_string(),
        }
    }
}

impl Theme {
    pub fn get_default_path() -> PathBuf {
        if let Ok(morp_dir) = std::env::var("MORP_DIR") {
            PathBuf::from(morp_dir).join("theme.toml")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".morp").join("theme.toml")
        } else {
            PathBuf::from("/tmp/morp_theme.toml")
        }
    }

    pub fn load_or_default() -> Self {
        let path = Self::get_default_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(theme) = toml::from_str::<Theme>(&content) {
                    return theme;
                }
            }
        }
        Self::default()
    }

    pub fn parse_color(&self, s: &str) -> Color {
        let normalized = s.trim().to_lowercase();

        // 1. Basic Colors
        match normalized.as_str() {
            "black" => return Color::Black,
            "red" => return Color::Red,
            "green" => return Color::Green,
            "yellow" => return Color::Yellow,
            "blue" => return Color::Blue,
            "magenta" => return Color::Magenta,
            "cyan" => return Color::Cyan,
            "gray" | "grey" => return Color::Gray,
            "dark_gray" | "dark_grey" => return Color::DarkGray,
            "light_red" => return Color::LightRed,
            "light_green" => return Color::LightGreen,
            "light_yellow" => return Color::LightYellow,
            "light_blue" => return Color::LightBlue,
            "light_magenta" => return Color::LightMagenta,
            "light_cyan" => return Color::LightCyan,
            "white" => return Color::White,
            _ => {}
        }

        // 2. Hex Colors (e.g. #ff5533 or ff5533)
        if normalized.starts_with('#') && normalized.len() == 7 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&normalized[1..3], 16),
                u8::from_str_radix(&normalized[3..5], 16),
                u8::from_str_radix(&normalized[5..7], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        } else if normalized.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&normalized[0..2], 16),
                u8::from_str_radix(&normalized[2..4], 16),
                u8::from_str_radix(&normalized[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }

        // 3. RGB format (e.g. rgb(255, 120, 0))
        if normalized.starts_with("rgb(") && normalized.ends_with(')') {
            let inner = &normalized[4..normalized.len() - 1];
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() == 3 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                ) {
                    return Color::Rgb(r, g, b);
                }
            }
        }

        Color::Reset
    }

    pub fn border_style(&self, active: bool) -> Style {
        let color = if active {
            self.parse_color(&self.border_active)
        } else {
            self.parse_color(&self.border_inactive)
        };
        if active {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        }
    }

    pub fn text_style(&self) -> Style {
        Style::default().fg(self.parse_color(&self.text_normal))
    }

    pub fn highlight_text_style(&self) -> Style {
        Style::default()
            .fg(self.parse_color(&self.text_highlight))
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected_item_style(&self, active: bool) -> Style {
        let bg = if active {
            self.parse_color(&self.text_selected_bg)
        } else {
            self.parse_color(&self.border_inactive)
        };
        Style::default()
            .bg(bg)
            .fg(self.parse_color(&self.text_selected_fg))
            .add_modifier(Modifier::BOLD)
    }

    pub fn player_status_border_style(&self) -> Style {
        Style::default().fg(self.parse_color(&self.player_status_border))
    }

    pub fn player_status_title_style(&self) -> Style {
        Style::default()
            .fg(self.parse_color(&self.player_status_title))
            .add_modifier(Modifier::BOLD)
    }

    pub fn progress_style(&self) -> Style {
        Style::default()
            .fg(self.parse_color(&self.player_progress_fg))
            .bg(self.parse_color(&self.player_progress_bg))
    }
}
