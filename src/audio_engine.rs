//! In-process audio output.
//!
//! Playback does not shell out to a system player - `paplay`/`aplay`, `afplay`, a
//! PowerShell one-liner around `Media.SoundPlayer`. Handing the transport to another
//! process makes it mean something different on each platform: pause becomes killing
//! it, seeking becomes restarting it from a temporary file, and the position comes
//! from wall-clock arithmetic that drifts whenever the player lags. On Windows it also
//! flashes a console and cannot pause at all without losing the clip.
//!
//! Here the samples are decoded once and handed to the operating system's own audio
//! device. Pause stops advancing the cursor, seek moves it, and the position IS the
//! cursor, so all three agree by construction and behave identically everywhere.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(feature = "native-audio")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Decoded audio plus the transport state the callback reads.
///
/// The callback runs on the audio thread and must never block, so everything it
/// touches is either immutable or an atomic.
pub(crate) struct Playing {
    /// Interleaved samples at the DEVICE's rate and channel count, so the callback
    /// only has to copy.
    samples: Vec<f32>,
    channels: usize,
    /// Next frame the callback will emit.
    cursor: AtomicUsize,
    paused: AtomicBool,
}

impl Playing {
    fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels
        }
    }
}

/// A handle to an output stream owned by a dedicated thread.
///
/// The stream itself is NOT `Send` - it is bound to the thread that opened it - so it
/// cannot be stored anywhere shared. Rather than confine the whole transport to one
/// thread, the stream stays parked on its own thread and only the sample slot, which
/// is a plain mutex over atomics, crosses back. That keeps the transport `Send` and
/// lets a process-wide one-shot player exist at all.
///
/// Opening a device costs tens of milliseconds and can briefly duck other audio, so
/// it is opened once and reused; a clip change swaps the buffer under the callback.
#[cfg(feature = "native-audio")]
pub(crate) struct Output {
    current: Arc<Mutex<Option<Arc<Playing>>>>,
    sample_rate: u32,
    channels: usize,
}

#[cfg(feature = "native-audio")]
impl Output {
    /// Open the default device. Fails when there is none, which is a normal state on
    /// a headless machine and must not be treated as an error by the caller.
    pub(crate) fn open() -> Result<Self, String> {
        let current: Arc<Mutex<Option<Arc<Playing>>>> = Arc::new(Mutex::new(None));
        let slot = current.clone();
        // The thread reports what it opened - or why it could not - before parking.
        let (tx, rx) = std::sync::mpsc::channel::<Result<(u32, usize), String>>();
        std::thread::Builder::new()
            .name("audio-out".into())
            .spawn(move || {
                let opened = (|| -> Result<(cpal::Stream, u32, usize), String> {
                    let host = cpal::default_host();
                    let device = host
                        .default_output_device()
                        .ok_or("no audio output device")?;
                    let config = device
                        .default_output_config()
                        .map_err(|e| format!("audio device config: {e}"))?;
                    let sample_rate = config.sample_rate().0;
                    let channels = config.channels() as usize;
                    let err_fn = |e| tracing::warn!("audio stream error: {e}");
                    // The callback is the real-time thread: no allocation, no blocking,
                    // and a failed lock means "emit silence this period" rather than
                    // stalling the device.
                    let stream = match config.sample_format() {
                        cpal::SampleFormat::F32 => device.build_output_stream(
                            &config.into(),
                            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                                fill(out, &slot);
                            },
                            err_fn,
                            None,
                        ),
                        fmt => return Err(format!("unsupported sample format {fmt:?}")),
                    }
                    .map_err(|e| format!("open audio stream: {e}"))?;
                    stream
                        .play()
                        .map_err(|e| format!("start audio stream: {e}"))?;
                    Ok((stream, sample_rate, channels))
                })();
                match opened {
                    Ok((stream, rate, ch)) => {
                        let _ = tx.send(Ok((rate, ch)));
                        // Hold the stream for the life of the process: dropping it
                        // closes the device, and everything else here is shared state.
                        std::mem::forget(stream);
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| format!("spawn audio thread: {e}"))?;
        let (sample_rate, channels) = rx
            .recv()
            .map_err(|_| "the audio thread stopped before reporting".to_string())??;
        Ok(Self {
            current,
            sample_rate,
            channels,
        })
    }

    /// Hand the device a new clip, resampled to its rate and channel count.
    pub(crate) fn set(&self, samples: &[f32], src_rate: u32, src_channels: usize) -> Arc<Playing> {
        let converted = resample(
            samples,
            src_rate,
            src_channels,
            self.sample_rate,
            self.channels,
        );
        let p = Arc::new(Playing {
            samples: converted,
            channels: self.channels,
            cursor: AtomicUsize::new(0),
            paused: AtomicBool::new(false),
        });
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
        p
    }

    pub(crate) fn clear(&self) {
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Copy the next block to the device, or silence when idle, paused or finished.
fn fill(out: &mut [f32], slot: &Arc<Mutex<Option<Arc<Playing>>>>) {
    out.fill(0.0);
    let Ok(guard) = slot.try_lock() else { return };
    let Some(p) = guard.as_ref() else { return };
    if p.paused.load(Ordering::Relaxed) {
        return;
    }
    let frames = p.frames();
    let want = out.len() / p.channels.max(1);
    let start = p.cursor.load(Ordering::Relaxed);
    if start >= frames {
        return;
    }
    let n = want.min(frames - start);
    let from = start * p.channels;
    out[..n * p.channels].copy_from_slice(&p.samples[from..from + n * p.channels]);
    p.cursor.store(start + n, Ordering::Relaxed);
}

/// Linear resampling plus channel mapping.
///
/// Speech models here emit 22-24 kHz mono while a desktop device is usually 48 kHz
/// stereo. Playing the samples untouched would shift the pitch by nearly an octave,
/// which is what makes the rate conversion mandatory rather than an optimisation.
pub(crate) fn resample(
    samples: &[f32],
    src_rate: u32,
    src_ch: usize,
    dst_rate: u32,
    dst_ch: usize,
) -> Vec<f32> {
    if src_ch == 0 || dst_ch == 0 || samples.is_empty() {
        return Vec::new();
    }
    let src_frames = samples.len() / src_ch;
    let ratio = f64::from(dst_rate) / f64::from(src_rate);
    let dst_frames = ((src_frames as f64) * ratio).round() as usize;
    let mut out = vec![0f32; dst_frames * dst_ch];
    for f in 0..dst_frames {
        let pos = f as f64 / ratio;
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(src_frames.saturating_sub(1));
        let t = (pos - i0 as f64) as f32;
        for c in 0..dst_ch {
            // Mono sources feed every output channel; beyond that, channels map
            // straight across and any extra output channel repeats the last input one.
            let sc = if src_ch == 1 { 0 } else { c.min(src_ch - 1) };
            let a = samples[i0.min(src_frames - 1) * src_ch + sc];
            let b = samples[i1 * src_ch + sc];
            out[f * dst_ch + c] = a + (b - a) * t;
        }
    }
    out
}

/// Decode a RIFF/WAVE blob to interleaved f32 with its rate and channel count.
///
/// Handles the 16-bit PCM and 32-bit float forms the server produces; anything else is
/// reported rather than played as noise.
pub(crate) fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32, usize), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let (mut rate, mut channels, mut bits, mut format) = (0u32, 0usize, 0u16, 1u16);
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let sz =
            u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap_or_default()) as usize;
        let body = off + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            format = u16::from_le_bytes(bytes[body..body + 2].try_into().unwrap_or_default());
            channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap_or_default())
                as usize;
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap_or_default());
            bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap_or_default());
        } else if id == b"data" {
            if rate == 0 || channels == 0 {
                return Err("fmt chunk missing before data".into());
            }
            let end = (body + sz).min(bytes.len());
            let data = &bytes[body..end];
            let samples = match (format, bits) {
                (1, 16) => data
                    .chunks_exact(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
                    .collect(),
                (3, 32) => data
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
                (1, 8) => data
                    .iter()
                    .map(|b| (f32::from(*b) - 128.0) / 128.0)
                    .collect(),
                _ => return Err(format!("unsupported WAV format {format} at {bits} bits")),
            };
            return Ok((samples, rate, channels));
        }
        off = body + sz + (sz & 1);
    }
    Err("no data chunk".into())
}

/// Transport over the output device, holding the cursor for the clip being played.
pub(crate) struct Transport {
    #[cfg(feature = "native-audio")]
    output: Option<Output>,
    playing: Option<Arc<Playing>>,
    rate: u32,
    duration_s: f32,
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport {
    pub(crate) fn new() -> Self {
        // A missing device is not an error until something is actually played, so the
        // GUI still starts on a machine with no sound card.
        #[cfg(feature = "native-audio")]
        let output = match Output::open() {
            Ok(o) => Some(o),
            Err(e) => {
                tracing::warn!("audio output unavailable: {e}");
                None
            }
        };
        Self {
            #[cfg(feature = "native-audio")]
            output,
            playing: None,
            rate: 0,
            duration_s: 0.0,
        }
    }

    #[cfg(not(feature = "native-audio"))]
    pub(crate) fn start(&mut self, _wav: &[u8]) -> Result<(), String> {
        Err("built without the native-audio feature".into())
    }

    #[cfg(feature = "native-audio")]
    pub(crate) fn start(&mut self, wav: &[u8]) -> Result<(), String> {
        let out = self.output.as_ref().ok_or("no audio output device")?;
        let (samples, rate, channels) = decode_wav(wav)?;
        self.duration_s = if rate == 0 || channels == 0 {
            0.0
        } else {
            (samples.len() / channels) as f32 / rate as f32
        };
        self.rate = out.sample_rate;
        self.playing = Some(out.set(&samples, rate, channels));
        Ok(())
    }

    /// Whether an output device was found at all. The play path already reports a
    /// missing device through the error `start` returns, so this exists for the one
    /// test that must SKIP rather than fail on a host with no sound card.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_device(&self) -> bool {
        #[cfg(feature = "native-audio")]
        {
            self.output.is_some()
        }
        #[cfg(not(feature = "native-audio"))]
        {
            false
        }
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        if let Some(p) = &self.playing {
            p.paused.store(paused, Ordering::Relaxed);
        }
    }

    pub(crate) fn stop(&mut self) {
        #[cfg(feature = "native-audio")]
        if let Some(o) = &self.output {
            o.clear();
        }
        self.playing = None;
        self.duration_s = 0.0;
    }

    pub(crate) fn seek(&self, seconds: f32) {
        if let (Some(p), rate) = (&self.playing, self.rate) {
            if rate > 0 {
                let frame = (seconds.max(0.0) * rate as f32) as usize;
                p.cursor.store(frame.min(p.frames()), Ordering::Relaxed);
            }
        }
    }

    pub(crate) fn position(&self) -> f32 {
        match (&self.playing, self.rate) {
            (Some(p), r) if r > 0 => p.cursor.load(Ordering::Relaxed) as f32 / r as f32,
            _ => 0.0,
        }
    }

    pub(crate) fn duration(&self) -> f32 {
        self.duration_s
    }

    /// True once the cursor has reached the end.
    pub(crate) fn finished(&self) -> bool {
        match &self.playing {
            Some(p) => p.cursor.load(Ordering::Relaxed) >= p.frames(),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    //! The ignored cases here need real weights, a device, or a reference dump on
    //! this machine; nothing about them is automatic. Run one by name with
    //!   cargo test --release -p atelier --lib NAME -- --ignored --nocapture
    use super::*;

    /// THE device gate: with a real output device, the operating system must actually
    /// PULL samples, and it must pull them at the clip's own rate.
    ///
    /// Every other test here runs without a device and so proves only that the buffers
    /// are right. This one proves the stream is live: the cursor is advanced solely by
    /// the audio callback, so if it moves, the device is consuming audio, and if it
    /// moves at the right speed, the rate conversion agreed with the hardware. A wrong
    /// rate is otherwise inaudible in a test and merely sounds wrong to a human.
    #[test]
    #[ignore = "needs a real audio output device"]
    fn the_device_consumes_samples_in_real_time() {
        let mut t = Transport::new();
        if !t.has_device() {
            println!("no output device on this host; skipping");
            return;
        }
        // One second of a quiet 440 Hz tone at 24 kHz mono, the shape the speech
        // models emit.
        let rate = 24_000u32;
        let frames = rate as usize;
        let mut wav = wav16(rate, 1, 0);
        let mut pcm = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v = ((i as f32 / rate as f32) * 440.0 * std::f32::consts::TAU).sin() * 0.05;
            pcm.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
        }
        // Patch the header's two length fields for the real payload.
        let data_len = pcm.len() as u32;
        let riff = 36 + data_len;
        wav[4..8].copy_from_slice(&riff.to_le_bytes());
        let dpos = wav.len() - 4;
        wav[dpos..].copy_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(&pcm);

        t.start(&wav).expect("start playback");
        assert!(
            (t.duration() - 1.0).abs() < 0.01,
            "duration {}",
            t.duration()
        );

        std::thread::sleep(std::time::Duration::from_millis(300));
        let played = t.position();
        println!("after 300 ms the device had consumed {played:.3} s");
        assert!(
            played > 0.15,
            "the device is not pulling samples (position {played:.3})"
        );
        assert!(
            played < 0.60,
            "playback ran too fast - the rate conversion disagrees with \
                                the device ({played:.3} s in 300 ms)"
        );

        // Pause must freeze the cursor, not merely mute it.
        t.set_paused(true);
        let at_pause = t.position();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(
            (t.position() - at_pause).abs() < 1e-3,
            "pause did not stop the cursor ({at_pause:.3} -> {:.3})",
            t.position()
        );

        // And seeking moves it where asked.
        t.set_paused(false);
        t.seek(0.8);
        let after_seek = t.position();
        assert!(
            (after_seek - 0.8).abs() < 0.05,
            "seek landed at {after_seek:.3}"
        );
        t.stop();
    }

    fn wav16(rate: u32, channels: u16, frames: usize) -> Vec<u8> {
        let data_len = frames * channels as usize * 2;
        let mut v = Vec::with_capacity(44 + data_len);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        v.extend_from_slice(&(channels * 2).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data_len as u32).to_le_bytes());
        for i in 0..frames {
            for _ in 0..channels {
                let s = ((i as f32 / frames as f32) * 30000.0) as i16;
                v.extend_from_slice(&s.to_le_bytes());
            }
        }
        v
    }

    #[test]
    fn decodes_16_bit_pcm_with_its_rate_and_channels() {
        let (s, rate, ch) = decode_wav(&wav16(24000, 1, 100)).expect("decode");
        assert_eq!((rate, ch, s.len()), (24000, 1, 100));
        assert!(
            s.iter().all(|v| (-1.0..=1.0).contains(v)),
            "samples must be normalised"
        );
    }

    #[test]
    fn rejects_a_blob_that_is_not_a_wav() {
        assert!(decode_wav(b"not audio at all").is_err());
        assert!(decode_wav(&[]).is_err());
    }

    /// Rate conversion is not cosmetic: a 24 kHz clip played at 48 kHz without it
    /// finishes in half the time and an octave high.
    #[test]
    fn resampling_preserves_duration_and_maps_mono_to_every_channel() {
        let mono: Vec<f32> = (0..240).map(|i| (i as f32 / 240.0) * 2.0 - 1.0).collect();
        let out = resample(&mono, 24000, 1, 48000, 2);
        assert_eq!(
            out.len(),
            480 * 2,
            "duration must be preserved across the rate change"
        );
        // Mono feeds both channels identically.
        for f in 0..480 {
            assert!((out[f * 2] - out[f * 2 + 1]).abs() < 1e-6);
        }
        // And the ramp is still monotonic, i.e. it interpolated rather than shuffled.
        assert!(out[0] < out[479 * 2], "the signal was not preserved");
    }

    #[test]
    fn resampling_at_the_same_rate_is_a_passthrough() {
        let mono: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
        let out = resample(&mono, 48000, 1, 48000, 1);
        assert_eq!(out.len(), 64);
        for (a, b) in out.iter().zip(&mono) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    /// The callback must be safe to run with no clip, while paused, and past the end -
    /// all three happen every time a clip finishes and the UI keeps painting.
    #[test]
    fn the_callback_emits_silence_when_idle_paused_or_finished() {
        let slot: Arc<Mutex<Option<Arc<Playing>>>> = Arc::new(Mutex::new(None));
        let mut buf = vec![1.0f32; 8];
        fill(&mut buf, &slot);
        assert!(buf.iter().all(|v| *v == 0.0), "idle must be silent");

        let p = Arc::new(Playing {
            samples: vec![0.5; 8],
            channels: 2,
            cursor: AtomicUsize::new(0),
            paused: AtomicBool::new(true),
        });
        *slot.lock().unwrap() = Some(p.clone());
        buf.fill(1.0);
        fill(&mut buf, &slot);
        assert!(buf.iter().all(|v| *v == 0.0), "paused must be silent");
        assert_eq!(
            p.cursor.load(Ordering::Relaxed),
            0,
            "paused must not advance"
        );

        p.paused.store(false, Ordering::Relaxed);
        buf.fill(0.0);
        fill(&mut buf, &slot);
        assert!(
            buf.iter().all(|v| (*v - 0.5).abs() < 1e-6),
            "playing must emit the samples"
        );
        assert_eq!(p.cursor.load(Ordering::Relaxed), 4);

        buf.fill(1.0);
        fill(&mut buf, &slot);
        assert!(buf.iter().all(|v| *v == 0.0), "past the end must be silent");
    }

    /// A short final block must not read past the buffer.
    #[test]
    fn a_partial_final_block_is_bounded() {
        let slot: Arc<Mutex<Option<Arc<Playing>>>> = Arc::new(Mutex::new(None));
        let p = Arc::new(Playing {
            samples: vec![0.25; 6],
            channels: 2,
            cursor: AtomicUsize::new(2),
            paused: AtomicBool::new(false),
        });
        *slot.lock().unwrap() = Some(p.clone());
        let mut buf = vec![9.0f32; 8];
        fill(&mut buf, &slot);
        // One frame remained: two samples written, the rest silence.
        assert!((buf[0] - 0.25).abs() < 1e-6 && (buf[1] - 0.25).abs() < 1e-6);
        assert!(buf[2..].iter().all(|v| *v == 0.0));
        assert_eq!(p.cursor.load(Ordering::Relaxed), 3);
    }
}
