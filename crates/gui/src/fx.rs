//! Eye candy: smooth movement, floating numbers, flashes and fading corpses. None of it
//! affects the game; it only replays the core's events.

use std::collections::HashMap;

use delve_core::game::{Event, Game, ItemKind};
use delve_core::map::Pos;
use macroquad::prelude::*;

use crate::sprites;

/// Key of the hero in [`Fx::shown`]; monsters use their id, which starts at 1.
const HERO: u32 = 0;
const FLOAT_TIME: f32 = 0.9;
const FADE_TIME: f32 = 0.45;
const LUNGE_TIME: f32 = 0.16;

pub struct Floater {
    pub at: Vec2,
    pub text: String,
    pub color: Color,
    pub age: f32,
}

pub struct Corpse {
    pub at: Vec2,
    pub sprite: u32,
    pub age: f32,
}

#[derive(Default)]
pub struct Fx {
    /// Where each creature is drawn, in tiles; slides toward its real cell.
    shown: HashMap<u32, Vec2>,
    pub floaters: Vec<Floater>,
    pub corpses: Vec<Corpse>,
    /// Hero's bump toward what they hit: direction and time left.
    lunge: Option<(Vec2, f32)>,
    /// 1 right after the hero is hurt, fading to 0.
    pub hurt: f32,
    pub shake: f32,
}

pub fn tile_center(p: Pos) -> Vec2 {
    vec2(p.x as f32, p.y as f32)
}

impl Fx {
    /// Forgets every position, e.g. on a new floor or a new game.
    pub fn reset(&mut self) {
        *self = Fx::default();
    }

    /// Turns the events of the last action into effects.
    pub fn on_events(&mut self, game: &Game) {
        for event in &game.events {
            match *event {
                Event::Hit { target, amount, to_hero } => {
                    let color = if to_hero { Color::from_rgba(255, 80, 70, 255) } else { WHITE };
                    self.float(target, format!("-{amount}"), color);
                    if to_hero {
                        self.hurt = 1.0;
                        self.shake = 0.25;
                    } else {
                        let dir = (tile_center(target) - tile_center(game.player)).normalize_or_zero();
                        self.lunge = Some((dir, LUNGE_TIME));
                    }
                }
                Event::Killed { pos, kind } => {
                    self.corpses.push(Corpse { at: tile_center(pos), sprite: sprites::monster(kind), age: 0.0 });
                }
                Event::Healed { amount } => self.float(game.player, format!("+{amount}"), GREEN),
                Event::LevelUp => self.float(game.player, "LEVEL UP".into(), GOLD),
                Event::PickedUp { pos, kind } => {
                    let text = match kind {
                        ItemKind::Potion => "+1 potion".to_string(),
                        ItemKind::Gold(n) => format!("+{n} gold"),
                        ItemKind::Amulet => "the Amulet!".to_string(),
                    };
                    self.float(pos, text, Color::from_rgba(255, 210, 60, 255));
                }
                Event::Descended => {
                    self.reset();
                }
            }
        }
    }

    fn float(&mut self, at: Pos, text: String, color: Color) {
        // Stack numbers that land on the same cell in the same instant.
        let stacked = self.floaters.iter().filter(|f| f.age < 0.1 && f.at == tile_center(at)).count();
        let mut f = Floater { at: tile_center(at), text, color, age: 0.0 };
        f.age -= stacked as f32 * 0.12;
        self.floaters.push(f);
    }

    /// Advances animations by `dt` seconds and slides creatures toward their cells.
    pub fn update(&mut self, game: &Game, dt: f32) {
        let follow = (dt * 16.0).min(1.0);
        let mut slide = |id: u32, to: Pos| {
            let target = tile_center(to);
            let p = self.shown.entry(id).or_insert(target);
            // Teleports (e.g. a fresh load) snap instead of sliding across the map.
            if p.distance(target) > 3.0 {
                *p = target;
            } else {
                *p += (target - *p) * follow;
            }
        };
        slide(HERO, game.player);
        for m in &game.monsters {
            slide(m.id, m.pos);
        }
        self.shown.retain(|&id, _| id == HERO || game.monsters.iter().any(|m| m.id == id));

        for f in &mut self.floaters {
            f.age += dt;
        }
        self.floaters.retain(|f| f.age < FLOAT_TIME);
        for c in &mut self.corpses {
            c.age += dt;
        }
        self.corpses.retain(|c| c.age < FADE_TIME);
        if let Some((_, t)) = &mut self.lunge {
            *t -= dt;
        }
        if self.lunge.is_some_and(|(_, t)| t <= 0.0) {
            self.lunge = None;
        }
        self.hurt = (self.hurt - dt * 2.5).max(0.0);
        self.shake = (self.shake - dt).max(0.0);
    }

    /// Where to draw the hero, in tiles, bump included.
    pub fn hero(&self, game: &Game) -> Vec2 {
        let base = self.shown.get(&HERO).copied().unwrap_or(tile_center(game.player));
        match self.lunge {
            // Out and back: a sine over the bump's lifetime.
            Some((dir, t)) => base + dir * 0.3 * (std::f32::consts::PI * t / LUNGE_TIME).sin(),
            None => base,
        }
    }

    pub fn monster(&self, id: u32, at: Pos) -> Vec2 {
        self.shown.get(&id).copied().unwrap_or(tile_center(at))
    }

    pub fn float_progress(f: &Floater) -> f32 {
        (f.age / FLOAT_TIME).clamp(0.0, 1.0)
    }

    pub fn fade_progress(c: &Corpse) -> f32 {
        (c.age / FADE_TIME).clamp(0.0, 1.0)
    }
}
