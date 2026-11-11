use crate::domain::SongInfo;
use std::fs::File;
use std::path::Path;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;

pub fn is_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        matches!(
            ext.to_lowercase().as_str(),
            "mp3"
                | "flac"
                | "ogg"
                | "wav"
                | "m4a"
                | "aac"
                | "alac"
                | "wma"
                | "aiff"
                | "opus"
                | "caf"
                | "mka"
        )
    } else {
        false
    }
}

pub fn is_playlist_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        matches!(ext.to_lowercase().as_str(), "m3u" | "m3u8" | "pls")
    } else {
        false
    }
}

pub fn read_tags(path: &Path) -> SongInfo {
    let mut song = SongInfo::new(path.to_path_buf());

    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return song,
    };

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();

    let mut probe_result = match symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts) {
        Ok(probed) => probed,
        Err(_) => return song,
    };

    let populate_tags = |song: &mut SongInfo, tags: &[symphonia::core::meta::Tag]| {
        for tag in tags {
            if let Some(std_key) = tag.std_key {
                match std_key {
                    StandardTagKey::TrackTitle => song.title = tag.value.to_string(),
                    StandardTagKey::Artist => song.artist = tag.value.to_string(),
                    StandardTagKey::Album => song.album = tag.value.to_string(),
                    _ => {}
                }
            } else {
                let key = tag.key.to_lowercase();
                if song.title.is_empty() && (key == "title" || key == "tit2") {
                    song.title = tag.value.to_string();
                } else if song.artist.is_empty() && (key == "artist" || key == "tpe1") {
                    song.artist = tag.value.to_string();
                } else if song.album.is_empty() && (key == "album" || key == "talb") {
                    song.album = tag.value.to_string();
                }
            }
        }
    };

    // 1. Check probe metadata
    if let Some(metadata) = probe_result.metadata.get() {
        if let Some(current) = metadata.current() {
            populate_tags(&mut song, current.tags());
        }
    }

    // 2. Check format metadata
    let mut format = probe_result.format;
    let metadata = format.metadata();
    if let Some(current) = metadata.current() {
        populate_tags(&mut song, current.tags());
    }

    // 3. Check track codec parameters (duration, sample rate, channels)
    if let Some(track) = format.default_track() {
        let params = &track.codec_params;
        song.sample_rate = params.sample_rate;
        song.channels = params.channels.map(|c| c.count() as u16);

        if let (Some(n_frames), Some(tb)) = (params.n_frames, params.time_base) {
            let time = tb.calc_time(n_frames);
            song.duration_sec = Some(time.seconds);
        } else if let (Some(n_frames), Some(sample_rate)) = (params.n_frames, params.sample_rate) {
            if sample_rate > 0 {
                song.duration_sec = Some(n_frames / (sample_rate as u64));
            }
        }
    }

    song
}
