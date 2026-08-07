//! Native video playback: decode the server's H.264 MP4 in-process and play it.
//!
//! The server returns generated video as an H.264 MP4, and until now the GUI could only
//! write that to disk and hand it to an external player. Playing it here means decoding
//! it here - the same reason the audio path grew an in-process engine rather than
//! shelling out to a system player.
//!
//! Decoding the DELIVERED FILE is the point. Re-encoding the render to stills would be
//! easier, and wrong: the preview has to be the artefact the user actually gets, or it
//! cannot be used to judge the render. A palettised or re-compressed preview shows
//! artefacts the file does not have, and hides ones it does.
//!
//! Two steps, two crates: `mp4` reads the container (which samples, and the SPS/PPS
//! parameter sets that live in the `avcC` box, not in the samples), and `rusty_h264`
//! decodes the H.264 elementary stream. Both are pure Rust - no C in the tree.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// I420 -> interleaved RGB8, BT.601 limited range.
///
/// The previous decoder wrote RGB for us; this one hands back the three planes, which is
/// the honest shape of H.264 output. Limited range (Y in 16..235) is what libx264 emits
/// by default and what the server does not override.
fn yuv_to_frame(f: &rusty_h264::YuvFrame) -> Frame {
    let (w, h) = (f.width, f.height);
    let cw = w.div_ceil(2);
    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            // Read the luma plane as defensively as the chroma planes below, and for the
            // same reason. A decoder can hand back a frame whose plane is shorter than
            // width times height - a stride that is not the width, a picture truncated at
            // the end of the stream - and indexing it directly PANICS. In a release build
            // that panic happens inside the paint loop, so the window simply disappears:
            // the failure a viewer sees is "it crashed when I pressed play", with nothing
            // to point at. Missing luma reads as black rather than taking the window down.
            let yy = (f.y.get(y * w + x).copied().unwrap_or(16) as f32 - 16.0)
                * (255.0 / 219.0);
            let ci = (y / 2) * cw + (x / 2);
            let cb = f.u.get(ci).copied().unwrap_or(128) as f32 - 128.0;
            let cr = f.v.get(ci).copied().unwrap_or(128) as f32 - 128.0;
            let o = (y * w + x) * 3;
            rgb[o] = (yy + 1.402 * cr).clamp(0.0, 255.0) as u8;
            rgb[o + 1] = (yy - 0.344 * cb - 0.714 * cr).clamp(0.0, 255.0) as u8;
            rgb[o + 2] = (yy + 1.772 * cb).clamp(0.0, 255.0) as u8;
        }
    }
    Frame { rgb, width: w, height: h }
}

/// One decoded frame, interleaved RGB8.
#[derive(Clone)]
pub struct Frame {
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// A clip that decodes AS IT PLAYS.
///
/// Decoding every picture up front and holding them all costs, at 481 frames of 512
/// square, 378 MB of RGB before the first one is shown - and the wait before playback
/// then grows with the length of the clip. What a container actually gives is a list of
/// packets -
/// a few megabytes - and a decoder is a stream: the pictures can be produced when they are
/// needed and thrown away afterwards.
///
/// Going FORWARD is what playback does, and it costs one decode per picture either way.
/// Going backwards past the small window kept behind the playhead means starting the
/// decoder again from the first packet, because an H.264 picture is not independent of the
/// ones before it. That is the honest cost of not holding the whole film in memory.
pub struct Clip {
    /// Annex-B packets, one per container sample, parameter sets prepended to the first.
    packets: Vec<Vec<u8>>,
    pub fps: f32,
    /// How many samples the CONTAINER holds - the length the interface shows.
    ///
    /// The decoder may yield fewer pictures than there are samples (a profile it cannot
    /// handle, a truncated stream). Asking for one past the end returns nothing rather than
    /// failing, and the player clamps, so a short stream plays short instead of crashing.
    pub samples: usize,
    /// Decoded picture size, taken from the first picture the decoder produces.
    ///
    /// Playback reads its dimensions from each frame, so nothing on the hot path
    /// consults these; they are what a caller asks a clip about before drawing one,
    /// and what the decode test asserts the container actually yielded.
    #[cfg_attr(not(test), allow(dead_code))]
    pub width: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    pub height: usize,
    /// Mutex rather than a cell: the clip is decoded on a worker thread and handed to the
    /// interface inside an `Arc`, which needs it to be shareable.
    dec: std::sync::Mutex<Stream>,
}

/// How far the decoder has got, and the few pictures kept around the playhead.
struct Stream {
    decoder: rusty_h264::Decoder,
    /// Next packet to feed.
    next_packet: usize,
    /// Pictures produced so far, i.e. the index the NEXT one will have.
    produced: usize,
    /// (picture index, frame), most recent last. Small on purpose - it exists so that a
    /// repaint of the same frame, or a step back of one, does not re-decode the clip.
    cache: std::collections::VecDeque<(usize, Frame)>,
}

/// Pictures kept behind the playhead. Enough to absorb a repaint and a step or two back,
/// far short of anything that grows with the length of the clip.
const CACHE_FRAMES: usize = 8;

impl Clip {
    pub fn duration_s(&self) -> f32 {
        if self.fps <= 0.0 {
            return 0.0;
        }
        self.samples as f32 / self.fps
    }

    /// How many pictures the interface may ask for.
    pub fn len(&self) -> usize {
        self.samples
    }

    pub fn is_empty(&self) -> bool {
        self.samples == 0
    }

    /// The picture at `idx`, decoding forward to reach it.
    ///
    /// Returned by value. A picture is under a megabyte and one is handed over per displayed
    /// frame, which is nothing beside decoding it - and it keeps the decoder's state private
    /// instead of lending a reference out of a lock.
    pub fn frame(&self, idx: usize) -> Option<Frame> {
        let mut st = self.dec.lock().ok()?;
        if let Some((_, f)) = st.cache.iter().find(|(i, _)| *i == idx) {
            return Some(f.clone());
        }
        // Behind the window: an H.264 picture depends on the ones before it, so the only
        // way back is to start again. Playback never takes this path; a backwards scrub does.
        if idx < st.produced {
            st.decoder = rusty_h264::Decoder::new();
            st.next_packet = 0;
            st.produced = 0;
            st.cache.clear();
        }
        while st.produced <= idx {
            let Some(packet) = self.packets.get(st.next_packet).cloned() else {
                return None; // the stream yielded fewer pictures than the container claims
            };
            st.next_packet += 1;
            // ISOLATE the decoder. It is a third-party component fed bytes that a container
            // does not guarantee: this player skips a sample it cannot read, which leaves a
            // discontinuity, and the decoder asserts on two paths that a slice arrives with
            // a picture already open. Those are `expect`s inside a dependency, so they are
            // panics, and a panic on the paint thread does not surface as an error - the
            // window disappears. A picture that cannot be decoded is a missing picture, not
            // the end of the application.
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                st.decoder.decode(&packet)
            }));
            let decoded = match decoded {
                Ok(r) => r,
                Err(_) => {
                    // Its internal state is now unknown, so stop asking it for more of this
                    // stream rather than panicking again on every repaint.
                    st.next_packet = self.packets.len();
                    return None;
                }
            };
            match decoded {
                // A decoder legitimately returns nothing while it reads parameter sets.
                Ok(None) => continue,
                Ok(Some(yuv)) => {
                    let f = yuv_to_frame(&yuv);
                    let at = st.produced;
                    st.produced += 1;
                    st.cache.push_back((at, f));
                    while st.cache.len() > CACHE_FRAMES {
                        st.cache.pop_front();
                    }
                }
                Err(_) => return None,
            }
        }
        st.cache.iter().find(|(i, _)| *i == idx).map(|(_, f)| f.clone())
    }
}

/// Decode an MP4's video track to RGB frames.
///
/// Returns an error rather than a partial clip when the container has no H.264 video
/// track: a silent empty result would show as a player that does nothing, which is the
/// least diagnosable failure available.
pub fn decode_mp4(bytes: &[u8]) -> Result<Clip, String> {
    use mp4::Mp4Reader;
    use std::io::Cursor;

    let size = bytes.len() as u64;
    let mut reader =
        Mp4Reader::read_header(Cursor::new(bytes), size).map_err(|e| format!("mp4: {e}"))?;

    // Pick the H.264 track. A generated clip has exactly one, but a file that arrived
    // from elsewhere may carry audio too.
    let (track_id, fps) = {
        let mut found = None;
        for (id, t) in reader.tracks() {
            if matches!(t.media_type(), Ok(mp4::MediaType::H264)) {
                let fps = t.frame_rate() as f32;
                found = Some((*id, if fps.is_finite() && fps > 0.0 { fps } else { 16.0 }));
                break;
            }
        }
        found.ok_or_else(|| "mp4: no H.264 video track".to_string())?
    };

    // The parameter sets live in the container, not in the samples. Without them
    // prepended, the decoder rejects every frame - and the symptom is an empty clip
    // rather than an error, so this is worth being explicit about.
    let mut annexb_header = Vec::new();
    if let Some(avc) = reader
        .tracks()
        .get(&track_id)
        .and_then(|t| t.trak.mdia.minf.stbl.stsd.avc1.as_ref())
    {
        for sps in &avc.avcc.sequence_parameter_sets {
            annexb_header.extend_from_slice(&[0, 0, 0, 1]);
            annexb_header.extend_from_slice(&sps.bytes);
        }
        for pps in &avc.avcc.picture_parameter_sets {
            annexb_header.extend_from_slice(&[0, 0, 0, 1]);
            annexb_header.extend_from_slice(&pps.bytes);
        }
    }

    let count = reader.sample_count(track_id).map_err(|e| format!("mp4: {e}"))?;

    // Collect the PACKETS and stop there. Decoding every one of them here is what made
    // opening a clip cost its whole length in memory and in waiting; the stream can be
    // walked as it plays instead.
    let mut packets: Vec<Vec<u8>> = Vec::with_capacity(count as usize);
    for i in 1..=count {
        let Ok(Some(sample)) = reader.read_sample(track_id, i) else { continue };
        // MP4 stores samples length-prefixed (AVCC); the decoder wants start codes
        // (Annex B). Same NAL units, different framing.
        let mut packet = if i == 1 { annexb_header.clone() } else { Vec::new() };
        let data = &sample.bytes;
        let mut off = 0usize;
        while off + 4 <= data.len() {
            let len = u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
                as usize;
            off += 4;
            if len == 0 || off + len > data.len() {
                break;
            }
            packet.extend_from_slice(&[0, 0, 0, 1]);
            packet.extend_from_slice(&data[off..off + len]);
            off += len;
        }
        if !packet.is_empty() {
            packets.push(packet);
        }
    }
    if packets.is_empty() {
        return Err("mp4: the video track carries no packets".to_string());
    }

    let clip = Clip {
        packets,
        fps,
        samples: count as usize,
        width: 0,
        height: 0,
        dec: std::sync::Mutex::new(Stream {
            decoder: rusty_h264::Decoder::new(),
            next_packet: 0,
            produced: 0,
            cache: std::collections::VecDeque::new(),
        }),
    };
    // Decode the FIRST picture now. It is what proves the stream is one this decoder can
    // read - without it an unsupported profile surfaces as an empty clip rather than an
    // error - and it is where the dimensions come from. One picture, not the film.
    let first = clip
        .frame(0)
        .ok_or_else(|| "mp4: the video track decoded to no frames".to_string())?;
    Ok(Clip { width: first.width, height: first.height, ..clip })
}

/// Playback state for one clip: which frame is on screen, and whether time is running.
///
/// Frames are decoded ONCE, up front, and kept as RGB. A generated clip is a few dozen
/// frames, so this trades a bounded amount of memory for a player that can seek and loop
/// instantly - and for seeking not to need a decoder that can run backwards.
pub struct VideoPlayer {
    clip: Arc<Clip>,
    /// Index of the frame currently shown.
    index: usize,
    playing: bool,
    /// Seconds of playback owed since the last frame change, so the rate follows the
    /// clip rather than the UI's repaint rate.
    accum: f32,
    pub looping: bool,
}

impl VideoPlayer {
    pub fn new(clip: Clip) -> Self {
        Self { clip: Arc::new(clip), index: 0, playing: false, accum: 0.0, looping: true }
    }

    pub fn frame_count(&self) -> usize {
        self.clip.len()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn fps(&self) -> f32 {
        self.clip.fps
    }

    /// Frames the CONTAINER held, against what decoded.
    ///
    /// Surfaced so the UI can say a clip came back short instead of silently playing a
    /// third of it - the failure that B-frames caused, which looked like a working
    /// player until the counts were compared.
    pub fn samples(&self) -> usize {
        self.clip.samples
    }

    pub fn duration_s(&self) -> f32 {
        self.clip.duration_s()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn current(&self) -> Option<Frame> {
        self.clip.frame(self.index)
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn toggle(&mut self) {
        self.playing = !self.playing;
    }

    /// Jump to a frame, clamped, and stop the accumulated time so the next tick starts
    /// from here rather than catching up.
    pub fn seek(&mut self, index: usize) {
        self.index = index.min(self.clip.len().saturating_sub(1));
        self.accum = 0.0;
    }

    /// Advance by `dt` seconds of wall clock.
    ///
    /// Time is ACCUMULATED rather than mapped from a start instant: the UI repaints at
    /// whatever rate it likes, and a clip at 16 fps must not run at the display's rate.
    /// Returns true when the visible frame changed, so the caller knows to re-upload.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.playing || self.clip.is_empty() || self.clip.fps <= 0.0 {
            return false;
        }
        self.accum += dt;
        let per = 1.0 / self.clip.fps;
        let mut changed = false;
        while self.accum >= per {
            self.accum -= per;
            if self.index + 1 < self.clip.len() {
                self.index += 1;
            } else if self.looping {
                self.index = 0;
            } else {
                self.playing = false;
                break;
            }
            changed = true;
        }
        changed
    }
}

/// Decode off the UI thread: a clip is tens of frames of H.264, which is far too long to
/// run inside a repaint.
pub struct BackgroundDecode {
    pub done: Arc<Mutex<Option<Result<Clip, String>>>>,
    pub running: Arc<AtomicBool>,
}

impl BackgroundDecode {
    pub fn spawn(bytes: Vec<u8>) -> Self {
        let done = Arc::new(Mutex::new(None));
        let running = Arc::new(AtomicBool::new(true));
        let (d, r) = (done.clone(), running.clone());
        std::thread::spawn(move || {
            let out = decode_mp4(&bytes);
            *d.lock().unwrap_or_else(|e| e.into_inner()) = Some(out);
            r.store(false, Ordering::Relaxed);
        });
        Self { done, running }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Take the result once decoding has finished.
    pub fn take(&self) -> Option<Result<Clip, String>> {
        self.done.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests {
    //! The ignored cases here need real weights, a device, or a reference dump on
    //! this machine; nothing about them is automatic. Run one by name with
    //!   cargo test --release -p atelier --lib NAME -- --ignored --nocapture
    use super::*;

    fn clip(n: usize, fps: f32) -> Clip {
        Clip {
            packets: Vec::new(),
            fps,
            samples: n,
            width: 1,
            height: 1,
            dec: std::sync::Mutex::new(Stream {
                decoder: rusty_h264::Decoder::new(),
                next_packet: 0,
                produced: 0,
                cache: std::collections::VecDeque::new(),
            }),
        }
    }

    /// A frame whose luma plane is SHORT must not take the window down.
    ///
    /// This is the shape that crashed a viewer: the chroma planes were read defensively and
    /// the luma plane was not, so a decoder handing back a stride that is not the width, or
    /// a picture truncated at the end of a stream, panicked inside the paint loop. The
    /// window vanished with nothing to point at.
    #[test]
    fn a_short_luma_plane_is_black_not_a_crash() {
        let (w, h) = (8usize, 4usize);
        let f = rusty_h264::YuvFrame {
            width: w,
            height: h,
            y: vec![128u8; w * h - 5], // short on purpose
            u: vec![128u8; (w / 2) * (h / 2)],
            v: vec![128u8; (w / 2) * (h / 2)],
        };
        let frame = super::yuv_to_frame(&f);
        assert_eq!(frame.rgb.len(), w * h * 3);
        assert_eq!((frame.width, frame.height), (w, h));
    }

    /// Playback must follow the CLIP's rate, not the caller's tick rate - a UI repainting
    /// at 60 Hz must not run a 16 fps clip at 60 fps.
    #[test]
    fn playback_follows_the_clips_frame_rate() {
        let mut p = VideoPlayer::new(clip(32, 16.0));
        p.play();
        // A 60 Hz repaint: most ticks must NOT advance the frame.
        let mut advanced = 0;
        for _ in 0..60 {
            if p.tick(1.0 / 60.0) {
                advanced += 1;
            }
        }
        assert_eq!(advanced, 16, "one second at 16 fps must advance 16 frames, got {advanced}");
    }

    /// A tick far larger than one frame must not silently drop the clip out of sync:
    /// it advances by the whole elapsed time.
    /// Known colours must come back as those colours.
    ///
    /// The decoder hands over I420 planes and `yuv_to_frame` does the conversion by
    /// hand - the previous decoder wrote RGB itself. A wrong range (treating limited
    /// range as full, or vice versa) or swapped Cb/Cr shifts every pixel without
    /// failing any frame-count assertion: the clip plays, washed out or with red and
    /// blue exchanged. So decode flat colour clips and check the pixels.
    ///
    /// Run with COLOUR_MP4_DIR=<dir> holding red.mp4 and grey.mp4 encoded at the
    /// server's own settings (libx264, baseline, -bf 0, yuv420p).
    #[test]
    #[ignore = "needs flat-colour mp4s; run with COLOUR_MP4_DIR=<dir>"]
    fn flat_colours_survive_the_yuv_conversion() {
        let Ok(dir) = std::env::var("COLOUR_MP4_DIR") else {
            println!("COLOUR_MP4_DIR unset; skipping");
            return;
        };
        let dir = std::path::Path::new(&dir);

        let clip = decode_mp4(&std::fs::read(dir.join("grey.mp4")).expect("grey")).expect("grey");
        let f = clip.frame(0).expect("first picture");
        let px = &f.rgb[(f.height / 2 * f.width + f.width / 2) * 3..][..3];
        println!("mid-grey decoded to {px:?}");
        // 0x808080 in, so all three channels near 128 and within a few of each other.
        for (i, &c) in px.iter().enumerate() {
            assert!((c as i32 - 128).abs() <= 12, "grey channel {i} came back {c}, not ~128");
        }
        let spread = *px.iter().max().unwrap() as i32 - *px.iter().min().unwrap() as i32;
        assert!(spread <= 8, "grey decoded with a {spread} channel spread - a range or Cb/Cr error");

        let clip = decode_mp4(&std::fs::read(dir.join("red.mp4")).expect("red")).expect("red");
        let f = clip.frame(0).expect("first picture");
        let px = &f.rgb[(f.height / 2 * f.width + f.width / 2) * 3..][..3];
        println!("red decoded to {px:?}");
        // Pure red: R dominant, G and B low. Catches a Cb/Cr swap, which would put the
        // energy in blue instead.
        assert!(px[0] > 170, "red channel came back {} - expected the dominant one", px[0]);
        assert!(px[1] < 90 && px[2] < 90, "red leaked into G/B: {px:?} - Cb/Cr swapped?");
    }

    #[test]
    fn a_long_stall_catches_up_rather_than_losing_time() {
        let mut p = VideoPlayer::new(clip(100, 10.0));
        p.play();
        p.tick(0.55); // 5.5 frames' worth
        assert_eq!(p.index(), 5, "half a second at 10 fps is 5 frames");
    }

    #[test]
    fn looping_wraps_and_non_looping_stops() {
        let mut p = VideoPlayer::new(clip(3, 10.0));
        p.play();
        p.tick(1.0);
        assert!(p.is_playing(), "a looping clip keeps running");
        assert!(p.index() < 3);

        let mut q = VideoPlayer::new(clip(3, 10.0));
        q.looping = false;
        q.play();
        q.tick(1.0);
        assert!(!q.is_playing(), "a non-looping clip stops at the end");
        assert_eq!(q.index(), 2, "and rests on the last frame");
    }

    #[test]
    fn seeking_clamps_and_resets_the_accumulator() {
        let mut p = VideoPlayer::new(clip(5, 10.0));
        p.seek(99);
        assert_eq!(p.index(), 4, "seek past the end clamps to the last frame");
        p.seek(2);
        assert_eq!(p.index(), 2);
    }

    /// A picture that will not decode must be ANNOUNCED, not covered by the previous one.
    ///
    /// `tick` leaves the texture untouched when the decoder yields nothing, so the last
    /// good picture stays on screen while the frame counter walks on. Looked at, that is a
    /// clip that froze for no stated reason; the interface can only say so if the playback
    /// state admits that what is displayed is not what was asked for.
    #[test]
    fn a_frame_that_cannot_be_decoded_is_reported_rather_than_hidden() {
        let ctx = egui::Context::default();
        // No packets, so every picture is missing - the shape a truncated or unsupported
        // stream produces.
        let mut pb = VideoPlayback {
            player: Some(VideoPlayer::new(clip(10, 10.0))),
            ..Default::default()
        };
        pb.tick(&ctx, 0.0);
        assert_eq!(pb.shown_frame(), None, "nothing decoded, so nothing is on the texture");
        assert_eq!(pb.stale_frame(), Some(0), "the missing frame must be nameable");
    }

    /// And when the picture IS the frame asked for, nothing is claimed to be wrong.
    #[test]
    fn a_frame_that_is_on_screen_is_not_reported_as_missing() {
        let mut pb = VideoPlayback {
            player: Some(VideoPlayer::new(clip(10, 10.0))),
            ..Default::default()
        };
        pb.uploaded = Some(0);
        assert_eq!(pb.stale_frame(), None);
        // Stepping away from the uploaded picture makes it stale again.
        pb.player.as_mut().unwrap().seek(4);
        assert_eq!(pb.stale_frame(), Some(4));
    }

    /// With no clip at all there is nothing to be stale about - the viewer must not read
    /// "frame 1 is missing" out of an empty player and put a warning on screen for it.
    #[test]
    fn an_empty_playback_reports_no_missing_frame() {
        assert_eq!(VideoPlayback::default().stale_frame(), None);
    }

    /// Garbage in must be an error, not an empty player that looks like a broken UI.
    #[test]
    fn a_non_mp4_payload_is_an_error() {
        assert!(decode_mp4(b"not an mp4 at all").is_err());
    }

    /// Decode a REAL H.264 MP4, produced by the same ffmpeg the server encodes with.
    ///
    /// The unit tests above exercise the transport; none of them touch the decoder, the
    /// AVCC-to-Annex-B reframing, or the parameter sets that live in the container rather
    /// than the stream. Those are exactly where this breaks, and the failure is silent -
    /// a clip that decodes to nothing looks like a player that does not work.
    ///
    /// Point VIDEO_MP4 at a file.
    #[test]
    #[ignore = "needs an mp4; run with VIDEO_MP4=<path>"]
    fn decodes_a_real_h264_mp4() {
        let Ok(path) = std::env::var("VIDEO_MP4") else {
            println!("VIDEO_MP4 not set; skipping");
            return;
        };
        let bytes = std::fs::read(&path).expect("read mp4");
        let clip = decode_mp4(&bytes).expect("decode");
        println!(
            "{} samples at {:.1} fps, {}x{}",
            clip.samples, clip.fps, clip.width, clip.height
        );
        assert!(clip.fps > 0.0, "frame rate must come out of the container");
        let (w, h) = (clip.width, clip.height);
        assert!(w > 0 && h > 0);
        // Walk it the way playback does. A decoder that cannot handle the stream's profile
        // returns fewer pictures than were encoded and reports no error, so the clip plays
        // short with nothing to point at - that is what this counts.
        let mut got = 0usize;
        for i in 0..clip.samples {
            let Some(f) = clip.frame(i) else { break };
            assert_eq!((f.width, f.height), (w, h), "frame {i} changed size");
            got += 1;
        }
        assert_eq!(
            got, clip.samples,
            "decoded {got} of {} samples - the decoder is dropping frames it cannot handle",
            clip.samples
        );
        // Consecutive frames must DIFFER: a decoder that returns its first picture over
        // and over passes every count above while showing a still image.
        let mut differing = 0usize;
        let mut prev: Option<Frame> = None;
        for i in 0..got {
            let Some(f) = clip.frame(i) else { break };
            assert_eq!(f.rgb.len(), w * h * 3, "frame {i} is not RGB8");
            if prev.as_ref().is_some_and(|p| p.rgb != f.rgb) {
                differing += 1;
            }
            prev = Some(f);
        }
        assert!(
            differing >= got / 2,
            "only {differing} of {} frame pairs differ - the decoder is repeating itself",
            got.saturating_sub(1)
        );
    }
}

/// What the UI keeps between frames to show one clip.
///
/// Lives beside the texture cache rather than inside MediaState, which is `Clone` -
/// a decoder and a GPU texture are not values to copy around, and making them
/// cloneable to fit a derive would be the tail wagging the dog.
#[derive(Default)]
pub struct VideoPlayback {
    /// Which `result_files` entry is loaded, so switching results reloads.
    pub source: Option<usize>,
    pub player: Option<VideoPlayer>,
    pub decoding: Option<BackgroundDecode>,
    pub texture: Option<egui::TextureHandle>,
    pub error: Option<String>,
    /// Frame currently uploaded, so an unchanged frame is not re-uploaded per repaint.
    uploaded: Option<usize>,
}

impl VideoPlayback {
    /// Start decoding `bytes` for result `index`, discarding whatever was loaded.
    pub fn open(&mut self, index: usize, bytes: Vec<u8>) {
        self.source = Some(index);
        self.player = None;
        self.texture = None;
        self.uploaded = None;
        self.error = None;
        self.decoding = Some(BackgroundDecode::spawn(bytes));
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// A playback whose picture is already on screen, for tests of the interface that
    /// need something to draw without a decoder in the loop.
    #[cfg(test)]
    pub fn with_frame(texture: egui::TextureHandle, index: usize) -> Self {
        Self { texture: Some(texture), uploaded: Some(index), ..Self::default() }
    }

    /// Which frame the texture actually holds, if it holds one.
    pub fn shown_frame(&self) -> Option<usize> {
        self.uploaded
    }

    /// The frame the transport points at, WHEN the picture on screen is not it.
    ///
    /// A picture that cannot be decoded is a missing picture: `tick` leaves the texture
    /// alone and the counter goes on advancing, so the viewer shows an older frame while
    /// claiming to be somewhere else. Nothing about that is visible - the clip simply
    /// appears to freeze - so the interface has to be able to ask, and say so.
    pub fn stale_frame(&self) -> Option<usize> {
        let want = self.player.as_ref()?.index();
        (self.uploaded != Some(want)).then_some(want)
    }

    /// Advance one UI frame: collect a finished decode, run the clock, and upload the
    /// visible frame when it changed. Returns true if a repaint is wanted soon.
    pub fn tick(&mut self, ctx: &egui::Context, dt: f32) -> bool {
        if let Some(d) = &self.decoding {
            if !d.is_running() {
                match d.take() {
                    Some(Ok(clip)) => {
                        let mut p = VideoPlayer::new(clip);
                        p.play();
                        self.player = Some(p);
                    }
                    Some(Err(e)) => self.error = Some(e),
                    None => {}
                }
                self.decoding = None;
            } else {
                return true;
            }
        }
        let Some(p) = self.player.as_mut() else { return false };
        let changed = p.tick(dt);
        let idx = p.index();
        if self.uploaded != Some(idx) || self.texture.is_none() {
            if let Some(f) = p.current() {
                let img = egui::ColorImage::from_rgb([f.width, f.height], &f.rgb);
                match &mut self.texture {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture = Some(ctx.load_texture(
                            "video_frame",
                            img,
                            egui::TextureOptions::LINEAR,
                        ))
                    }
                }
                self.uploaded = Some(idx);
            }
        }
        let _ = changed;
        p.is_playing()
    }
}
