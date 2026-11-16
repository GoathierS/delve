//! Sound effects, synthesized when the game starts rather than read from files: small
//! blips and noises in the spirit of the pixel art, with nothing to license or ship.

use std::cell::{Cell, RefCell};
use std::f32::consts::TAU;

use delve_core::game::{Ability, Action, Event, Game, ItemKind, State};
use delve_core::map::Pos;
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};
use macroquad::rand;

const RATE: u32 = 44_100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    /// The hero's blow lands.
    Hit,
    /// The hero is hurt.
    Hurt,
    /// A blow swishes past, or the hero dashes.
    Whoosh,
    Stun,
    Fire,
    Kill,
    /// A potion drunk.
    Heal,
    LevelUp,
    Coin,
    Oil,
    Potion,
    Amulet,
    Descend,
    Shade,
    TorchOut,
    Death,
    Escape,
    /// Moving through a menu.
    Tick,
    Select,
    Denied,
}

impl Sfx {
    pub const ALL: [Sfx; 20] = [
        Sfx::Hit,
        Sfx::Hurt,
        Sfx::Whoosh,
        Sfx::Stun,
        Sfx::Fire,
        Sfx::Kill,
        Sfx::Heal,
        Sfx::LevelUp,
        Sfx::Coin,
        Sfx::Oil,
        Sfx::Potion,
        Sfx::Amulet,
        Sfx::Descend,
        Sfx::Shade,
        Sfx::TorchOut,
        Sfx::Death,
        Sfx::Escape,
        Sfx::Tick,
        Sfx::Select,
        Sfx::Denied,
    ];

    /// Sounds heard over and over get a few takes, so they don't drill.
    fn takes(self) -> u32 {
        match self {
            Sfx::Hit | Sfx::Hurt | Sfx::Whoosh | Sfx::Kill => 3,
            _ => 1,
        }
    }

    pub fn name(self) -> String {
        format!("{self:?}").to_lowercase()
    }

    /// The samples of one take, between -1 and 1.
    pub fn samples(self, take: u32) -> Vec<f32> {
        let mut noise = Noise(0x9e37_79b9 ^ ((self as u32 + 1) * 7919 + take * 104_729));
        let v = take as f32;
        match self {
            Sfx::Hit => {
                let mut lp = Lowpass::default();
                let mut body = Osc::default();
                render(0.14, |t| {
                    let crack = lp.run(noise.next(), 0.35) * env(t, 0.001, 0.05);
                    let f = sweep(t, 0.14, 190.0 + v * 15.0, 65.0);
                    crack * 0.6 + body.sine(f) * 0.6 * env(t, 0.004, 0.14)
                })
            }
            Sfx::Hurt => {
                let mut lp = Lowpass::default();
                let mut buzz = Osc::default();
                render(0.24, |t| {
                    let f = sweep(t, 0.24, 280.0 - v * 20.0, 90.0);
                    let e = env(t, 0.002, 0.24);
                    (buzz.square(f) * 0.28 + lp.run(noise.next(), 0.5) * 0.4) * e
                })
            }
            Sfx::Whoosh => {
                let mut lp = Lowpass::default();
                let len = 0.2 + v * 0.02;
                render(len, |t| lp.run(noise.next(), 0.03 + 0.25 * t / len) * 1.4 * env(t, 0.08, len))
            }
            Sfx::Stun => {
                let (mut a, mut b, mut c) = (Osc::default(), Osc::default(), Osc::default());
                render(0.5, |t| {
                    let wobble = 1.0 + 0.01 * (t * TAU * 7.0).sin();
                    (a.sine(880.0 * wobble) * 0.3 + b.sine(1320.0) * 0.18 + c.sine(1760.0 * wobble) * 0.08)
                        * env(t, 0.002, 0.5)
                })
            }
            Sfx::Fire => {
                let mut lp = Lowpass::default();
                let mut rumble = Osc::default();
                render(0.55, |t| {
                    let roar = lp.run(noise.next(), 0.2 + 0.1 * (1.0 - t / 0.55));
                    let crackle = if noise.next() > 0.995 { 0.5 } else { 0.0 };
                    (roar * 0.9 + crackle + rumble.sine(70.0) * 0.25) * env(t, 0.04, 0.55)
                })
            }
            Sfx::Kill => {
                let mut lp = Lowpass::default();
                let mut body = Osc::default();
                render(0.32, |t| {
                    let f = sweep(t, 0.32, 120.0 - v * 10.0, 38.0);
                    (body.sine(f) * 0.7 + lp.run(noise.next(), 0.12) * 0.5) * env(t, 0.003, 0.32)
                })
            }
            Sfx::Heal => {
                let mut o = Osc::default();
                render(0.36, |t| {
                    // Three bubbles, each rising.
                    let k = (t / 0.12).floor();
                    let local = t - k * 0.12;
                    let f = 380.0 + k * 140.0 + local * 3000.0;
                    o.sine(f) * 0.4 * env(local, 0.005, 0.12)
                })
            }
            Sfx::LevelUp => notes(&[(523.0, 0.08), (659.0, 0.08), (784.0, 0.08), (1047.0, 0.3)], Wave::Square, 0.2),
            Sfx::Coin => notes(&[(988.0, 0.06), (1319.0, 0.22)], Wave::Square, 0.18),
            Sfx::Oil => {
                let mut o = Osc::default();
                render(0.24, |t| {
                    let local = t % 0.12;
                    o.sine(sweep(local, 0.12, 300.0, 140.0)) * 0.5 * env(local, 0.004, 0.12)
                })
            }
            Sfx::Potion => {
                let mut o = Osc::default();
                render(0.16, |t| o.triangle(sweep(t, 0.16, 1100.0, 1700.0)) * 0.35 * env(t, 0.003, 0.16))
            }
            Sfx::Amulet => {
                let mut voices = [Osc::default(), Osc::default(), Osc::default(), Osc::default()];
                render(1.4, |t| {
                    let shimmer = 1.0 + 0.004 * (t * TAU * 6.0).sin();
                    let mut s = 0.0;
                    for (o, f) in voices.iter_mut().zip([523.0, 659.0, 784.0, 1047.0]) {
                        s += o.sine(f * shimmer) * 0.15;
                    }
                    s * env(t, 0.06, 1.4)
                })
            }
            Sfx::Descend => notes(&[(392.0, 0.1), (330.0, 0.1), (262.0, 0.1), (196.0, 0.35)], Wave::Triangle, 0.4),
            Sfx::Shade => {
                let (mut a, mut b) = (Osc::default(), Osc::default());
                let mut lp = Lowpass::default();
                render(1.1, |t| {
                    let f = sweep(t, 1.1, 165.0, 120.0) * (1.0 + 0.03 * (t * TAU * 5.0).sin());
                    let moan = a.sine(f) * 0.3 + b.sine(f * 1.02) * 0.3;
                    let breath = lp.run(noise.next(), 0.04) * 0.6;
                    (moan + breath) * env(t, 0.45, 1.1)
                })
            }
            Sfx::TorchOut => {
                let mut lp = Lowpass::default();
                let mut low = Osc::default();
                render(0.8, |t| {
                    let fizz = lp.run(noise.next(), 0.45 * (1.0 - t / 0.8) + 0.02);
                    (fizz * 0.7 + low.sine(sweep(t, 0.8, 110.0, 55.0)) * 0.3) * env(t, 0.01, 0.8)
                })
            }
            Sfx::Death => {
                let mut o = Osc::default();
                let mut lp = Lowpass::default();
                render(1.3, |t| {
                    let f = sweep(t, 1.3, 330.0, 50.0) * (1.0 + 0.04 * (t * TAU * 6.0).sin());
                    (o.square(f) * 0.22 + lp.run(noise.next(), 0.08) * 0.25) * env(t, 0.01, 1.3)
                })
            }
            Sfx::Escape => notes(&[(392.0, 0.1), (494.0, 0.1), (587.0, 0.1), (784.0, 0.5)], Wave::Triangle, 0.45),
            Sfx::Tick => {
                let mut o = Osc::default();
                render(0.03, |t| o.sine(1700.0) * 0.18 * env(t, 0.001, 0.03))
            }
            Sfx::Select => {
                let mut o = Osc::default();
                render(0.09, |t| o.square(sweep(t, 0.09, 660.0, 990.0)) * 0.13 * env(t, 0.002, 0.09))
            }
            Sfx::Denied => {
                let mut o = Osc::default();
                render(0.2, |t| o.square(140.0) * 0.2 * env(t % 0.1, 0.002, 0.09))
            }
        }
    }
}

thread_local! {
    /// Every take of every sound, in [`Sfx::ALL`] order; empty when the game is silent.
    static SOUNDS: RefCell<Vec<Vec<Sound>>> = const { RefCell::new(vec![]) };
    static VOLUME: Cell<f32> = const { Cell::new(0.7) };
}

/// Builds every sound; until then, and if this is never called, the game is silent.
pub async fn load() {
    let mut all = vec![];
    for sfx in Sfx::ALL {
        let mut takes = vec![];
        for take in 0..sfx.takes() {
            if let Ok(sound) = load_sound_from_bytes(&wav(&sfx.samples(take))).await {
                takes.push(sound);
            }
        }
        all.push(takes);
    }
    SOUNDS.set(all);
}

pub fn volume() -> f32 {
    VOLUME.get()
}

pub fn set_volume(v: f32) {
    VOLUME.set(v.clamp(0.0, 1.0));
}

pub fn play(sfx: Sfx) {
    let volume = volume();
    if volume <= 0.0 {
        return;
    }
    SOUNDS.with_borrow(|all| {
        if let Some(takes) = all.get(sfx as usize)
            && !takes.is_empty()
        {
            let take = &takes[rand::gen_range(0, takes.len())];
            play_sound(take, PlaySoundParams { looped: false, volume });
        }
    });
}

/// What the hero and the game were like before an action, to hear what it changed.
pub struct Before {
    player: Pos,
    lit: bool,
    state: State,
}

impl Before {
    pub fn of(game: &Game) -> Self {
        Before { player: game.player, lit: game.torch > 0, state: game.state }
    }
}

/// The sounds of the last action, each heard once however many times it happened.
pub fn on_action(game: &Game, action: Action, before: &Before) {
    let mut heard = vec![];
    // No footsteps: walking is most of the game, and they soon grate.
    if game.player != before.player && matches!(action, Action::Use(Ability::Dash, ..)) {
        heard.push(Sfx::Whoosh);
    }
    for event in &game.events {
        heard.push(match *event {
            Event::Hit { to_hero: true, .. } => Sfx::Hurt,
            Event::Hit { .. } => Sfx::Hit,
            Event::Dodged { .. } => Sfx::Whoosh,
            Event::Stunned { .. } => Sfx::Stun,
            Event::Fire { .. } => Sfx::Fire,
            Event::Killed { .. } => Sfx::Kill,
            Event::Healed { .. } => Sfx::Heal,
            Event::LevelUp => Sfx::LevelUp,
            Event::PickedUp { kind, .. } => match kind {
                ItemKind::Gold(_) => Sfx::Coin,
                ItemKind::Oil => Sfx::Oil,
                ItemKind::Potion => Sfx::Potion,
                ItemKind::Amulet => Sfx::Amulet,
            },
            Event::Descended => Sfx::Descend,
            Event::Shade { .. } => Sfx::Shade,
        });
    }
    if before.lit && game.torch <= 0 {
        heard.push(Sfx::TorchOut);
    }
    if before.state != game.state {
        heard.push(if game.state == State::Dead { Sfx::Death } else { Sfx::Escape });
    }
    let mut played = vec![];
    for sfx in heard {
        if !played.contains(&sfx) {
            play(sfx);
            played.push(sfx);
        }
    }
}

/// A mono, 16-bit WAV file of these samples.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    for s in samples {
        out.extend(((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// `secs` of sound, each sample computed from its time.
fn render(secs: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    (0..n).map(|i| f(i as f32 / RATE as f32).clamp(-1.0, 1.0)).collect()
}

/// A quick rise over `attack`, then a fall to silence at `len`.
fn env(t: f32, attack: f32, len: f32) -> f32 {
    if t < attack { t / attack } else { (1.0 - (t - attack) / (len - attack)).clamp(0.0, 1.0).powi(2) }
}

/// From `from` to `to` over `len`, evenly in pitch.
fn sweep(t: f32, len: f32, from: f32, to: f32) -> f32 {
    from * (to / from).powf((t / len).min(1.0))
}

#[derive(Clone, Copy)]
enum Wave {
    Square,
    Triangle,
}

/// A little tune: each note held for its time, the last one ringing out.
fn notes(tune: &[(f32, f32)], wave: Wave, volume: f32) -> Vec<f32> {
    let mut out = vec![];
    let mut o = Osc::default();
    for &(f, len) in tune {
        out.extend(render(len, |t| {
            let s = match wave {
                Wave::Square => o.square(f),
                Wave::Triangle => o.triangle(f),
            };
            s * volume * env(t, 0.004, len)
        }));
    }
    out
}

/// An oscillator, keeping its phase so its pitch can glide.
#[derive(Default)]
struct Osc {
    phase: f32,
}

impl Osc {
    fn advance(&mut self, freq: f32) -> f32 {
        let p = self.phase;
        self.phase = (self.phase + freq / RATE as f32).fract();
        p
    }

    fn sine(&mut self, freq: f32) -> f32 {
        (self.advance(freq) * TAU).sin()
    }

    fn square(&mut self, freq: f32) -> f32 {
        if self.advance(freq) < 0.5 { 1.0 } else { -1.0 }
    }

    fn triangle(&mut self, freq: f32) -> f32 {
        let p = self.advance(freq);
        4.0 * (p - 0.5).abs() - 1.0
    }
}

/// White noise from a fixed seed, so each sound is the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        // xorshift32
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Softens a signal: the smaller `k`, the duller.
#[derive(Default)]
struct Lowpass {
    y: f32,
}

impl Lowpass {
    fn run(&mut self, x: f32, k: f32) -> f32 {
        self.y += (x - self.y) * k;
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_and_audible() {
        for sfx in Sfx::ALL {
            for take in 0..sfx.takes() {
                let s = sfx.samples(take);
                assert!(!s.is_empty() && s.len() < RATE as usize * 2, "{sfx:?}");
                let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                assert!(peak > 0.05 && peak <= 1.0, "{sfx:?} peaks at {peak}");
                assert!(s.iter().all(|x| x.is_finite()), "{sfx:?}");
            }
        }
    }

    #[test]
    fn wav_header_matches_the_samples() {
        let w = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 6);
    }
}
