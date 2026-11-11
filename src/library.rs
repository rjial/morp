use crate::domain::SongInfo;
use crate::tags::{is_audio_file, read_tags};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: Option<u64>,
}

impl LibraryTrack {
    pub fn from_song_info(info: SongInfo) -> Self {
        let artist = if info.artist.trim().is_empty() {
            "Unknown Artist".to_string()
        } else {
            info.artist.trim().to_string()
        };

        let album = if info.album.trim().is_empty() {
            "Unknown Album".to_string()
        } else {
            info.album.trim().to_string()
        };

        Self {
            path: info.path,
            title: info.title,
            artist,
            album,
            duration_sec: info.duration_sec,
        }
    }

    pub fn formatted_duration(&self) -> String {
        match self.duration_sec {
            Some(sec) => format!("{:02}:{:02}", sec / 60, sec % 60),
            None => "--:--".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn to_song_info(&self) -> SongInfo {
        SongInfo {
            path: self.path.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration_sec: self.duration_sec,
            bitrate_kbps: None,
            sample_rate: None,
            channels: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Library {
    pub music_dir: PathBuf,
    pub tracks: Vec<LibraryTrack>,
}

impl Library {
    pub fn get_default_music_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("MUSIC_DIR") {
            PathBuf::from(dir)
        } else if let Some(audio) = dirs::audio_dir() {
            audio
        } else if let Some(home) = dirs::home_dir() {
            let music = home.join("Music");
            if music.exists() {
                music
            } else {
                home
            }
        } else {
            PathBuf::from(".")
        }
    }

    pub fn get_cache_path() -> PathBuf {
        if let Ok(morp_dir) = std::env::var("MORP_DIR") {
            PathBuf::from(morp_dir).join("library_cache.json")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".morp").join("library_cache.json")
        } else {
            PathBuf::from("/tmp/morp_library_cache.json")
        }
    }

    pub fn load_or_scan(music_dir: &Path) -> Self {
        let cache_path = Self::get_cache_path();
        if cache_path.exists() {
            if let Ok(file) = File::open(&cache_path) {
                let reader = BufReader::new(file);
                if let Ok(lib) = serde_json::from_reader::<_, Library>(reader) {
                    if lib.music_dir == music_dir && !lib.tracks.is_empty() {
                        return lib;
                    }
                }
            }
        }

        let lib = Self::scan(music_dir);
        let _ = lib.save_cache();
        lib
    }

    pub fn scan(music_dir: &Path) -> Self {
        let mut tracks = Vec::new();

        if music_dir.exists() && music_dir.is_dir() {
            for entry in WalkDir::new(music_dir)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() && is_audio_file(path) {
                    let song = read_tags(path);
                    tracks.push(LibraryTrack::from_song_info(song));
                }
            }
        }

        // Sort by Artist, Album, Title
        tracks.sort_by(|a, b| {
            a.artist
                .to_lowercase()
                .cmp(&b.artist.to_lowercase())
                .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase()))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        Self {
            music_dir: music_dir.to_path_buf(),
            tracks,
        }
    }

    pub fn save_cache(&self) -> Result<(), String> {
        let cache_path = Self::get_cache_path();
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = File::create(&cache_path).map_err(|e| e.to_string())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn artists(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for t in &self.tracks {
            set.insert(t.artist.clone());
        }
        set.into_iter().collect()
    }

    pub fn albums_for_artist(&self, artist: Option<&str>) -> Vec<String> {
        let mut set = BTreeSet::new();
        for t in &self.tracks {
            if let Some(art) = artist {
                if t.artist.eq_ignore_ascii_case(art) {
                    set.insert(t.album.clone());
                }
            } else {
                set.insert(t.album.clone());
            }
        }
        set.into_iter().collect()
    }

    pub fn tracks_for(&self, artist: Option<&str>, album: Option<&str>) -> Vec<&LibraryTrack> {
        self.tracks
            .iter()
            .filter(|t| {
                if let Some(art) = artist {
                    if !t.artist.eq_ignore_ascii_case(art) {
                        return false;
                    }
                }
                if let Some(alb) = album {
                    if !t.album.eq_ignore_ascii_case(alb) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn search(&self, query: &str) -> Vec<&LibraryTrack> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.tracks.iter().collect();
        }

        let matcher = SkimMatcherV2::default();
        let mut scored_tracks: Vec<(i64, &LibraryTrack)> = self.tracks
            .iter()
            .filter_map(|t| {
                let text = format!("{} {} {}", t.artist, t.album, t.title);
                matcher.fuzzy_match(&text, &q).map(|score| (score, t))
            })
            .collect();

        scored_tracks.sort_by(|a, b| b.0.cmp(&a.0));
        scored_tracks.into_iter().map(|(_, t)| t).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_search_and_filters() {
        let track1 = LibraryTrack {
            path: PathBuf::from("/music/radiohead/okc/airbag.mp3"),
            title: "Airbag".to_string(),
            artist: "Radiohead".to_string(),
            album: "OK Computer".to_string(),
            duration_sec: Some(284),
        };
        let track2 = LibraryTrack {
            path: PathBuf::from("/music/daftpunk/discovery/one_more_time.mp3"),
            title: "One More Time".to_string(),
            artist: "Daft Punk".to_string(),
            album: "Discovery".to_string(),
            duration_sec: Some(320),
        };

        let lib = Library {
            music_dir: PathBuf::from("/music"),
            tracks: vec![track1, track2],
        };

        // Test artists
        let artists = lib.artists();
        assert_eq!(artists.len(), 2);
        assert!(artists.contains(&"Radiohead".to_string()));
        assert!(artists.contains(&"Daft Punk".to_string()));

        // Test albums for artist
        let albums = lib.albums_for_artist(Some("Radiohead"));
        assert_eq!(albums, vec!["OK Computer".to_string()]);

        // Test tracks for
        let tracks = lib.tracks_for(Some("Radiohead"), Some("OK Computer"));
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Airbag");

        // Test search
        let search_results = lib.search("discovery");
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].title, "One More Time");
    }
}

#[cfg(test)]
mod fuzzy_tests {
    use super::*;

    #[test]
    fn test_fuzzy_search_typo_tolerance() {
        let tracks = vec![
            LibraryTrack {
                path: PathBuf::from("/music/radiohead/okc/airbag.mp3"),
                title: "Airbag".to_string(),
                artist: "Radiohead".to_string(),
                album: "OK Computer".to_string(),
                duration_sec: Some(284),
            },
            LibraryTrack {
                path: PathBuf::from("/music/daftpunk/discovery/one_more_time.mp3"),
                title: "One More Time".to_string(),
                artist: "Daft Punk".to_string(),
                album: "Discovery".to_string(),
                duration_sec: Some(320),
            },
        ];

        let lib = Library {
            music_dir: PathBuf::from("/music"),
            tracks,
        };

        // Fuzzy: out-of-order, partial letters should still match via Skim algorithm
        let results = lib.search("rdhead okc");
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Airbag");
    }
}
