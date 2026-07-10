//! Typewriter key sound: real recorded typewriter keystrokes played on every registered
//! keypress while typing (correct and incorrect alike), with deeper space-bar/typebar
//! thunks for Space/Enter/Backspace.
//!
//! The samples are CC0 (public-domain) recordings bundled under `assets/sounds/` and
//! embedded in the binary. Sources:
//!
//! - `key-click-1/2/3.wav`: "Typewriter - single key - type 1/2/3.wav" by yottasounds,
//!   Freesound sounds #380138, #380137, #380136 (CC0 1.0,
//!   <https://freesound.org/people/yottasounds/sounds/380138/> etc.).
//! - `key-deep-1.wav`: "Typewriter, space" (#2843) and `key-deep-2.wav`: "Typewriter,
//!   key" (#2842), Hermes Precisa 305 recordings by Joseph Sardin, BigSoundBank
//!   (<https://bigsoundbank.com/typewriter-space-s2843.html>, CC0/public-domain
//!   equivalent per <https://bigsoundbank.com/licenses.html>).
//!
//! All five were converted to mono 16-bit 44.1 kHz WAV, trimmed to ~0.22-0.25 s, and
//! peak-normalized to about -3 dBFS. Each WAV is decoded once in `init()`; every press
//! then appends a cheap buffered source to one long-lived output mixer, picking a
//! random sample with a small random pitch/volume variation so rapid typing does not
//! sound machine-gun identical.
//!
//! Fallback: the original procedurally synthesized click (`synth_click`) is kept. If any
//! embedded sample fails to decode, one warning is logged and the synth buffers are used
//! instead, so `init` never fails for decode reasons.
//!
//! Resilience: audio must never crash or block the app. The output device is opened
//! lazily on the first play; if there is no device (headless CI, `make smoke`) the
//! failure is logged once and sound stays silently off while everything else works.

use rand::RngExt;

/// Sample rate of the synthesized buffers and of the bundled WAV recordings.
pub const SAMPLE_RATE: u32 = 44_100;

/// Recorded single-key strokes for ordinary keys (16-bit PCM WAV, mono, 44.1 kHz).
const NORMAL_WAVS: [(&str, &[u8]); 3] = [
    (
        "key-click-1.wav",
        include_bytes!("../../assets/sounds/key-click-1.wav"),
    ),
    (
        "key-click-2.wav",
        include_bytes!("../../assets/sounds/key-click-2.wav"),
    ),
    (
        "key-click-3.wav",
        include_bytes!("../../assets/sounds/key-click-3.wav"),
    ),
];

/// Recorded space-bar/typebar thunks for Space/Enter/Backspace.
const DEEP_WAVS: [(&str, &[u8]); 2] = [
    (
        "key-deep-1.wav",
        include_bytes!("../../assets/sounds/key-deep-1.wav"),
    ),
    (
        "key-deep-2.wav",
        include_bytes!("../../assets/sounds/key-deep-2.wav"),
    ),
];

/// Which click to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    /// An ordinary key.
    Normal,
    /// Space / Enter / Backspace: deeper and heavier, like a space-bar thunk.
    Deep,
}

/// Synthesize one click as mono f32 PCM at `SAMPLE_RATE`. Deterministic (seeded noise),
/// so tests can assert its shape. Kept as the fallback when the recordings fail to
/// decode.
pub fn synth_click(kind: ClickKind) -> Vec<f32> {
    let deep = kind == ClickKind::Deep;
    let sr = SAMPLE_RATE as f32;
    let dur = if deep { 0.060 } else { 0.035 };
    let n = (sr * dur) as usize;
    let tau = if deep { 0.013 } else { 0.006 };

    // Tiny deterministic LCG noise source; no global RNG needed for the buffer itself.
    let mut state: u32 = if deep { 0x9E37_79B9 } else { 0x1234_5677 };
    let mut noise = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };

    // One-pole low-pass shapes the noise; the deep variant is darker.
    let alpha = if deep { 0.16 } else { 0.42 };
    let thump_hz = if deep { 130.0 } else { 400.0 };
    let mut lp = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        // ~1.5ms attack so the click has a mechanical snap without a pop.
        let attack = (t / 0.0015).min(1.0);
        let env = attack * (-t / tau).exp();
        lp += alpha * (noise() - lp);
        let thump = (2.0 * std::f32::consts::PI * thump_hz * t).sin() * (-t / (tau * 1.7)).exp();
        let s = (lp * 0.85 + thump * if deep { 0.55 } else { 0.25 }) * env;
        out.push((s * 0.9).clamp(-1.0, 1.0));
    }
    out
}

/// One decoded, ready-to-play PCM buffer.
struct Sample {
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
    data: Vec<f32>,
}

/// Decode one embedded WAV into a PCM buffer. No audio device needed.
fn decode_wav(name: &str, bytes: &'static [u8]) -> Result<Sample, String> {
    use rodio::Source;
    let decoder =
        rodio::Decoder::new(std::io::Cursor::new(bytes)).map_err(|e| format!("{name}: {e}"))?;
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let data: Vec<f32> = decoder.collect();
    if data.is_empty() {
        return Err(format!("{name}: decoded to zero samples"));
    }
    Ok(Sample {
        channels,
        rate,
        data,
    })
}

/// Decode every embedded recording; any single failure fails the lot (the caller then
/// falls back to the synthesized clicks).
fn decode_embedded() -> Result<(Vec<Sample>, Vec<Sample>), String> {
    let decode_set = |set: &[(&str, &'static [u8])]| -> Result<Vec<Sample>, String> {
        set.iter()
            .map(|(name, bytes)| decode_wav(name, bytes))
            .collect()
    };
    Ok((decode_set(&NORMAL_WAVS)?, decode_set(&DEEP_WAVS)?))
}

/// Wrap a synthesized click in the same buffer type as the recordings.
fn synth_sample(kind: ClickKind) -> Sample {
    Sample {
        channels: rodio::ChannelCount::new(1).expect("1 is non-zero"),
        rate: rodio::SampleRate::new(SAMPLE_RATE).expect("SAMPLE_RATE is non-zero"),
        data: synth_click(kind),
    }
}

/// The live audio engine: one output device sink/mixer for the whole app plus the
/// pre-decoded PCM buffers (recordings, or the synth fallback).
pub struct KeySound {
    sink: rodio::MixerDeviceSink,
    normal: Vec<Sample>,
    deep: Vec<Sample>,
}

impl KeySound {
    /// Open the default output device and decode the embedded recordings once. Device
    /// errors are returned as strings (the caller logs once and disables sound); decode
    /// errors are logged once and fall back to the synthesized clicks, so they never
    /// fail init.
    pub fn init() -> Result<Self, String> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no audio output: {e}"))?;
        let (normal, deep) = match decode_embedded() {
            Ok(buffers) => buffers,
            Err(e) => {
                tracing::warn!(
                    "embedded key-sound sample failed to decode ({e}); \
                     falling back to synthesized clicks"
                );
                (
                    vec![synth_sample(ClickKind::Normal)],
                    vec![synth_sample(ClickKind::Deep)],
                )
            }
        };
        Ok(KeySound { sink, normal, deep })
    }

    /// Play one click: append a buffered source to the mixer, picking a random sample
    /// of the requested kind with a little random pitch and volume variation (real
    /// recordings vary naturally, so the range is tight). Never blocks.
    pub fn play(&self, kind: ClickKind) {
        let pool = match kind {
            ClickKind::Normal => &self.normal,
            ClickKind::Deep => &self.deep,
        };
        let mut rng = rand::rng();
        let sample = &pool[rng.random_range(0..pool.len())];
        let speed: f32 = rng.random_range(0.97..1.03);
        let volume: f32 = rng.random_range(0.55..0.70);
        use rodio::Source;
        let src =
            rodio::buffer::SamplesBuffer::new(sample.channels, sample.rate, sample.data.clone())
                .speed(speed)
                .amplify(volume);
        self.sink.mixer().add(src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_buffers_are_nonempty_and_bounded() {
        for kind in [ClickKind::Normal, ClickKind::Deep] {
            let buf = synth_click(kind);
            assert!(!buf.is_empty());
            // Bounded duration: a click, not a drone (under 100ms of samples).
            assert!(buf.len() < (SAMPLE_RATE as usize) / 10, "len={}", buf.len());
            // Every sample within [-1, 1]; real energy present.
            assert!(buf.iter().all(|s| (-1.0..=1.0).contains(s)));
            let peak = buf.iter().fold(0.0f32, |a, s| a.max(s.abs()));
            assert!(peak > 0.05, "click has no energy: peak={peak}");
            // Decays: the last millisecond is much quieter than the peak.
            let tail = &buf[buf.len().saturating_sub(44)..];
            let tail_peak = tail.iter().fold(0.0f32, |a, s| a.max(s.abs()));
            assert!(tail_peak < peak * 0.25, "click does not decay");
        }
    }

    #[test]
    fn deep_click_is_longer_than_normal() {
        assert!(synth_click(ClickKind::Deep).len() > synth_click(ClickKind::Normal).len());
    }

    /// Every embedded recording must decode headlessly (no audio device) to a
    /// non-empty, short, non-clipping mono buffer at the expected rate. This is what
    /// `init` relies on; if it breaks, `init` falls back to the synth, but the assets
    /// should never be broken in the first place.
    #[test]
    fn embedded_recordings_decode_clean() {
        for (name, bytes) in NORMAL_WAVS.iter().chain(DEEP_WAVS.iter()) {
            let s = decode_wav(name, bytes).unwrap_or_else(|e| panic!("decode failed: {e}"));
            assert_eq!(s.channels.get(), 1, "{name}: expected mono");
            assert_eq!(s.rate.get(), SAMPLE_RATE, "{name}: expected 44.1 kHz");
            assert!(!s.data.is_empty(), "{name}: empty buffer");
            // A click, not a drone: at most 300ms of samples.
            assert!(
                s.data.len() <= (SAMPLE_RATE as usize) * 3 / 10,
                "{name}: too long ({} samples)",
                s.data.len()
            );
            // In range and non-clipping: peaks were normalized with headroom, so even
            // after the in-app amplify (max 0.70) nothing can clip.
            assert!(
                s.data.iter().all(|x| (-1.0..=1.0).contains(x)),
                "{name}: sample out of [-1, 1]"
            );
            let peak = s.data.iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!(peak > 0.2, "{name}: no real energy (peak={peak})");
            assert!(peak < 0.95, "{name}: too hot / clipping risk (peak={peak})");
        }
    }

    /// The decode-failure path returns an error (which `init` turns into the synth
    /// fallback) instead of panicking.
    #[test]
    fn garbage_bytes_fail_decode_gracefully() {
        assert!(decode_wav("garbage", b"definitely not a RIFF/WAVE file").is_err());
    }

    /// Manual end-to-end check on a machine with speakers:
    /// `cargo test keysound_plays_on_real_device -- --ignored --nocapture`.
    /// Ignored by default because CI/headless boxes have no audio output device.
    #[test]
    #[ignore = "needs a real audio output device; run manually"]
    fn keysound_plays_on_real_device() {
        let ks = KeySound::init().expect("no audio device on this machine");
        for kind in [
            ClickKind::Normal,
            ClickKind::Normal,
            ClickKind::Normal,
            ClickKind::Deep,
            ClickKind::Deep,
        ] {
            ks.play(kind);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        // Let the last thunk ring out before the sink drops.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    /// The engine plays whole pools, so both kinds must have at least one sample and
    /// Normal must offer real variety (3+ distinct recordings).
    #[test]
    fn embedded_pools_have_expected_variety() {
        assert!(NORMAL_WAVS.len() >= 3);
        assert!(!DEEP_WAVS.is_empty());
        // Distinct content, not the same file copied around.
        for (i, (_, a)) in NORMAL_WAVS.iter().enumerate() {
            for (_, b) in NORMAL_WAVS.iter().skip(i + 1) {
                assert_ne!(a, b, "duplicate normal-key recordings");
            }
        }
    }
}
