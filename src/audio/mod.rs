//! Audio playback engine (owned by the audio-core workstream).
//!
//! Contract: the UI thread owns a single `Player` value. All public methods are
//! cheap and non blocking; decoding happens on a worker thread, and the realtime
//! cpal callback only reads from a lock free ring buffer.
//!
//! Layout
//! ------
//! * `Player` — the cheap, `&self` handle used by the UI thread. It owns the
//!   shared atomics/status and a command channel to the engine thread.
//! * Engine thread — owns the `cpal::Stream` (which is `!Send` on some platforms)
//!   and reacts to `Cmd`s. It probes files, picks an output configuration and
//!   spawns one decoder thread per loaded file.
//! * Decoder thread — symphonia 0.6 decodes into interleaved f32, remaps the
//!   channel layout, optionally linearly resamples to the device rate and pushes
//!   into the ring buffer. It polls small atomics for pause/seek/stop requests.
//! * cpal callback — pops from the ring buffer, applies volume, converts to the
//!   device sample format and never allocates, locks or blocks.
//!
//! Only mp3 / m4a (aac+alac) / flac / wav / ogg (vorbis) are decodable here; the
//! resolver transcodes anything else (webm/opus) to m4a before playback.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use crate::model::{PlaybackState, PlaybackStatus};

/// Sentinel stored in `Shared::seek_req` meaning "no seek pending".
const SEEK_NONE: u64 = u64::MAX;
/// Ring buffer length in seconds of output audio.
const BUFFER_SECONDS: usize = 1;
/// How often the engine thread wakes when idle (checks for stream errors/commands).
const ENGINE_TICK: Duration = Duration::from_millis(20);

// ---------------------------------------------------------------------------
// Basic file probing (also used by the resolver to pick a decodable container)
// ---------------------------------------------------------------------------

/// Basic stream information read from a media container header.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioInfo {
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_secs: Option<f64>,
}

/// Probe `path` for a decodable audio stream.
///
/// This only reads container headers and the track's codec parameters, so it is
/// fast enough to call from `Player::load()` directly. It fails when the file is
/// missing/unreadable or when the container/codec is not enabled in symphonia
/// (the resolver uses that to decide whether an ffmpeg transcode is needed).
pub(crate) fn probe_audio(path: &Path) -> Result<AudioInfo, String> {
    let decoder = FileDecoder::open(path)?;
    Ok(AudioInfo {
        sample_rate: decoder.file_rate,
        channels: decoder.file_channels,
        duration_secs: decoder.duration_secs,
    })
}

/// Convenience wrapper: can symphonia decode this file as-is?
pub(crate) fn is_decodable(path: &Path) -> bool {
    probe_audio(path).is_ok()
}

// ---------------------------------------------------------------------------
// Shared state between the UI thread, the engine thread and the cpal callback
// ---------------------------------------------------------------------------

struct Shared {
    /// UI visible status (state/duration/error). Never locked by the callback.
    status: Mutex<PlaybackStatus>,
    /// `f32` bits of the current volume, read by the callback.
    volume: AtomicU32,
    /// Output frames played since the last seek (`OUTPUT` rate, increments in callback).
    position_frames: AtomicU64,
    /// Output sample rate in Hz (written by the engine, read by `status()`).
    out_rate: AtomicU32,
    /// Decoder reached end of stream.
    eof: AtomicBool,
    /// The callback consumed the last buffered sample after EOF — set exactly once.
    ended: AtomicBool,
    /// Latch so `ended` is only raised once per playback/seek epoch.
    ended_latch: AtomicBool,
    /// Output is currently consuming samples.
    playing: AtomicBool,
    /// The UI has asked for playback (`Player::play`) and has not paused/stopped
    /// since. Used to hide transient `Loading`/`Paused` engine states: app.rs
    /// stops syncing its own state while it is `Loading`, so reporting `Loading`
    /// once after a play request can freeze the progress bar.
    play_requested: AtomicBool,
    /// Pending seek in seconds (`f64` bits) or `SEEK_NONE`.
    seek_req: AtomicU64,
    /// Incremented to make the callback flush the ring buffer (seek/load).
    flush_epoch: AtomicU64,
    /// Last `flush_epoch` the audio callback has acknowledged (drained its buffer).
    flushed_epoch: AtomicU64,
    /// Incremented to invalidate/stop the current decoder thread.
    generation: AtomicU64,
    /// Set from the cpal error callback; the engine thread turns it into status.
    stream_error: AtomicBool,
}

impl Shared {
    fn new(volume: f32) -> Self {
        Self {
            status: Mutex::new(PlaybackStatus {
                volume,
                ..Default::default()
            }),
            volume: AtomicU32::new(volume.to_bits()),
            position_frames: AtomicU64::new(0),
            out_rate: AtomicU32::new(44_100),
            eof: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            ended_latch: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            play_requested: AtomicBool::new(false),
            seek_req: AtomicU64::new(SEEK_NONE),
            flush_epoch: AtomicU64::new(0),
            flushed_epoch: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            stream_error: AtomicBool::new(false),
        }
    }

    fn lock_status(&self) -> MutexGuard<'_, PlaybackStatus> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Snapshot used by `Player::status()`. Position/volume/ended are merged in
    /// from the atomics so the value is fresh even between engine ticks.
    ///
    /// Once playback has been requested, transient `Loading`/`Paused` engine
    /// states are reported as `Playing`. The app polls status once per frame and
    /// pauses its own syncing while its state is `Loading`, so exposing `Loading`
    /// after `play()` could leave the progress bar frozen until the next user
    /// action. `Error`/`Idle` are never masked.
    fn snapshot(&self) -> PlaybackStatus {
        let mut status = self.lock_status().clone();
        let rate = self.out_rate.load(Ordering::Acquire).max(1);
        status.position_secs =
            self.position_frames.load(Ordering::Acquire) as f64 / f64::from(rate);
        status.volume = f32::from_bits(self.volume.load(Ordering::Acquire));
        status.ended = self.ended.load(Ordering::Acquire);
        if self.play_requested.load(Ordering::Acquire)
            && matches!(status.state, PlaybackState::Loading | PlaybackState::Paused)
        {
            status.state = PlaybackState::Playing;
        }
        status
    }

    fn set_error(&self, message: impl Into<String>) {
        let message = message.into();
        log::warn!("playback error: {message}");
        let mut status = self.lock_status();
        status.state = PlaybackState::Error;
        status.error = Some(message);
        self.playing.store(false, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------
// Commands handled by the engine thread
// ---------------------------------------------------------------------------

enum Cmd {
    Load(PathBuf),
    Play,
    Pause,
    Seek(f64),
    Stop,
    Shutdown,
}

// ---------------------------------------------------------------------------
// Decoding helpers (symphonia 0.6)
// ---------------------------------------------------------------------------

/// A decoded block of interleaved `f32` audio at the file's native rate/layout.
struct DecodedBlock {
    frames: usize,
    channels: usize,
    rate: u32,
    /// Presentation timestamp of the first frame, when the container knows it.
    /// Used by the seek tests to assert the landing position.
    #[allow(dead_code)]
    start_secs: Option<f64>,
}

/// A single logical audio file opened for sequential decoding.
///
/// The decoder thread wraps this with channel remapping + resampling; tests use
/// it directly to validate decoding and seeking without audio hardware.
struct FileDecoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    file_rate: u32,
    file_channels: usize,
    duration_secs: Option<f64>,
    time_base: Option<symphonia::core::units::TimeBase>,
    /// Frames to discard after a seek so the first output frame lands on target.
    pending_skip_frames: u64,
    /// Scratch buffer reused for the last decoded packet.
    packet_buf: Vec<f32>,
}

impl FileDecoder {
    fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|err| format!("cannot open {path:?}: {err}"))?;
        let media_source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }
        let reader = symphonia::default::get_probe()
            .probe(
                &hint,
                media_source,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|err| format!("unsupported audio file {path:?}: {err}"))?;

        let (track_id, params, time_base, duration_secs) = {
            let track = reader
                .default_track(TrackType::Audio)
                .ok_or_else(|| format!("no audio track in {path:?}"))?;
            let params = match &track.codec_params {
                Some(CodecParameters::Audio(params)) => params.clone(),
                _ => return Err(format!("no decodable audio track in {path:?}")),
            };
            let time_base = track.time_base;
            let duration = track
                .duration
                .and_then(|duration| time_base.and_then(|base| base.calc_duration(duration)))
                .map(|time| time.as_secs_f64());
            (track.id, params, time_base, duration)
        };

        let registered = symphonia::default::get_codecs()
            .get_audio_decoder(params.codec)
            .ok_or_else(|| format!("unsupported audio codec {:?} in {path:?}", params.codec))?;
        let decoder = (registered.factory)(&params, &AudioDecoderOptions::default())
            .map_err(|err| format!("cannot create decoder for {path:?}: {err}"))?;

        let file_rate = params.sample_rate.unwrap_or(44_100).max(1);
        let file_channels = params
            .channels
            .as_ref()
            .map(|c| c.count())
            .unwrap_or(2)
            .max(1);

        Ok(Self {
            reader,
            decoder,
            track_id,
            file_rate,
            file_channels,
            duration_secs,
            time_base,
            pending_skip_frames: 0,
            packet_buf: Vec::new(),
        })
    }

    /// Decode the next packet into `out` (cleared first, interleaved `f32` at the
    /// file's native rate/layout). Returns `None` at end of stream.
    fn next_block(&mut self, out: &mut Vec<f32>) -> Result<Option<DecodedBlock>, String> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::IoError(err)) if err.kind() == ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(err) => return Err(format!("read packet: {err}")),
            };
            if packet.track_id != self.track_id || packet.data.is_empty() {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(err) => return Err(format!("decode: {err}")),
            };
            if decoded.frames() == 0 {
                continue;
            }

            // Convert the planar buffer to interleaved f32 (reusing the scratch vec).
            self.packet_buf.clear();
            decoded.copy_to_vec_interleaved::<f32>(&mut self.packet_buf);
            let channels = decoded.spec().channels().count().max(1);

            let start_secs = self
                .time_base
                .and_then(|base| base.calc_time(packet.pts))
                .map(|time| time.as_secs_f64());

            out.clear();
            out.extend_from_slice(&self.packet_buf);

            let mut frames = decoded.frames();
            if self.pending_skip_frames > 0 && frames > 0 {
                let skip = self.pending_skip_frames.min(frames as u64) as usize;
                out.drain(..skip * channels);
                frames -= skip;
                self.pending_skip_frames -= skip as u64;
            }
            if frames == 0 {
                continue;
            }

            return Ok(Some(DecodedBlock {
                frames,
                channels,
                rate: decoded.spec().rate().max(1),
                start_secs,
            }));
        }
    }

    /// Seek to `seconds`; the decoder is reset and the following blocks discard
    /// the frames between the container's nearest seek point and the target.
    fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let seconds = seconds.max(0.0);
        let time = Time::try_from_secs_f64(seconds)
            .ok_or_else(|| format!("invalid seek position {seconds}"))?;
        let seeked = self
            .reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|err| format!("seek to {seconds:.2}s: {err}"))?;
        self.decoder.reset();

        // SeekMode::Accurate lands at or before the target; drop the difference.
        let mut skip_frames = 0u64;
        if let Some(base) = self.time_base {
            let delta = seeked.required_ts.get() - seeked.actual_ts.get();
            if delta > 0 {
                let delta_secs =
                    delta as f64 * f64::from(base.numer.get()) / f64::from(base.denom.get());
                skip_frames = (delta_secs * f64::from(self.file_rate)).round().max(0.0) as u64;
            }
        }
        self.pending_skip_frames = skip_frames;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Channel mapping + resampling
// ---------------------------------------------------------------------------

/// Remap interleaved `f32` audio from `in_channels` to `out_channels`.
///
/// * equal counts — plain copy
/// * down to mono — average all input channels
/// * up from mono — duplicate
/// * otherwise — take the first `out_channels` (repeat the last one if needed)
fn remap_channels(input: &[f32], in_channels: usize, out_channels: usize, out: &mut Vec<f32>) {
    out.clear();
    let in_channels = in_channels.max(1);
    let out_channels = out_channels.max(1);
    if input.is_empty() {
        return;
    }
    if in_channels == out_channels {
        out.extend_from_slice(input);
        return;
    }
    let frames = input.len() / in_channels;
    out.reserve(frames * out_channels);
    if out_channels == 1 {
        let scale = 1.0 / in_channels as f32;
        for frame in input.chunks_exact(in_channels) {
            out.push(frame.iter().copied().sum::<f32>() * scale);
        }
    } else if in_channels == 1 {
        for &sample in &input[..frames] {
            for _ in 0..out_channels {
                out.push(sample);
            }
        }
    } else {
        for frame in input.chunks_exact(in_channels) {
            for channel in 0..out_channels {
                out.push(frame[channel.min(in_channels - 1)]);
            }
        }
    }
}

/// Minimal linear interpolating resampler for interleaved `f32` audio.
///
/// The resolver prefers native-rate playback, so this is only used when the
/// output device cannot be configured at the file's sample rate.
struct LinearResampler {
    step: f64,
    channels: usize,
    /// Position inside the next input block, in frames (`-1.0..`).
    position: f64,
    /// Last input frame of the previous block (for interpolation across blocks).
    previous: Vec<f32>,
}

impl LinearResampler {
    fn new(from_rate: u32, to_rate: u32, channels: usize) -> Self {
        Self {
            step: f64::from(from_rate.max(1)) / f64::from(to_rate.max(1)),
            channels: channels.max(1),
            position: 0.0,
            previous: vec![0.0; channels.max(1)],
        }
    }

    fn reset(&mut self) {
        self.position = 0.0;
        self.previous.iter_mut().for_each(|sample| *sample = 0.0);
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        out.clear();
        let channels = self.channels;
        let frames = input.len() / channels;
        if frames == 0 {
            return;
        }
        let last_index = frames as f64 - 1.0;
        let mut position = self.position;
        while position < last_index {
            if position < 0.0 {
                // Interpolate between the last frame of the previous block and the
                // first frame of this one.
                let frac = (position + 1.0) as f32;
                for channel in 0..channels {
                    let a = self.previous[channel];
                    let b = input[channel];
                    out.push(a + (b - a) * frac);
                }
            } else {
                let index = position as usize;
                let frac = (position - index as f64) as f32;
                let base = index * channels;
                for channel in 0..channels {
                    let a = input[base + channel];
                    let b = input[base + channels + channel];
                    out.push(a + (b - a) * frac);
                }
            }
            position += self.step;
        }
        self.position = position - frames as f64;
        let last = (frames - 1) * channels;
        self.previous.copy_from_slice(&input[last..last + channels]);
    }
}

// ---------------------------------------------------------------------------
// cpal output
// ---------------------------------------------------------------------------

struct ActiveOutput {
    stream: cpal::Stream,
}

/// Pick an output configuration: prefer the file's sample rate with a float
/// sample format, fall back to the device default (the decoder then resamples).
fn choose_output_config(
    device: &cpal::Device,
    file_rate: u32,
) -> Result<cpal::SupportedStreamConfig, String> {
    let mut best: Option<(u8, u8, cpal::SupportedStreamConfig)> = None;
    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            if range.channels() == 0 {
                continue;
            }
            let min = range.min_sample_rate();
            let max = range.max_sample_rate();
            if file_rate < min || file_rate > max {
                continue;
            }
            let Some(format_score) = sample_format_preference(range.sample_format()) else {
                continue;
            };
            let channel_score = match range.channels() {
                2 => 0u8,
                1 => 1,
                _ => 2,
            };
            let key = (format_score, channel_score);
            if best
                .as_ref()
                .map(|(f, c, _)| key < (*f, *c))
                .unwrap_or(true)
            {
                if let Some(config) = range.try_with_sample_rate(file_rate) {
                    best = Some((format_score, channel_score, config));
                }
            }
        }
    }
    if let Some((_, _, config)) = best {
        return Ok(config);
    }
    let config = device
        .default_output_config()
        .map_err(|err| format!("no output configuration: {err}"))?;
    if sample_format_preference(config.sample_format()).is_none() {
        return Err(format!(
            "unsupported output sample format {:?}",
            config.sample_format()
        ));
    }
    Ok(config)
}

/// Supported output sample formats, lower is better.
fn sample_format_preference(format: SampleFormat) -> Option<u8> {
    match format {
        SampleFormat::F32 => Some(0),
        SampleFormat::I16 => Some(1),
        SampleFormat::U16 => Some(2),
        SampleFormat::I32 => Some(3),
        SampleFormat::U32 => Some(4),
        SampleFormat::F64 => Some(5),
        SampleFormat::I8 => Some(6),
        SampleFormat::U8 => Some(7),
        SampleFormat::I64 => Some(8),
        SampleFormat::U64 => Some(9),
        _ => None,
    }
}

fn open_default_device() -> Option<cpal::Device> {
    // `cpal::default_host()` panics if the host cannot be initialised at all;
    // a missing audio stack must never take the application down.
    std::panic::catch_unwind(|| cpal::default_host().default_output_device())
        .ok()
        .flatten()
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    shared: Arc<Shared>,
    consumer: HeapCons<f32>,
) -> Result<cpal::Stream, cpal::Error> {
    let out_channels = usize::from(config.channels());
    let sample_format = config.sample_format();
    match sample_format {
        SampleFormat::F32 => build_typed::<f32>(device, config, shared, consumer),
        SampleFormat::I16 => build_typed::<i16>(device, config, shared, consumer),
        SampleFormat::U16 => build_typed::<u16>(device, config, shared, consumer),
        SampleFormat::I32 => build_typed::<i32>(device, config, shared, consumer),
        SampleFormat::U32 => build_typed::<u32>(device, config, shared, consumer),
        SampleFormat::F64 => build_typed::<f64>(device, config, shared, consumer),
        SampleFormat::I8 => build_typed::<i8>(device, config, shared, consumer),
        SampleFormat::U8 => build_typed::<u8>(device, config, shared, consumer),
        SampleFormat::I64 => build_typed::<i64>(device, config, shared, consumer),
        SampleFormat::U64 => build_typed::<u64>(device, config, shared, consumer),
        _ => Err(cpal::Error::new(cpal::ErrorKind::UnsupportedConfig)),
    }
    .map(|stream| {
        let _ = out_channels;
        stream
    })
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    shared: Arc<Shared>,
    consumer: HeapCons<f32>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels()).max(1);
    let error_shared = Arc::clone(&shared);
    let mut consumer = consumer;
    let mut epoch = shared.flush_epoch.load(Ordering::Acquire);
    // Allocated once, outside the realtime callback.
    let mut scratch = vec![0.0f32; 4096];
    let stream_config: StreamConfig = config.config();
    device.build_output_stream::<T, _, _>(
        stream_config,
        move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
            fill_output::<T>(
                data,
                &mut consumer,
                &mut scratch,
                &shared,
                &mut epoch,
                channels,
            );
        },
        move |err| {
            log::warn!("audio output stream error: {err}");
            error_shared.stream_error.store(true, Ordering::Release);
        },
        None,
    )
}

/// The realtime audio callback body. No locks, allocations or blocking calls.
fn fill_output<T>(
    data: &mut [T],
    consumer: &mut HeapCons<f32>,
    scratch: &mut [f32],
    shared: &Shared,
    epoch: &mut u64,
    channels: usize,
) where
    T: SizedSample + FromSample<f32>,
{
    if !shared.playing.load(Ordering::Acquire) {
        data.fill(T::EQUILIBRIUM);
        return;
    }

    // A seek/load asks the callback to drop everything buffered so far.
    let current_epoch = shared.flush_epoch.load(Ordering::Acquire);
    if current_epoch != *epoch {
        *epoch = current_epoch;
        consumer.clear();
        shared.flushed_epoch.store(current_epoch, Ordering::Release);
        data.fill(T::EQUILIBRIUM);
        return;
    }

    let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
    let channels = channels.max(1);
    let mut written = 0usize;
    while written < data.len() {
        // Always consume whole frames: a partial frame would swap channels.
        let want = ((data.len() - written).min(scratch.len()) / channels) * channels;
        if want == 0 {
            break;
        }
        let got = consumer.pop_slice(&mut scratch[..want]);
        if got == 0 {
            break;
        }
        for index in 0..got {
            let sample = (scratch[index] * volume).clamp(-1.0, 1.0);
            data[written + index] = T::from_sample(sample);
        }
        written += got;
    }

    let frames = written / channels;
    if frames > 0 {
        shared
            .position_frames
            .fetch_add(frames as u64, Ordering::Relaxed);
    }

    if written < data.len() {
        data[written..].fill(T::EQUILIBRIUM);
        if shared.eof.load(Ordering::Acquire)
            && consumer.is_empty()
            && !shared.ended_latch.swap(true, Ordering::AcqRel)
        {
            shared.ended.store(true, Ordering::Release);
        }
    }
}

// ---------------------------------------------------------------------------
// Decoder thread
// ---------------------------------------------------------------------------

enum DecodeStop {
    Replaced,
    Failed(String),
}

fn spawn_decoder(
    shared: Arc<Shared>,
    path: PathBuf,
    generation: u64,
    producer: HeapProd<f32>,
    out_rate: u32,
    out_channels: usize,
) {
    let error_shared = Arc::clone(&shared);
    let spawned = thread::Builder::new()
        .name("echoport-decoder".to_owned())
        .spawn(move || {
            if let Err(DecodeStop::Failed(message)) =
                decoder_thread(&shared, &path, generation, producer, out_rate, out_channels)
            {
                if shared.generation.load(Ordering::Acquire) == generation {
                    shared.set_error(message);
                }
            }
        });
    if let Err(err) = spawned {
        error_shared.set_error(format!("cannot start decoder thread: {err}"));
    }
}

fn decoder_thread(
    shared: &Shared,
    path: &Path,
    generation: u64,
    producer: HeapProd<f32>,
    out_rate: u32,
    out_channels: usize,
) -> Result<(), DecodeStop> {
    let mut decoder = match FileDecoder::open(path) {
        Ok(decoder) => decoder,
        Err(message) => return Err(DecodeStop::Failed(message)),
    };
    let mut current_rate = decoder.file_rate;
    let mut resampler = if current_rate != out_rate {
        Some(LinearResampler::new(current_rate, out_rate, out_channels))
    } else {
        None
    };

    let mut producer = producer;
    let mut decoded_buf = Vec::new();
    let mut mapped_buf = Vec::new();
    let mut resampled_buf = Vec::new();
    let mut at_eof = false;

    loop {
        if shared.generation.load(Ordering::Acquire) != generation {
            return Err(DecodeStop::Replaced);
        }

        // Seek requests have priority over decoding/pushing.
        let seek_bits = shared.seek_req.swap(SEEK_NONE, Ordering::AcqRel);
        if seek_bits != SEEK_NONE {
            let target = f64::from_bits(seek_bits);
            if let Err(message) = decoder.seek(target) {
                return Err(DecodeStop::Failed(message));
            }
            if let Some(resampler) = resampler.as_mut() {
                resampler.reset();
            }
            at_eof = false;
            shared.eof.store(false, Ordering::Release);
            shared.ended_latch.store(false, Ordering::Release);
            // Ask the realtime callback to drop everything it still has buffered
            // (including samples pushed for the pre-seek position) and wait until
            // it has acknowledged before pushing audio from the new position.
            let epoch = shared.flush_epoch.fetch_add(1, Ordering::AcqRel) + 1;
            let started = std::time::Instant::now();
            while shared.flushed_epoch.load(Ordering::Acquire) < epoch
                && started.elapsed() < Duration::from_millis(150)
            {
                if shared.generation.load(Ordering::Acquire) != generation {
                    return Err(DecodeStop::Replaced);
                }
                thread::sleep(Duration::from_millis(2));
            }
            continue;
        }

        if at_eof {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        if !shared.playing.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(8));
            continue;
        }

        let block = match decoder.next_block(&mut decoded_buf) {
            Ok(Some(block)) => block,
            Ok(None) => {
                at_eof = true;
                shared.eof.store(true, Ordering::Release);
                continue;
            }
            Err(message) => return Err(DecodeStop::Failed(message)),
        };

        // A few containers (e.g. HLS playlists) can change the sample rate
        // mid-stream; rebuild the resampler when that happens.
        if block.rate != current_rate {
            current_rate = block.rate;
            resampler = if current_rate != out_rate {
                Some(LinearResampler::new(current_rate, out_rate, out_channels))
            } else {
                None
            };
        }

        remap_channels(
            &decoded_buf[..block.frames * block.channels],
            block.channels,
            out_channels,
            &mut mapped_buf,
        );

        let output_samples: &[f32] = if let Some(resampler) = resampler.as_mut() {
            resampler.process(&mapped_buf, &mut resampled_buf);
            &resampled_buf
        } else {
            &mapped_buf
        };

        // Push with backpressure; abort early if a seek/replacement arrives so we
        // don't keep filling the buffer with stale audio.
        let mut offset = 0usize;
        while offset < output_samples.len() {
            if shared.generation.load(Ordering::Acquire) != generation {
                return Err(DecodeStop::Replaced);
            }
            if shared.seek_req.load(Ordering::Acquire) != SEEK_NONE {
                break;
            }
            let pushed = producer.push_slice(&output_samples[offset..]);
            if pushed == 0 {
                thread::sleep(Duration::from_millis(3));
            } else {
                offset += pushed;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Engine thread
// ---------------------------------------------------------------------------

fn engine_thread(shared: Arc<Shared>, commands: Receiver<Cmd>) {
    let mut active: Option<ActiveOutput> = None;
    loop {
        match commands.recv_timeout(ENGINE_TICK) {
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::Load(path)) => {
                stop_playback(&shared, &mut active);
                handle_load(&shared, &mut active, path);
            }
            Ok(Cmd::Play) => handle_play(&shared, &mut active),
            Ok(Cmd::Pause) => handle_pause(&shared, &mut active),
            Ok(Cmd::Seek(seconds)) => handle_seek(&shared, seconds),
            Ok(Cmd::Stop) => stop_playback(&shared, &mut active),
            Err(RecvTimeoutError::Timeout) => {}
        }

        if shared.stream_error.swap(false, Ordering::AcqRel) {
            shared.set_error("audio output stream failed");
        }
    }
    stop_playback(&shared, &mut active);
    shared.playing.store(false, Ordering::Release);
}

fn stop_playback(shared: &Shared, active: &mut Option<ActiveOutput>) {
    // Invalidate the decoder thread before dropping the stream so it stops
    // pushing into a buffer nobody consumes.
    shared.generation.fetch_add(1, Ordering::AcqRel);
    shared.playing.store(false, Ordering::Release);
    if let Some(output) = active.take() {
        let _ = output.stream.pause();
        drop(output.stream);
    }
    shared.seek_req.store(SEEK_NONE, Ordering::Release);
    shared.eof.store(false, Ordering::Release);
    shared.ended.store(false, Ordering::Release);
    shared.ended_latch.store(false, Ordering::Release);
    shared.position_frames.store(0, Ordering::Release);
    let mut status = shared.lock_status();
    status.state = PlaybackState::Idle;
    status.position_secs = 0.0;
    status.duration_secs = None;
    status.error = None;
    status.ended = false;
}

fn handle_load(shared: &Arc<Shared>, active: &mut Option<ActiveOutput>, path: PathBuf) {
    let generation = shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
    shared.playing.store(false, Ordering::Release);
    shared.position_frames.store(0, Ordering::Release);
    shared.eof.store(false, Ordering::Release);
    shared.ended.store(false, Ordering::Release);
    shared.ended_latch.store(false, Ordering::Release);
    shared.seek_req.store(SEEK_NONE, Ordering::Release);
    shared.flush_epoch.fetch_add(1, Ordering::AcqRel);
    {
        let mut status = shared.lock_status();
        status.state = PlaybackState::Loading;
        status.position_secs = 0.0;
        status.duration_secs = None;
        status.error = None;
        status.ended = false;
    }

    let info = match probe_audio(&path) {
        Ok(info) => info,
        Err(message) => {
            shared.set_error(message);
            return;
        }
    };

    let Some(device) = open_default_device() else {
        shared.set_error("no audio output device available");
        return;
    };
    let config = match choose_output_config(&device, info.sample_rate) {
        Ok(config) => config,
        Err(message) => {
            shared.set_error(message);
            return;
        }
    };
    let out_rate = config.sample_rate().max(1);
    let out_channels = usize::from(config.channels()).max(1);
    shared.out_rate.store(out_rate, Ordering::Release);

    let capacity = (out_rate as usize * out_channels)
        .saturating_mul(BUFFER_SECONDS)
        .max(4096);
    let (producer, consumer) = HeapRb::<f32>::new(capacity).split();

    spawn_decoder(
        Arc::clone(shared),
        path,
        generation,
        producer,
        out_rate,
        out_channels,
    );

    match build_output_stream(&device, &config, Arc::clone(shared), consumer) {
        Ok(stream) => {
            {
                let mut status = shared.lock_status();
                status.state = PlaybackState::Paused;
                status.duration_secs = info.duration_secs;
                status.error = None;
                status.ended = false;
            }
            *active = Some(ActiveOutput { stream });
        }
        Err(err) => {
            shared.generation.fetch_add(1, Ordering::AcqRel);
            shared.set_error(format!("cannot open audio output: {err}"));
        }
    }
}

fn handle_play(shared: &Shared, active: &mut Option<ActiveOutput>) {
    let Some(output) = active.as_ref() else {
        return;
    };
    // Replay from the start when the track had ended (e.g. single-track loop).
    if shared.ended.load(Ordering::Acquire) {
        shared.ended.store(false, Ordering::Release);
        shared.ended_latch.store(false, Ordering::Release);
        shared.position_frames.store(0, Ordering::Release);
        shared.seek_req.store(0f64.to_bits(), Ordering::Release);
        shared.flush_epoch.fetch_add(1, Ordering::AcqRel);
    }
    if let Err(err) = output.stream.play() {
        shared.set_error(format!("cannot start audio output: {err}"));
        return;
    }
    shared.playing.store(true, Ordering::Release);
    let mut status = shared.lock_status();
    status.state = PlaybackState::Playing;
    status.error = None;
}

fn handle_pause(shared: &Shared, active: &mut Option<ActiveOutput>) {
    shared.playing.store(false, Ordering::Release);
    if let Some(output) = active.as_ref() {
        let _ = output.stream.pause();
    }
    let mut status = shared.lock_status();
    if status.state == PlaybackState::Playing || status.state == PlaybackState::Loading {
        status.state = PlaybackState::Paused;
    }
}

fn handle_seek(shared: &Shared, seconds: f64) {
    let target = if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    };
    let out_rate = shared.out_rate.load(Ordering::Acquire).max(1);
    shared.position_frames.store(
        (target * f64::from(out_rate)).round() as u64,
        Ordering::Release,
    );
    shared.seek_req.store(target.to_bits(), Ordering::Release);
    shared.eof.store(false, Ordering::Release);
    shared.ended.store(false, Ordering::Release);
    shared.ended_latch.store(false, Ordering::Release);
    let mut status = shared.lock_status();
    status.position_secs = target;
    status.ended = false;
}

// ---------------------------------------------------------------------------
// Player — the UI thread handle
// ---------------------------------------------------------------------------

pub struct Player {
    shared: Arc<Shared>,
    commands: Sender<Cmd>,
    engine: Mutex<Option<JoinHandle<()>>>,
}

impl Player {
    /// Create the player. This never fails because of a missing audio device:
    /// the engine reports device problems through `PlaybackStatus::error` when a
    /// track is loaded, so the rest of the application stays usable (and the
    /// demo/screenshot modes work on machines without audio).
    pub fn new() -> anyhow::Result<Self> {
        let shared = Arc::new(Shared::new(0.72));
        let (commands, receiver) = mpsc::channel();
        let engine_shared = Arc::clone(&shared);
        let engine = thread::Builder::new()
            .name("echoport-audio".to_owned())
            .spawn(move || engine_thread(engine_shared, receiver));
        let engine = match engine {
            Ok(handle) => Some(handle),
            Err(err) => {
                log::warn!("cannot start audio engine thread: {err}");
                None
            }
        };
        Ok(Self {
            shared,
            commands,
            engine: Mutex::new(engine),
        })
    }

    /// Load a local audio file, replacing whatever was playing. Starts paused.
    ///
    /// The container header is probed synchronously so `duration_secs` is known
    /// immediately; the actual stream/decoder is prepared on the engine thread.
    pub fn load(&self, path: &Path) -> anyhow::Result<()> {
        self.shared.play_requested.store(false, Ordering::Release);
        let info = probe_audio(path).map_err(anyhow::Error::msg)?;
        {
            let mut status = self.shared.lock_status();
            status.state = PlaybackState::Loading;
            status.position_secs = 0.0;
            status.duration_secs = info.duration_secs;
            status.error = None;
            status.ended = false;
        }
        self.shared
            .out_rate
            .store(info.sample_rate.max(1), Ordering::Release);
        self.shared.position_frames.store(0, Ordering::Release);
        let _ = self.commands.send(Cmd::Load(path.to_path_buf()));
        Ok(())
    }

    pub fn play(&self) {
        self.shared.play_requested.store(true, Ordering::Release);
        let _ = self.commands.send(Cmd::Play);
    }

    pub fn pause(&self) {
        self.shared.play_requested.store(false, Ordering::Release);
        let _ = self.commands.send(Cmd::Pause);
    }

    pub fn toggle(&self) {
        let state = self.shared.lock_status().state;
        match state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.play(),
            _ => {}
        }
    }

    pub fn seek(&self, seconds: f64) {
        let target = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let duration = self.shared.lock_status().duration_secs;
        let target = match duration {
            Some(duration) if duration > 0.0 => target.min(duration),
            _ => target,
        };
        let out_rate = self.shared.out_rate.load(Ordering::Acquire).max(1);
        self.shared.position_frames.store(
            (target * f64::from(out_rate)).round() as u64,
            Ordering::Release,
        );
        self.shared.ended.store(false, Ordering::Release);
        self.shared.lock_status().position_secs = target;
        let _ = self.commands.send(Cmd::Seek(target));
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Release);
        self.shared.lock_status().volume = volume;
    }

    pub fn stop(&self) {
        self.shared.play_requested.store(false, Ordering::Release);
        self.shared.position_frames.store(0, Ordering::Release);
        self.shared.eof.store(false, Ordering::Release);
        self.shared.ended.store(false, Ordering::Release);
        let _ = self.commands.send(Cmd::Stop);
        let mut status = self.shared.lock_status();
        status.state = PlaybackState::Idle;
        status.position_secs = 0.0;
        status.duration_secs = None;
        status.ended = false;
        status.error = None;
    }

    pub fn status(&self) -> PlaybackStatus {
        self.shared.snapshot()
    }

    /// Acknowledge the end-of-track flag once the queue has advanced.
    pub fn clear_ended(&self) {
        self.shared.ended.store(false, Ordering::Release);
    }

    /// Names of available output devices.
    pub fn output_devices() -> Vec<String> {
        let names = std::panic::catch_unwind(|| {
            let host = cpal::default_host();
            let mut names: Vec<String> = host
                .output_devices()
                .map(|devices| {
                    devices
                        .filter_map(|device| device.description().ok().map(|d| d.name().to_owned()))
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names.dedup();
            names
        });
        names.unwrap_or_default()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Cmd::Shutdown);
        let handle = self
            .engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Optional real MP3 supplied by the developer/CI via
    /// `ECHOPORT_TEST_MP3=/path/to/file.mp3`; no machine-specific path is
    /// committed to the repository.
    fn local_file() -> Option<PathBuf> {
        let path = PathBuf::from(std::env::var_os("ECHOPORT_TEST_MP3")?);
        path.is_file().then_some(path)
    }

    #[test]
    fn probes_local_mp3() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let info = probe_audio(&path).expect("probe mp3");
        assert!(info.sample_rate > 0);
        assert!(info.channels > 0);
        let duration = info.duration_secs.expect("duration");
        assert!(duration > 0.5, "unexpected duration {duration}");
        assert!(is_decodable(&path));
    }

    #[test]
    fn decodes_local_mp3_frames() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let mut decoder = FileDecoder::open(&path).expect("open decoder");
        let mut buffer = Vec::new();
        let mut frames = 0u64;
        let mut peak = 0.0f32;
        let mut blocks = 0u32;
        loop {
            match decoder.next_block(&mut buffer).expect("decode") {
                Some(block) => {
                    assert!(block.rate > 0);
                    assert!(block.channels > 0);
                    assert_eq!(buffer.len(), block.frames * block.channels);
                    for &sample in &buffer {
                        peak = peak.max(sample.abs());
                    }
                    frames += block.frames as u64;
                    blocks += 1;
                }
                None => break,
            }
        }
        assert!(blocks > 10, "expected many mp3 frames, got {blocks} blocks");
        assert!(frames > 0);
        assert!(peak > 0.01, "decoded audio is silent (peak {peak})");
    }

    #[test]
    fn seeks_local_mp3() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let mut decoder = FileDecoder::open(&path).expect("open decoder");
        decoder.seek(10.0).expect("seek");
        let mut buffer = Vec::new();
        let block = decoder
            .next_block(&mut buffer)
            .expect("decode after seek")
            .expect("a block after seek");
        let start = block.start_secs.expect("packet timestamp");
        // The seek lands before the target and the decoder drops the difference,
        // so the first block should begin within one mp3 frame (~26 ms).
        assert!(
            (start - 10.0).abs() < 0.15,
            "first block after seek starts at {start:.3}s"
        );
        assert!(!buffer.is_empty());
    }

    #[test]
    fn channel_remap_variants() {
        let mut out = Vec::new();
        remap_channels(&[0.5, -0.5, 0.25, 0.75], 2, 2, &mut out);
        assert_eq!(out, vec![0.5, -0.5, 0.25, 0.75]);

        remap_channels(&[0.5, -0.5, 0.25, 0.75], 2, 1, &mut out);
        assert_eq!(out, vec![0.0, 0.5]);

        remap_channels(&[0.5, -0.5], 1, 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);

        remap_channels(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, 2, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.4, 0.5]);
    }

    #[test]
    fn linear_resampler_length_and_continuity() {
        let mut resampler = LinearResampler::new(44_100, 48_000, 1);
        let input: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin())
            .collect();
        let mut output = Vec::new();
        // Feed in two blocks to exercise the cross-block interpolation state.
        resampler.process(&input[..22_050], &mut output);
        let first_len = output.len();
        let mut second = Vec::new();
        resampler.process(&input[22_050..], &mut second);
        let total = first_len + second.len();
        assert!(
            (total as f64 - 48_000.0).abs() < 16.0,
            "resampled to {total} frames"
        );
        assert!(second.iter().all(|sample| sample.is_finite()));
        assert!(output.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn player_load_reports_duration() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let player = Player::new().expect("player");
        player.load(&path).expect("load");
        let status = player.status();
        let duration = status
            .duration_secs
            .expect("duration immediately after load");
        assert!((duration - 32.78).abs() < 0.5);
        // Loading a missing file is reported synchronously.
        assert!(player
            .load(Path::new("/nonexistent/echoport/file.mp3"))
            .is_err());
        player.stop();
    }

    #[test]
    fn volume_is_clamped_and_visible() {
        let player = Player::new().expect("player");
        player.set_volume(2.5);
        assert_eq!(player.status().volume, 1.0);
        player.set_volume(-1.0);
        assert_eq!(player.status().volume, 0.0);
        player.set_volume(0.42);
        let status = player.status();
        assert!((status.volume - 0.42).abs() < f32::EPSILON);
        assert_eq!(status.state, PlaybackState::Idle);
    }

    /// Write a short PCM WAV file so end-of-stream behaviour can be tested
    /// without shipping a fixture.
    fn write_test_wav(path: &Path, seconds: f32) -> std::io::Result<()> {
        let sample_rate = 8_000u32;
        let channels = 1u16;
        let bits = 16u16;
        let frames = (sample_rate as f32 * seconds) as u32;
        let data_len = frames * u32::from(channels) * 2;
        let mut bytes = Vec::with_capacity((44 + data_len) as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let t = frame as f32 / sample_rate as f32;
            let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 0.2 * f32::from(i16::MAX);
            bytes.extend_from_slice(&(sample as i16).to_le_bytes());
        }
        std::fs::write(path, bytes)
    }

    /// The `ended` flag must fire exactly once after the last buffered sample.
    /// Requires a working audio output device; run manually:
    /// `cargo test --lib audio ended -- --ignored --nocapture`
    #[test]
    #[ignore = "needs audio hardware"]
    fn ended_fires_exactly_once_at_eof() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("short.wav");
        write_test_wav(&path, 0.6).expect("write wav");

        let player = Player::new().expect("player");
        player.load(&path).expect("load");
        let status = player.status();
        let duration = status.duration_secs.expect("wav duration");
        assert!((duration - 0.6).abs() < 0.05, "duration {duration}");
        player.play();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_ended = false;
        while Instant::now() < deadline {
            if player.status().ended {
                saw_ended = true;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(saw_ended, "ended flag never fired: {:?}", player.status());

        // Acknowledge it like the app does after advancing the queue; the latch
        // must keep the flag off until a seek/load starts a new epoch.
        player.clear_ended();
        thread::sleep(Duration::from_millis(350));
        assert!(!player.status().ended, "ended fired more than once");

        player.stop();
    }

    #[test]
    fn status_masks_transient_loading_after_play_requested() {
        let player = Player::new().expect("player");
        // Simulate an engine thread that is still preparing the output stream
        // after load(): app.rs polls status() every frame and would latch on
        // Loading, freezing the progress bar (user bug report).
        player.shared.lock_status().state = PlaybackState::Loading;
        player.shared.play_requested.store(false, Ordering::Release);
        assert_eq!(player.status().state, PlaybackState::Loading);

        player.play();
        assert_eq!(
            player.status().state,
            PlaybackState::Playing,
            "Loading must never leak through status() once play() was requested"
        );

        // Pausing is a deliberate state and must be reported as such.
        player.pause();
        player.shared.lock_status().state = PlaybackState::Paused;
        assert_eq!(player.status().state, PlaybackState::Paused);

        // Error states are never masked either.
        player.shared.play_requested.store(true, Ordering::Release);
        player.shared.lock_status().state = PlaybackState::Error;
        assert_eq!(player.status().state, PlaybackState::Error);
        player.shared.play_requested.store(false, Ordering::Release);

        // Stop clears the play request and reports Idle.
        player.shared.lock_status().state = PlaybackState::Loading;
        player.play();
        assert_eq!(player.status().state, PlaybackState::Playing);
        player.stop();
        player.shared.lock_status().state = PlaybackState::Idle;
        assert_eq!(player.status().state, PlaybackState::Idle);
        assert!(!player.shared.play_requested.load(Ordering::Acquire));
    }

    /// Poll status exactly like app.rs does once per frame and make sure the
    /// position advances continuously through load → play → pause → resume →
    /// seek while `state` never latches on `Loading`.
    #[test]
    #[ignore = "needs audio hardware"]
    fn status_polling_position_advances_like_the_app() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let player = Player::new().expect("player");
        player.load(&path).expect("load");
        let duration = player.status().duration_secs.expect("duration");
        assert!(duration > 30.0);

        player.play();
        let started = Instant::now();
        let mut last_position = 0.0;
        let mut advancing_polls = 0;
        while started.elapsed() < Duration::from_millis(600) {
            let status = player.status();
            assert_ne!(
                status.state,
                PlaybackState::Loading,
                "status exposed Loading after play(): {status:?}"
            );
            assert!(status.error.is_none(), "unexpected error: {status:?}");
            if status.state == PlaybackState::Playing {
                if status.position_secs > last_position + 0.005 {
                    advancing_polls += 1;
                }
                last_position = status.position_secs;
            }
            thread::sleep(Duration::from_millis(16));
        }
        assert!(
            last_position > 0.2,
            "position did not advance: {last_position:.3}s"
        );
        assert!(
            advancing_polls >= 5,
            "position advanced in only {advancing_polls} polls"
        );

        // Pause must freeze the position; resume must continue from there.
        player.pause();
        thread::sleep(Duration::from_millis(150));
        let paused = player.status();
        assert_eq!(paused.state, PlaybackState::Paused);
        thread::sleep(Duration::from_millis(150));
        let after_pause = player.status();
        assert!(
            (after_pause.position_secs - paused.position_secs).abs() < 0.05,
            "position moved while paused: {:.3} -> {:.3}",
            paused.position_secs,
            after_pause.position_secs
        );
        player.play();
        thread::sleep(Duration::from_millis(300));
        let resumed = player.status();
        assert_eq!(resumed.state, PlaybackState::Playing);
        assert!(
            resumed.position_secs > paused.position_secs + 0.1,
            "position did not continue after resume: {:.3} -> {:.3}",
            paused.position_secs,
            resumed.position_secs
        );

        // Seek, then keep advancing from the new position.
        player.seek(15.0);
        thread::sleep(Duration::from_millis(350));
        let sought = player.status();
        assert!(
            (sought.position_secs - 15.0).abs() < 1.5,
            "position after seek: {:.3}",
            sought.position_secs
        );
        assert_eq!(sought.state, PlaybackState::Playing);
        let before = sought.position_secs;
        thread::sleep(Duration::from_millis(250));
        assert!(
            player.status().position_secs > before,
            "position did not advance after seek"
        );
        player.stop();
    }

    /// Requires a working audio output device (PulseAudio/ALSA). Run manually:
    /// `cargo test --lib audio -- --ignored --nocapture`
    #[test]
    #[ignore = "needs audio hardware"]
    fn cpal_smoke_playback() {
        let Some(path) = local_file() else {
            eprintln!("skipping: ECHOPORT_TEST_MP3 not present");
            return;
        };
        let player = Player::new().expect("player");
        player.load(&path).expect("load");
        assert!(
            !Player::output_devices().is_empty(),
            "no output devices listed"
        );
        player.play();
        thread::sleep(Duration::from_millis(700));
        let status = player.status();
        eprintln!("status after 700ms: {status:?}");
        assert_eq!(
            status.state,
            PlaybackState::Playing,
            "not playing: {status:?}"
        );
        assert!(
            status.position_secs > 0.1,
            "position did not advance: {status:?}"
        );
        player.seek(20.0);
        thread::sleep(Duration::from_millis(300));
        let status = player.status();
        assert!(
            (status.position_secs - 20.0).abs() < 1.5,
            "seek went to {}",
            status.position_secs
        );
        player.pause();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(player.status().state, PlaybackState::Paused);
        player.play();
        thread::sleep(Duration::from_millis(400));
        assert_eq!(player.status().state, PlaybackState::Playing);
        player.stop();
        assert_eq!(player.status().state, PlaybackState::Idle);
    }
}
