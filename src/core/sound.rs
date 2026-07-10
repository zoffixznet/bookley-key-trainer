//! Typewriter key sound: a procedurally synthesized mechanical click played on every
//! registered keypress while typing (correct and incorrect alike), with a deeper, longer
//! variant for Space/Enter/Backspace.
//!
//! License-clean by construction: the click is synthesized in Rust (a filtered-noise
//! burst with a fast attack and exponential decay over a faint resonant thump); no
//! downloaded samples. The PCM buffers are generated once and reused; each press appends
//! a cheap buffered source to one long-lived output mixer, with a few percent of random
//! pitch/volume variation so rapid typing does not sound machine-gun identical.
//!
//! Resilience: audio must never crash or block the app. The output device is opened
//! lazily on the first play; if there is no device (headless CI, `make smoke`) the
//! failure is logged once and sound stays silently off while everything else works.

use rand::RngExt;

/// Sample rate of the synthesized buffers.
pub const SAMPLE_RATE: u32 = 44_100;

/// Which click to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    /// An ordinary key.
    Normal,
    /// Space / Enter / Backspace: slightly deeper and longer, like a carriage thock.
    Deep,
}

/// Synthesize one click as mono f32 PCM at `SAMPLE_RATE`. Deterministic (seeded noise),
/// so tests can assert its shape.
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

/// The live audio engine: one output device sink/mixer for the whole app plus the two
/// pre-synthesized PCM buffers.
pub struct KeySound {
    sink: rodio::MixerDeviceSink,
    normal: Vec<f32>,
    deep: Vec<f32>,
}

impl KeySound {
    /// Open the default output device and pre-generate the buffers. Errors are returned
    /// as strings; the caller logs once and disables sound.
    pub fn init() -> Result<Self, String> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no audio output: {e}"))?;
        Ok(KeySound {
            sink,
            normal: synth_click(ClickKind::Normal),
            deep: synth_click(ClickKind::Deep),
        })
    }

    /// Play one click: append a buffered source to the mixer with a few percent of
    /// random pitch and volume variation. Never blocks.
    pub fn play(&self, kind: ClickKind) {
        let data = match kind {
            ClickKind::Normal => self.normal.clone(),
            ClickKind::Deep => self.deep.clone(),
        };
        let mut rng = rand::rng();
        let speed: f32 = rng.random_range(0.94..1.06);
        let volume: f32 = rng.random_range(0.55..0.70);
        use rodio::Source;
        let channels = rodio::ChannelCount::new(1).expect("1 is non-zero");
        let rate = rodio::SampleRate::new(SAMPLE_RATE).expect("SAMPLE_RATE is non-zero");
        let src = rodio::buffer::SamplesBuffer::new(channels, rate, data)
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
}
