use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
}

impl Lyrics {
    pub fn parse_lrc(path: &Path) -> Self {
        let mut lines = Vec::new();
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Self::default(),
        };

        let reader = BufReader::new(file);
        for line in reader.lines().filter_map(|l| l.ok()) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let mut text_start = 0;
            let mut timestamps = Vec::new();

            // A line can start with one or more tags: e.g. [00:12.34][01:12.34]Text
            while text_start < line.len() && line[text_start..].starts_with('[') {
                if let Some(end_idx) = line[text_start..].find(']') {
                    let tag = &line[text_start + 1..text_start + end_idx];
                    if let Some(ms) = parse_timestamp(tag) {
                        timestamps.push(ms);
                    }
                    text_start += end_idx + 1;
                } else {
                    break;
                }
            }

            if text_start <= line.len() {
                let text = line[text_start..].trim().to_string();
                for time_ms in timestamps {
                    lines.push(LyricLine {
                        time_ms,
                        text: text.clone(),
                    });
                }
            }
        }

        lines.sort_by_key(|l| l.time_ms);

        Self { lines }
    }

    pub fn current_index(&self, current_ms: u64) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }

        let mut selected = None;
        for (idx, line) in self.lines.iter().enumerate() {
            if line.time_ms <= current_ms {
                selected = Some(idx);
            } else {
                break;
            }
        }
        selected
    }
}

fn parse_timestamp(tag: &str) -> Option<u64> {
    let parts: Vec<&str> = tag.split(':').collect();
    if parts.len() == 2 {
        let min = parts[0].parse::<u64>().ok()?;
        let sec_part = parts[1];
        let sec_parts: Vec<&str> = sec_part.split('.').collect();
        if sec_parts.len() == 2 {
            let sec = sec_parts[0].parse::<u64>().ok()?;
            let centisec = sec_parts[1].parse::<u64>().ok()?;
            let ms = (min * 60 + sec) * 1000 + centisec * 10;
            return Some(ms);
        } else {
            let sec = sec_part.parse::<u64>().ok()?;
            return Some((min * 60 + sec) * 1000);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_temp_lrc(content: &str) -> PathBuf {
        let unique = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "morp_test_{}_{}.lrc",
            std::process::id(),
            unique
        ));
        let mut f = File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_parse_lrc_basic_lines() {
        let content = "\
[ar:Test Artist]
[ti:Test Title]
[00:00.00]First line
[00:05.50]Second line
[00:12.34]Third line
";
        let path = write_temp_lrc(content);
        let lyrics = Lyrics::parse_lrc(&path);
        std::fs::remove_file(&path).ok();

        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].text, "First line");
        assert_eq!(lyrics.lines[0].time_ms, 0);
        assert_eq!(lyrics.lines[1].time_ms, 5500);
        assert_eq!(lyrics.lines[2].time_ms, 12340);
    }

    #[test]
    fn test_parse_lrc_multiple_timestamps_per_line() {
        let content = "[00:10.00][01:10.00]Repeated chorus line";
        let path = write_temp_lrc(content);
        let lyrics = Lyrics::parse_lrc(&path);
        std::fs::remove_file(&path).ok();

        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.lines[0].time_ms, 10000);
        assert_eq!(lyrics.lines[1].time_ms, 70000);
        assert_eq!(lyrics.lines[0].text, "Repeated chorus line");
    }

    #[test]
    fn test_current_index_tracks_playback_position() {
        let content = "\
[00:00.00]Line A
[00:10.00]Line B
[00:20.00]Line C
";
        let path = write_temp_lrc(content);
        let lyrics = Lyrics::parse_lrc(&path);
        std::fs::remove_file(&path).ok();

        assert_eq!(lyrics.current_index(0), Some(0));
        assert_eq!(lyrics.current_index(5000), Some(0));
        assert_eq!(lyrics.current_index(10000), Some(1));
        assert_eq!(lyrics.current_index(25000), Some(2));
    }

    #[test]
    fn test_missing_file_returns_empty() {
        let lyrics = Lyrics::parse_lrc(Path::new("/nonexistent/path/does_not_exist.lrc"));
        assert!(lyrics.lines.is_empty());
        assert_eq!(lyrics.current_index(1000), None);
    }
}
