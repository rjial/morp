use crate::domain::PlaybackState;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

#[derive(Debug)]
pub enum AudioCommand {
    Play(PathBuf),
    Pause,
    Resume,
    Stop,
    Seek(i64),
    Jump(u64),
    SetVolume(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEvent {
    TrackFinished,
}

#[derive(Clone)]
pub struct AudioStatusHandle {
    pub state: Arc<Mutex<PlaybackState>>,
    pub current_sec: Arc<AtomicU64>,
    pub total_sec: Arc<AtomicU64>,
    pub volume: Arc<AtomicU32>,
    pub sample_rate: Arc<AtomicU32>,
    pub channels: Arc<AtomicU32>,
    pub bitrate: Arc<AtomicU32>,
}

pub struct AudioEngine {
    cmd_sender: Sender<AudioCommand>,
    pub status: AudioStatusHandle,
}

impl AudioEngine {
    pub fn new(event_sender: Sender<AudioEvent>) -> Self {
        let (cmd_sender, cmd_receiver) = channel();

        let status = AudioStatusHandle {
            state: Arc::new(Mutex::new(PlaybackState::Stop)),
            current_sec: Arc::new(AtomicU64::new(0)),
            total_sec: Arc::new(AtomicU64::new(0)),
            volume: Arc::new(AtomicU32::new(80)),
            sample_rate: Arc::new(AtomicU32::new(0)),
            channels: Arc::new(AtomicU32::new(2)),
            bitrate: Arc::new(AtomicU32::new(0)),
        };

        let status_clone = status.clone();

        thread::spawn(move || {
            run_audio_thread(cmd_receiver, event_sender, status_clone);
        });

        Self { cmd_sender, status }
    }

    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_sender.send(cmd);
    }
}

struct AudioSource {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    sample_buf: Option<SampleBuffer<f32>>,
    samples: Vec<f32>,
    sample_pos: usize,
    samples_played: u64,
    time_base: Option<symphonia::core::units::TimeBase>,
}

fn open_audio_source(path: &Path) -> Result<AudioSource, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .map_err(|e| e.to_string())?;

    let format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "No default audio track found".to_string())?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let time_base = track.codec_params.time_base;

    let dec_opts = DecoderOptions::default();
    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| e.to_string())?;

    Ok(AudioSource {
        format,
        decoder,
        track_id,
        sample_rate,
        channels,
        sample_buf: None,
        samples: Vec::new(),
        sample_pos: 0,
        samples_played: 0,
        time_base,
    })
}

fn run_audio_thread(
    cmd_receiver: Receiver<AudioCommand>,
    event_sender: Sender<AudioEvent>,
    status: AudioStatusHandle,
) {
    // Try initializing CPAL device
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            eprintln!("Warning: No default audio output device available");
            // Run a fallback dummy loop that drains commands
            while let Ok(cmd) = cmd_receiver.recv() {
                if let AudioCommand::Play(_) = cmd {
                    *status.state.lock().unwrap() = PlaybackState::Play;
                } else if let AudioCommand::Stop = cmd {
                    *status.state.lock().unwrap() = PlaybackState::Stop;
                }
            }
            return;
        }
    };

    let config: cpal::SupportedStreamConfig = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Failed to get default audio output config: {}", e);
            return;
        }
    };

    let out_sample_rate = config.sample_rate().0;
    let out_channels = config.channels() as usize;

    // Ring buffer / sample queue shared between decoding and CPAL callback
    let sample_queue = Arc::new(Mutex::new(Vec::<f32>::with_capacity(32768)));
    let sample_queue_clone = Arc::clone(&sample_queue);

    let is_playing = Arc::new(AtomicBool::new(false));
    let is_playing_clone = Arc::clone(&is_playing);

    let volume_clone = Arc::clone(&status.volume);

    // Build CPAL output stream
    let stream: Option<Stream> = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    write_samples_f32(
                        data,
                        &sample_queue_clone,
                        &is_playing_clone,
                        &volume_clone,
                    );
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
            .ok(),
        _ => None,
    };

    if let Some(ref s) = stream {
        let _ = s.play();
    }

    let mut current_source: Option<AudioSource> = None;

    loop {
        // 1. Process any pending commands without blocking if playing, or blocking if stopped
        let has_active_playback = {
            let state = *status.state.lock().unwrap();
            state == PlaybackState::Play
        };

        let cmd_opt = if has_active_playback {
            cmd_receiver.try_recv().ok()
        } else {
            cmd_receiver.recv().ok()
        };

        if let Some(cmd) = cmd_opt {
            match cmd {
                AudioCommand::Play(path) => {
                    match open_audio_source(&path) {
                        Ok(source) => {
                            status.sample_rate.store(source.sample_rate, Ordering::SeqCst);
                            status.channels.store(source.channels as u32, Ordering::SeqCst);
                            status.current_sec.store(0, Ordering::SeqCst);

                            // Estimate total seconds
                            let total = if let (Some(n_frames), Some(tb)) = (
                                source.format.default_track().and_then(|t| t.codec_params.n_frames),
                                source.time_base,
                            ) {
                                tb.calc_time(n_frames).seconds
                            } else {
                                0
                            };
                            status.total_sec.store(total, Ordering::SeqCst);

                            // Estimate bitrate if available
                            if let Some(track) = source.format.default_track() {
                                if let (Some(n_frames), Some(time_base)) = (track.codec_params.n_frames, track.codec_params.time_base) {
                                    let duration = time_base.calc_time(n_frames).seconds;
                                    if duration > 0 {
                                        if let Ok(meta) = fs::metadata(&path) {
                                            let kbps = (meta.len() * 8) / (duration * 1000);
                                            status.bitrate.store(kbps as u32, Ordering::SeqCst);
                                        }
                                    }
                                }
                            }

                            // Clear buffer
                            sample_queue.lock().unwrap().clear();

                            current_source = Some(source);
                            *status.state.lock().unwrap() = PlaybackState::Play;
                            is_playing.store(true, Ordering::SeqCst);
                        }
                        Err(e) => {
                            eprintln!("Failed to open {}: {}", path.display(), e);
                            *status.state.lock().unwrap() = PlaybackState::Stop;
                            is_playing.store(false, Ordering::SeqCst);
                        }
                    }
                }
                AudioCommand::Pause => {
                    *status.state.lock().unwrap() = PlaybackState::Pause;
                    is_playing.store(false, Ordering::SeqCst);
                }
                AudioCommand::Resume => {
                    if current_source.is_some() {
                        *status.state.lock().unwrap() = PlaybackState::Play;
                        is_playing.store(true, Ordering::SeqCst);
                    }
                }
                AudioCommand::Stop => {
                    *status.state.lock().unwrap() = PlaybackState::Stop;
                    is_playing.store(false, Ordering::SeqCst);
                    sample_queue.lock().unwrap().clear();
                    current_source = None;
                    status.current_sec.store(0, Ordering::SeqCst);
                }
                AudioCommand::Seek(delta_sec) => {
                    if let Some(ref mut source) = current_source {
                        let cur = status.current_sec.load(Ordering::SeqCst) as i64;
                        let target = (cur + delta_sec).max(0) as u64;
                        let seek_to = SeekTo::Time {
                            time: Time::from(target as f64),
                            track_id: Some(source.track_id),
                        };
                        if source.format.seek(SeekMode::Coarse, seek_to).is_ok() {
                            source.decoder.reset();
                            source.samples.clear();
                            source.sample_pos = 0;
                            source.samples_played = target * (source.sample_rate as u64) * (source.channels as u64);
                            sample_queue.lock().unwrap().clear();
                            status.current_sec.store(target, Ordering::SeqCst);
                        }
                    }
                }
                AudioCommand::Jump(target_sec) => {
                    if let Some(ref mut source) = current_source {
                        let seek_to = SeekTo::Time {
                            time: Time::from(target_sec as f64),
                            track_id: Some(source.track_id),
                        };
                        if source.format.seek(SeekMode::Coarse, seek_to).is_ok() {
                            source.decoder.reset();
                            source.samples.clear();
                            source.sample_pos = 0;
                            source.samples_played = target_sec * (source.sample_rate as u64) * (source.channels as u64);
                            sample_queue.lock().unwrap().clear();
                            status.current_sec.store(target_sec, Ordering::SeqCst);
                        }
                    }
                }
                AudioCommand::SetVolume(vol) => {
                    status.volume.store(vol.min(100), Ordering::SeqCst);
                }
            }
        }

        // 2. If playing, decode next chunk of audio into sample queue
        let is_active = {
            let st = *status.state.lock().unwrap();
            st == PlaybackState::Play
        };

        if is_active {
            if let Some(ref mut source) = current_source {
                let queue_len = sample_queue.lock().unwrap().len();

                // If buffer is low, decode packets
                if queue_len < 16384 {
                    match decode_next_samples(source, out_sample_rate, out_channels) {
                        Ok(Some(new_samples)) => {
                            let mut q = sample_queue.lock().unwrap();
                            q.extend(new_samples);

                            // Update current playback time
                            let sec = source.samples_played
                                / ((source.sample_rate as u64) * (source.channels as u64).max(1));
                            status.current_sec.store(sec, Ordering::SeqCst);
                        }
                        Ok(None) => {
                            // End of track reached
                            if queue_len == 0 {
                                *status.state.lock().unwrap() = PlaybackState::Stop;
                                is_playing.store(false, Ordering::SeqCst);
                                current_source = None;
                                let _ = event_sender.send(AudioEvent::TrackFinished);
                            } else {
                                thread::sleep(Duration::from_millis(15));
                            }
                        }
                        Err(e) => {
                            eprintln!("Decode error: {}", e);
                            if queue_len == 0 {
                                *status.state.lock().unwrap() = PlaybackState::Stop;
                                is_playing.store(false, Ordering::SeqCst);
                                current_source = None;
                                let _ = event_sender.send(AudioEvent::TrackFinished);
                            }
                        }
                    }
                } else {
                    thread::sleep(Duration::from_millis(15));
                }
            }
        }
    }
}

fn decode_next_samples(
    source: &mut AudioSource,
    out_sample_rate: u32,
    out_channels: usize,
) -> Result<Option<Vec<f32>>, SymphoniaError> {
    loop {
        // If we already have decoded samples from previous packet
        if source.sample_pos < source.samples.len() {
            let chunk_size = 4096.min(source.samples.len() - source.sample_pos);
            let slice = &source.samples[source.sample_pos..source.sample_pos + chunk_size];
            source.sample_pos += chunk_size;
            source.samples_played += chunk_size as u64;

            let converted = resample_and_remix(
                slice,
                source.sample_rate,
                source.channels,
                out_sample_rate,
                out_channels,
            );
            return Ok(Some(converted));
        }

        // Decode next packet
        let packet = match source.format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(SymphoniaError::ResetRequired) => {
                source.decoder.reset();
                continue;
            }
            Err(e) => return Err(e),
        };

        if packet.track_id() != source.track_id {
            continue;
        }

        match source.decoder.decode(&packet) {
            Ok(decoded) => {
                if source.sample_buf.is_none() {
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;
                    source.sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                }

                if let Some(buf) = &mut source.sample_buf {
                    buf.copy_interleaved_ref(decoded);
                    source.samples.clear();
                    source.samples.extend_from_slice(buf.samples());
                    source.sample_pos = 0;
                }
            }
            Err(SymphoniaError::DecodeError(_)) => {
                // Skip decode error in corrupt packet
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

fn resample_and_remix(
    in_samples: &[f32],
    in_rate: u32,
    in_channels: usize,
    out_rate: u32,
    out_channels: usize,
) -> Vec<f32> {
    if in_channels == 0 || out_channels == 0 {
        return Vec::new();
    }

    let in_frames = in_samples.len() / in_channels;
    if in_frames == 0 {
        return Vec::new();
    }

    // Step 1: Resample if rates differ
    let rate_ratio = out_rate as f64 / in_rate as f64;
    let out_frames = (in_frames as f64 * rate_ratio).round() as usize;
    let mut resampled_frames = Vec::with_capacity(out_frames * in_channels);

    if (rate_ratio - 1.0).abs() < 0.001 {
        // Same sample rate, copy directly
        resampled_frames.extend_from_slice(in_samples);
    } else {
        // Linear interpolation
        for i in 0..out_frames {
            let src_idx_f = i as f64 / rate_ratio;
            let src_idx = src_idx_f.floor() as usize;
            let fract = (src_idx_f - src_idx as f64) as f32;

            for ch in 0..in_channels {
                let s0 = in_samples.get(src_idx * in_channels + ch).copied().unwrap_or(0.0);
                let s1 = in_samples.get((src_idx + 1) * in_channels + ch).copied().unwrap_or(s0);
                let s = s0 + (s1 - s0) * fract;
                resampled_frames.push(s);
            }
        }
    }

    let num_frames = resampled_frames.len() / in_channels;
    let mut out_samples = Vec::with_capacity(num_frames * out_channels);

    // Step 2: Channel remixing (mono -> stereo, multi -> stereo, etc.)
    for f in 0..num_frames {
        let frame_offset = f * in_channels;
        match (in_channels, out_channels) {
            (1, 2) => {
                let s = resampled_frames[frame_offset];
                out_samples.push(s);
                out_samples.push(s);
            }
            (2, 2) => {
                out_samples.push(resampled_frames[frame_offset]);
                out_samples.push(resampled_frames[frame_offset + 1]);
            }
            (1, 1) => {
                out_samples.push(resampled_frames[frame_offset]);
            }
            (2, 1) => {
                let left = resampled_frames[frame_offset];
                let right = resampled_frames[frame_offset + 1];
                out_samples.push((left + right) * 0.5);
            }
            _ => {
                // Generic fallback: copy first N channels or duplicate first channel
                for c in 0..out_channels {
                    if c < in_channels {
                        out_samples.push(resampled_frames[frame_offset + c]);
                    } else {
                        out_samples.push(resampled_frames[frame_offset]);
                    }
                }
            }
        }
    }

    out_samples
}

fn write_samples_f32(
    output: &mut [f32],
    queue: &Arc<Mutex<Vec<f32>>>,
    is_playing: &Arc<AtomicBool>,
    volume: &Arc<AtomicU32>,
) {
    if !is_playing.load(Ordering::SeqCst) {
        output.fill(0.0);
        return;
    }

    let vol_percent = volume.load(Ordering::SeqCst) as f32 / 100.0;

    let mut q = queue.lock().unwrap();
    let num_available = q.len().min(output.len());

    for (out, sample) in output[..num_available].iter_mut().zip(q.drain(..num_available)) {
        *out = sample * vol_percent;
    }

    if num_available < output.len() {
        output[num_available..].fill(0.0);
    }
}
