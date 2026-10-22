//! Our own sounds (addendum §3.2–3.4, §4.7): every cue is a short
//! procedural sample function, no assets; the mixer sums voices with gain,
//! pan and a one-pole low-pass (the same muffling the doors give app
//! streams). The PipeWire stream that pulls `render()` lives in `nkw-sys`.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::Time;
use crate::acoustic::pan_to_stereo;

/// The floor under a footstep (a room's pattern, `plano-mundo.md` §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Floor {
    Wood,
    Carpet,
    Tile,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cue {
    Footstep(Floor),
    /// the creak at the start of an opening swing
    DoorOpen,
    /// the full thunk when a door you closed meets the frame
    DoorClose,
    /// the soft thunk of an auto-close (ambient, not an event)
    DoorThunk,
    Meow,
    /// one animalese syllable; `pitch` in Hz
    Blip {
        pitch: f32,
    },
    /// three tics over 420 ms
    VaultDial,
    /// the low clunk, placed right after the dial (play both together)
    VaultUnlock,
    /// a dry thunk after the 320 ms door
    VaultLock,
    Click,
    Purr,
}

/// Longest cue, seconds.
pub const MAX_CUE_S: f32 = 1.2;

/// How long a cue sounds, seconds.
pub fn duration(cue: &Cue) -> f32 {
    match cue {
        Cue::Footstep(Floor::Wood) => 0.08,
        Cue::Footstep(Floor::Carpet) => 0.14,
        Cue::Footstep(Floor::Tile) => 0.12,
        Cue::DoorOpen => 0.25,
        Cue::DoorClose => 0.12,
        Cue::DoorThunk => 0.07,
        Cue::Meow => 0.45,
        Cue::Blip { pitch } => {
            // vowel-ish low syllables last a little longer (25–40 ms)
            if *pitch < 300.0 { 0.04 } else { 0.03 }
        }
        Cue::VaultDial => 0.42,
        Cue::VaultUnlock => 0.62,
        Cue::VaultLock => 0.40,
        Cue::Click => 0.015,
        Cue::Purr => 1.0,
    }
}

/// Deterministic white noise in -1..1 from a sample index.
fn noise(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Attack-decay envelope: linear rise over `a` s, exponential fall.
fn env(t: f32, a: f32, decay: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < a {
        t / a
    } else {
        (-(t - a) / decay).exp()
    }
}

fn tri(phase: f32) -> f32 {
    let p = phase.fract();
    4.0 * (p - 0.5).abs() - 1.0
}

fn sine(f: f32, t: f32) -> f32 {
    (TAU * f * t).sin()
}

/// A short tic at `at` seconds.
fn tic(t: f32, at: f32, i: u32) -> f32 {
    let u = t - at;
    if !(0.0..0.015).contains(&u) {
        return 0.0;
    }
    (0.6 * sine(2400.0, u) + 0.4 * noise(i)) * env(u, 0.001, 0.004)
}

/// A low thunk at `at` seconds.
fn thunk(t: f32, at: f32, f: f32, decay: f32) -> f32 {
    let u = t - at;
    if u < 0.0 {
        return 0.0;
    }
    sine(f * (1.0 + 0.5 * (-u / 0.01).exp()), u) * env(u, 0.002, decay)
}

/// One sample of `cue` at `t` seconds from its start; 0 outside it. Pure.
pub fn synth(cue: &Cue, t: f32, rate: u32) -> f32 {
    let dur = duration(cue);
    if !(0.0..dur).contains(&t) || rate == 0 {
        return 0.0;
    }
    let i = (t * rate as f32) as u32;
    let fade = ((dur - t) / 0.005).min(1.0); // no click at the end
    let s = match cue {
        Cue::Footstep(floor) => {
            let (f, decay, n) = match floor {
                Floor::Wood => (110.0, 0.018, 0.35),
                Floor::Carpet => (80.0, 0.04, 0.15),
                Floor::Tile => (420.0, 0.03, 0.6),
            };
            (0.6 * sine(f, t) + n * noise(i)) * env(t, 0.002, decay) * 0.8
        }
        Cue::DoorOpen => {
            // a creak: a 400→900 Hz sweep, rough with noise
            let f = 400.0 + 500.0 * (t / dur);
            let phase = 400.0 * t + 250.0 * t * t / dur;
            let rough = 0.7 + 0.3 * noise(i / 64);
            (0.5 * tri(phase) * rough + 0.15 * noise(i)) * env(t, 0.02, 0.12) * (f / 900.0).sqrt()
        }
        Cue::DoorClose => 0.8 * thunk(t, 0.0, 70.0, 0.035) + 0.1 * noise(i) * env(t, 0.001, 0.006),
        Cue::DoorThunk => 0.45 * thunk(t, 0.0, 60.0, 0.02),
        Cue::Meow => {
            // a glide up then down, with a second harmonic
            let x = t / dur;
            let f = 520.0 + 300.0 * (std::f32::consts::PI * x).sin() - 80.0 * x;
            let phase = f * t;
            let body = 0.6 * tri(phase) + 0.25 * (TAU * 2.0 * phase).sin();
            body * env(t, 0.04, 0.25) * 0.8
        }
        Cue::Blip { pitch } => {
            let p = pitch.clamp(80.0, 2000.0);
            0.7 * tri(p * t) * env(t, 0.003, 0.012)
        }
        Cue::VaultDial => 0.8 * (tic(t, 0.0, i) + tic(t, 0.14, i) + tic(t, 0.28, i)),
        Cue::VaultUnlock => 0.85 * thunk(t, 0.42, 55.0, 0.05) + 0.4 * tic(t, 0.42, i),
        Cue::VaultLock => 0.8 * thunk(t, 0.32, 65.0, 0.025),
        Cue::Click => 0.6 * (0.5 * sine(1800.0, t) + 0.5 * noise(i)) * env(t, 0.0005, 0.003),
        Cue::Purr => {
            // a low rumble pulsing at 25 Hz
            let am = 0.5 + 0.5 * sine(25.0, t);
            let body = 0.5 * sine(110.0, t) + 0.3 * noise(i / 8);
            body * am * env(t, 0.15, 0.6) * 0.6
        }
    };
    (s * fade).clamp(-1.0, 1.0)
}

/// One-pole low-pass: `prev + a(x − prev)`, `a = 1 − e^(−2π·fc/rate)`.
pub fn one_pole(prev: f32, x: f32, cutoff_hz: f32, rate: u32) -> f32 {
    if rate == 0 {
        return x;
    }
    let a = 1.0 - (-TAU * cutoff_hz.max(0.0) / rate as f32).exp();
    prev + a * (x - prev)
}

/// A cue being played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    cue: Cue,
    /// frames played
    t: u32,
    gain: f32,
    pan: f32,
    lowpass_hz: Option<f32>,
    lp: f32,
}

/// Most voices at once; the oldest one is dropped past this.
pub const MAX_VOICES: usize = 32;

/// Sums voices into interleaved stereo. Pure apart from its own voices.
#[derive(Clone, Debug, PartialEq)]
pub struct Mixer {
    pub rate: u32,
    voices: Vec<Voice>,
    /// frames rendered since the last voice ended
    quiet: u64,
}

impl Mixer {
    pub fn new(rate: u32) -> Mixer {
        Mixer {
            rate,
            voices: Vec::new(),
            quiet: 0,
        }
    }

    /// Starts `cue`. `gain` 0..1, `pan` -1..1 (0 for mono), `lowpass_hz`
    /// from the acoustics when the sound comes through a door.
    pub fn play(&mut self, cue: Cue, gain: f32, pan: f32, lowpass_hz: Option<f32>) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(Voice {
            cue,
            t: 0,
            gain: gain.clamp(0.0, 1.0),
            pan: pan.clamp(-1.0, 1.0),
            lowpass_hz,
            lp: 0.0,
        });
    }

    /// Fills `out` (interleaved L, R) with the sum of the voices, clipped to
    /// ±1, and drops the finished ones. `true` while any voice is left.
    pub fn render(&mut self, out: &mut [f32]) -> bool {
        out.fill(0.0);
        let frames = out.len() / 2;
        let rate = self.rate;
        let had = !self.voices.is_empty();
        for v in &mut self.voices {
            let (gl, gr) = pan_to_stereo(v.gain, v.pan, false);
            for f in 0..frames {
                let t = (v.t + f as u32) as f32 / rate as f32;
                let mut s = synth(&v.cue, t, rate);
                if let Some(hz) = v.lowpass_hz {
                    v.lp = one_pole(v.lp, s, hz, rate);
                    s = v.lp;
                }
                out[2 * f] += s * gl;
                out[2 * f + 1] += s * gr;
            }
            v.t += frames as u32;
        }
        for x in out.iter_mut() {
            *x = x.clamp(-1.0, 1.0);
        }
        self.voices
            .retain(|v| (v.t as f32) < duration(&v.cue) * rate as f32);
        if self.voices.is_empty() {
            self.quiet = if had { 0 } else { self.quiet + frames as u64 };
        } else {
            self.quiet = 0;
        }
        !self.voices.is_empty()
    }

    pub fn active(&self) -> usize {
        self.voices.len()
    }

    /// Seconds of silence rendered since the last voice ended (0 while
    /// playing). The stream is suspended past 200 ms (§4.7). Silence is
    /// counted in rendered frames, which is what the stream sees; `now` is
    /// taken for symmetry with the other machines and not needed.
    pub fn silent_for(&self, _now: Time) -> f64 {
        if !self.voices.is_empty() {
            return 0.0;
        }
        self.quiet as f64 / self.rate.max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn all_cues() -> Vec<Cue> {
        vec![
            Cue::Footstep(Floor::Wood),
            Cue::Footstep(Floor::Carpet),
            Cue::Footstep(Floor::Tile),
            Cue::DoorOpen,
            Cue::DoorClose,
            Cue::DoorThunk,
            Cue::Meow,
            Cue::Blip { pitch: 220.0 },
            Cue::Blip { pitch: 660.0 },
            Cue::Blip { pitch: 1e9 },
            Cue::VaultDial,
            Cue::VaultUnlock,
            Cue::VaultLock,
            Cue::Click,
            Cue::Purr,
        ]
    }

    fn samples(cue: &Cue) -> Vec<f32> {
        let n = (duration(cue) * RATE as f32) as usize;
        (0..n)
            .map(|i| synth(cue, i as f32 / RATE as f32, RATE))
            .collect()
    }

    #[test]
    fn every_cue_is_short_finite_bounded_and_audible() {
        for cue in all_cues() {
            assert!(
                duration(&cue) > 0.0 && duration(&cue) <= MAX_CUE_S,
                "{cue:?}"
            );
            let s = samples(&cue);
            assert!(s.iter().all(|x| x.is_finite()), "{cue:?} finite");
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak <= 1.0, "{cue:?} peak {peak}");
            assert!(peak > 0.05, "{cue:?} audible, peak {peak}");
            assert_eq!(
                synth(&cue, duration(&cue) + 0.01, RATE),
                0.0,
                "{cue:?} ends"
            );
            assert_eq!(synth(&cue, -0.01, RATE), 0.0);
        }
    }

    #[test]
    fn synth_is_pure() {
        for cue in all_cues() {
            assert_eq!(samples(&cue), samples(&cue));
        }
    }

    #[test]
    fn vault_dial_lasts_420_ms_with_three_tics() {
        assert!((duration(&Cue::VaultDial) - 0.42).abs() < 1e-6);
        let s = samples(&Cue::VaultDial);
        let ms = |a: f32, b: f32| {
            let (i, j) = ((a * RATE as f32) as usize, (b * RATE as f32) as usize);
            s[i..j].iter().any(|x| x.abs() > 0.05)
        };
        assert!(ms(0.0, 0.02) && ms(0.14, 0.16) && ms(0.28, 0.30));
        assert!(!ms(0.05, 0.13), "silent between tics");
        // the unlock clunk waits for the dial
        let u = samples(&Cue::VaultUnlock);
        assert!(u[..(0.41 * RATE as f32) as usize].iter().all(|x| *x == 0.0));
    }

    #[test]
    fn one_pole_cutoff_is_monotone() {
        // step response after one sample: higher cutoff follows faster
        let mut last = 0.0;
        for hz in [50.0, 200.0, 800.0, 3_000.0, 10_000.0, 20_000.0] {
            let y = one_pole(0.0, 1.0, hz, RATE);
            assert!(y > last && y <= 1.0, "{hz}");
            last = y;
        }
        assert_eq!(one_pole(0.3, 1.0, 0.0, RATE), 0.3, "0 Hz holds");
        // it converges on a constant input
        let mut y = 0.0;
        for _ in 0..10_000 {
            y = one_pole(y, 1.0, 800.0, RATE);
        }
        assert!((y - 1.0).abs() < 1e-4);
    }

    fn zero_crossings(buf: &[f32]) -> usize {
        buf.chunks(2)
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    }

    fn render_all(m: &mut Mixer) -> Vec<f32> {
        let mut all = Vec::new();
        let mut buf = vec![0.0; 2 * 256];
        while m.render(&mut buf) {
            all.extend_from_slice(&buf);
        }
        all.extend_from_slice(&buf);
        all
    }

    #[test]
    fn lowpass_removes_high_frequency_energy() {
        for cue in [Cue::Footstep(Floor::Tile), Cue::DoorOpen, Cue::Click] {
            let mut dry = Mixer::new(RATE);
            dry.play(cue, 1.0, 0.0, None);
            let mut wet = Mixer::new(RATE);
            wet.play(cue, 1.0, 0.0, Some(800.0));
            let (a, b) = (
                zero_crossings(&render_all(&mut dry)),
                zero_crossings(&render_all(&mut wet)),
            );
            assert!(b < a, "{cue:?}: {b} < {a}");
        }
    }

    #[test]
    fn mixer_sums_pans_and_drops_finished_voices() {
        let mut m = Mixer::new(RATE);
        let mut buf = vec![0.0; 2 * 128];
        assert!(!m.render(&mut buf), "empty");
        assert!(buf.iter().all(|x| *x == 0.0));
        m.play(Cue::Click, 1.0, -1.0, None);
        m.play(Cue::Meow, 0.5, 1.0, None);
        assert_eq!(m.active(), 2);
        // hard left click only on L; hard right meow only on R
        let mut one = Mixer::new(RATE);
        one.play(Cue::Click, 1.0, -1.0, None);
        let mut b1 = vec![0.0; 2 * 128];
        one.render(&mut b1);
        assert!(b1.chunks(2).all(|f| f[1].abs() < 1e-6));
        assert!(b1.chunks(2).any(|f| f[0].abs() > 0.01));
        // the sum equals the two voices rendered apart
        let mut two = Mixer::new(RATE);
        two.play(Cue::Meow, 0.5, 1.0, None);
        let mut b2 = vec![0.0; 2 * 128];
        two.render(&mut b2);
        assert!(m.render(&mut buf));
        for k in 0..buf.len() {
            assert!((buf[k] - (b1[k] + b2[k])).abs() < 1e-6);
        }
        // the click (15 ms = 720 frames) ends first
        for _ in 0..6 {
            m.render(&mut buf);
        }
        assert_eq!(m.active(), 1);
        while m.render(&mut buf) {}
        assert_eq!(m.active(), 0);
        assert!(!m.render(&mut buf));
    }

    #[test]
    fn mixer_clips_and_caps_voices() {
        let mut m = Mixer::new(RATE);
        for _ in 0..40 {
            m.play(Cue::DoorClose, 1.0, 0.0, None);
        }
        assert_eq!(m.active(), MAX_VOICES);
        let mut buf = vec![0.0; 2 * 512];
        m.render(&mut buf);
        assert!(buf.iter().all(|x| x.abs() <= 1.0));
        assert!(buf.iter().any(|x| x.abs() > 0.99), "loud enough to clip");
    }

    #[test]
    fn silent_for_counts_rendered_silence() {
        let mut m = Mixer::new(RATE);
        m.play(Cue::Click, 1.0, 0.0, None);
        assert_eq!(m.silent_for(0.0), 0.0);
        let mut buf = vec![0.0; 2 * 4800];
        assert!(!m.render(&mut buf), "click done within 100 ms");
        assert_eq!(m.silent_for(0.0), 0.0, "just ended");
        m.render(&mut buf);
        m.render(&mut buf);
        assert!((m.silent_for(0.0) - 0.2).abs() < 1e-9);
        m.play(Cue::Click, 1.0, 0.0, None);
        assert_eq!(m.silent_for(0.0), 0.0);
    }
}
