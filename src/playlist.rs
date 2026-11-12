use crate::domain::SongInfo;
use crate::tags::{is_audio_file, is_playlist_file, read_tags};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub items: Vec<SongInfo>,
    pub current_index: Option<usize>,
    pub shuffle: bool,
    pub repeat: bool,
    pub autonext: bool,
    shuffle_order: Vec<usize>,
    shuffle_pos: usize,
}

impl Default for Playlist {
    fn default() -> Self {
        Self::new()
    }
}

impl Playlist {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_index: None,
            shuffle: false,
            repeat: false,
            autonext: true,
            shuffle_order: Vec::new(),
            shuffle_pos: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn current_song(&self) -> Option<&SongInfo> {
        self.current_index.and_then(|idx| self.items.get(idx))
    }

    #[allow(dead_code)]
    pub fn add(&mut self, song: SongInfo) {
        self.items.push(song);
        self.rebuild_shuffle_order();
    }

    pub fn add_path(&mut self, path: &Path) {
        if path.is_dir() {
            let mut found_paths = Vec::new();
            for entry in WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() && is_audio_file(p) {
                    found_paths.push(p.to_path_buf());
                }
            }
            found_paths.sort();
            for p in found_paths {
                self.items.push(read_tags(&p));
            }
        } else if is_playlist_file(path) {
            self.load_playlist_file(path);
        } else if is_audio_file(path) {
            self.items.push(read_tags(path));
        }
        self.rebuild_shuffle_order();
    }

    pub fn append_paths(&mut self, paths: &[PathBuf]) {
        for p in paths {
            self.add_path(p);
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<SongInfo> {
        if index >= self.items.len() {
            return None;
        }

        let removed = self.items.remove(index);

        if let Some(curr) = self.current_index {
            if curr == index {
                if self.items.is_empty() {
                    self.current_index = None;
                } else if curr >= self.items.len() {
                    self.current_index = Some(self.items.len() - 1);
                }
            } else if curr > index {
                self.current_index = Some(curr - 1);
            }
        }

        self.rebuild_shuffle_order();
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = None;
        self.shuffle_order.clear();
        self.shuffle_pos = 0;
    }

    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }

        let item = self.items.remove(from);
        self.items.insert(to, item);

        if let Some(curr) = self.current_index {
            if curr == from {
                self.current_index = Some(to);
            } else if from < curr && to >= curr {
                self.current_index = Some(curr - 1);
            } else if from > curr && to <= curr {
                self.current_index = Some(curr + 1);
            }
        }

        self.rebuild_shuffle_order();
    }

    pub fn play_index(&mut self, index: usize) -> Option<&SongInfo> {
        if index < self.items.len() {
            self.current_index = Some(index);
            if self.shuffle {
                if let Some(pos) = self.shuffle_order.iter().position(|&i| i == index) {
                    self.shuffle_pos = pos;
                }
            }
            self.current_song()
        } else {
            None
        }
    }

    pub fn next(&mut self) -> Option<&SongInfo> {
        if self.items.is_empty() {
            return None;
        }

        if self.shuffle {
            if self.shuffle_order.is_empty() {
                self.rebuild_shuffle_order();
            }
            if self.shuffle_pos + 1 < self.shuffle_order.len() {
                self.shuffle_pos += 1;
                let next_idx = self.shuffle_order[self.shuffle_pos];
                self.current_index = Some(next_idx);
                self.current_song()
            } else if self.repeat {
                self.rebuild_shuffle_order();
                self.shuffle_pos = 0;
                let next_idx = self.shuffle_order[self.shuffle_pos];
                self.current_index = Some(next_idx);
                self.current_song()
            } else {
                None
            }
        } else {
            match self.current_index {
                Some(curr) => {
                    if curr + 1 < self.items.len() {
                        self.current_index = Some(curr + 1);
                        self.current_song()
                    } else if self.repeat {
                        self.current_index = Some(0);
                        self.current_song()
                    } else {
                        None
                    }
                }
                None => {
                    self.current_index = Some(0);
                    self.current_song()
                }
            }
        }
    }

    pub fn prev(&mut self) -> Option<&SongInfo> {
        if self.items.is_empty() {
            return None;
        }

        if self.shuffle {
            if self.shuffle_pos > 0 && self.shuffle_pos < self.shuffle_order.len() {
                self.shuffle_pos -= 1;
                let prev_idx = self.shuffle_order[self.shuffle_pos];
                self.current_index = Some(prev_idx);
                self.current_song()
            } else if self.repeat && !self.shuffle_order.is_empty() {
                self.shuffle_pos = self.shuffle_order.len() - 1;
                let prev_idx = self.shuffle_order[self.shuffle_pos];
                self.current_index = Some(prev_idx);
                self.current_song()
            } else {
                None
            }
        } else {
            match self.current_index {
                Some(curr) => {
                    if curr > 0 {
                        self.current_index = Some(curr - 1);
                        self.current_song()
                    } else if self.repeat {
                        self.current_index = Some(self.items.len() - 1);
                        self.current_song()
                    } else {
                        None
                    }
                }
                None => {
                    self.current_index = Some(0);
                    self.current_song()
                }
            }
        }
    }

    pub fn toggle_shuffle(&mut self) -> bool {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            self.rebuild_shuffle_order();
        }
        self.shuffle
    }

    pub fn toggle_repeat(&mut self) -> bool {
        self.repeat = !self.repeat;
        self.repeat
    }

    pub fn toggle_autonext(&mut self) -> bool {
        self.autonext = !self.autonext;
        self.autonext
    }

    fn rebuild_shuffle_order(&mut self) {
        let len = self.items.len();
        self.shuffle_order = (0..len).collect();
        let mut rng = thread_rng();
        self.shuffle_order.shuffle(&mut rng);
        if let Some(curr) = self.current_index {
            if let Some(pos) = self.shuffle_order.iter().position(|&i| i == curr) {
                self.shuffle_order.swap(0, pos);
                self.shuffle_pos = 0;
            }
        }
    }

    fn load_playlist_file(&mut self, path: &Path) {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return,
        };

        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let track_path = PathBuf::from(line);
            let full_path = if track_path.is_absolute() {
                track_path
            } else {
                parent.join(track_path)
            };

            if full_path.exists() && is_audio_file(&full_path) {
                self.items.push(read_tags(&full_path));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playlist_basic_flow() {
        let mut pl = Playlist::new();
        let song1 = SongInfo::new(PathBuf::from("/music/song1.mp3"));
        let song2 = SongInfo::new(PathBuf::from("/music/song2.mp3"));

        pl.add(song1.clone());
        pl.add(song2.clone());
        assert_eq!(pl.len(), 2);

        // Next
        let next1 = pl.next();
        assert!(next1.is_some());
        assert_eq!(next1.unwrap().path, song1.path);

        let next2 = pl.next();
        assert!(next2.is_some());
        assert_eq!(next2.unwrap().path, song2.path);

        // At end, without repeat -> None
        assert!(pl.next().is_none());

        // With repeat -> wraps to 0
        pl.toggle_repeat();
        assert_eq!(pl.next().unwrap().path, song1.path);
    }
}
