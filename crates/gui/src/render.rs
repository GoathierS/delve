//! Drawing a run: the dungeon and its interface.

use delve_core::game::{Ability, Game, ItemKind, LAST_DEPTH, State, THROW_COST, TORCH_MAX, Tone};
use delve_core::gear::Slot;
use delve_core::map::{self, Pos, Tile};
use delve_core::save::Score;
use macroquad::prelude::*;

use crate::fx::{Fx, tile_center};
use crate::sprites;
use crate::ui::{self, BAD, DIM, Face, GOLDEN, GOOD, TEXT, sprite};

/// Where the world sits on screen: the camera's center, in tiles, and the tile size, in
/// interface units.
pub struct View {
    pub center: Vec2,
    pub tile: f32,
    pub shake: Vec2,
}

impl View {
    pub fn to_screen(&self, at: Vec2) -> Vec2 {
        (at - self.center) * self.tile + ui::size() / 2.0 + self.shake
    }

    /// The map cell under a screen point.
    pub fn to_cell(&self, screen: Vec2) -> Pos {
        let at = (screen - self.shake - ui::size() / 2.0) / self.tile + self.center;
        Pos::new((at.x + 0.5).floor() as i32, (at.y + 0.5).floor() as i32)
    }
}

pub struct Ending<'a> {
    pub scores: &'a [Score],
    pub rank: Option<usize>,
    /// The purse once this run's gold went in.
    pub purse: i32,
}

pub fn draw(
    game: &Game,
    fx: &Fx,
    view: &View,
    sheet: &Texture2D,
    hover: Option<Pos>,
    aim: Option<Ability>,
    ending: Option<Ending>,
) {
    clear_background(ui::BACKGROUND);
    set_camera(&ui::ui_camera());
    draw_world(game, fx, view, sheet);
    if let Some(ability) = aim {
        draw_aim(game, view, ability);
    }
    let size = ui::size();
    if fx.hurt > 0.0 {
        draw_rectangle(0.0, 0.0, size.x, size.y, Color::new(0.8, 0.0, 0.0, 0.22 * fx.hurt));
    }
    ui::vignette(0.35);
    draw_hud(game, sheet);
    draw_log(game);
    draw_help();
    if let Some(ability) = aim {
        let text = format!("{}: choose a direction      Esc cancels", capitalized(ability.name()));
        let w = ui::measure(&text, 22.0, Face::Bold) + 48.0;
        let r = Rect::new((size.x - w) / 2.0, 20.0, w, 50.0);
        ui::card(r, true);
        ui::centered(&text, size.x / 2.0, r.y + 33.0, 22.0, Face::Bold, GOLDEN);
    }
    if let Some(cell) = hover
        && ending.is_none()
    {
        draw_tooltip(game, cell);
    }
    if let Some(ending) = ending {
        draw_ending(game, &ending);
    }
    set_default_camera();
}

/// Light falls off with distance from the hero and wavers a little, like a torch flame;
/// remembered cells are dim and bluish.
fn light(game: &Game, p: Pos) -> Option<Color> {
    if game.map.is_visible(p) {
        let t = get_time() as f32;
        let flicker = 1.0 + 0.05 * (t * 6.3).sin() + 0.03 * (t * 14.7).sin();
        let d = tile_center(p).distance(tile_center(game.player)) / ((game.light_radius() as f32 + 1.0) * flicker);
        let l = 1.0 - 0.6 * d * d;
        Some(Color::new(l, l * 0.94, l * 0.84, 1.0))
    } else if game.map.is_explored(p) {
        Some(Color::new(0.22, 0.22, 0.32, 1.0))
    } else {
        None
    }
}

/// The map cells that show on screen.
fn on_screen(view: &View) -> impl Iterator<Item = Pos> {
    let half = ui::size() / (2.0 * view.tile) + vec2(2.0, 2.0);
    let (x0, x1) = (((view.center.x - half.x) as i32).max(0), ((view.center.x + half.x) as i32).min(map::WIDTH - 1));
    let (y0, y1) = (((view.center.y - half.y) as i32).max(0), ((view.center.y + half.y) as i32).min(map::HEIGHT - 1));
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| Pos::new(x, y)))
}

/// Draws one map cell; stairs sit on a floor, not on a void.
pub fn draw_tile(sheet: &Texture2D, map: &map::Map, p: Pos, view: &View, tint: Color) {
    let t = view.tile;
    let at = view.to_screen(tile_center(p)) - vec2(t, t) / 2.0;
    let tile = map.tile(p);
    if matches!(tile, Tile::StairsDown | Tile::Exit) {
        sprite(sheet, sprites::tile(Tile::Floor, p), at, t, tint);
    }
    sprite(sheet, sprites::tile(tile, p), at, t, tint);
}

/// The whole of a floor, lit evenly: the backdrop of the menus.
pub fn draw_map(sheet: &Texture2D, map: &map::Map, view: &View, tint: Color) {
    // Solid rock stays dark: only the walls that border a room or a corridor show.
    for p in on_screen(view) {
        if map.tile(p).walkable() || map::Map::neighbors(p).any(|n| map.tile(n).walkable()) {
            draw_tile(sheet, map, p, view, tint);
        }
    }
}

fn draw_world(game: &Game, fx: &Fx, view: &View, sheet: &Texture2D) {
    let t = view.tile;
    for p in on_screen(view) {
        if let Some(tint) = light(game, p) {
            draw_tile(sheet, &game.map, p, view, tint);
        }
    }

    // Ways out glow, so that they catch the eye from afar.
    let glow = 0.5 + 0.5 * (get_time() as f32 * 2.5).sin();
    for p in on_screen(view).filter(|&p| game.map.tile(p) == Tile::Exit && game.map.is_explored(p)) {
        let at = view.to_screen(tile_center(p));
        draw_circle(at.x, at.y, t * (0.7 + 0.1 * glow), Color::new(1.0, 0.85, 0.4, 0.10 + 0.08 * glow));
        draw_rectangle_lines(at.x - t / 2.0, at.y - t / 2.0, t, t, 2.0, Color::new(1.0, 0.85, 0.4, 0.4 + 0.4 * glow));
    }

    draw_threats(game, view);

    for item in game.items.iter().filter(|i| game.map.is_visible(i.pos)) {
        let tint = light(game, item.pos).unwrap_or(WHITE);
        // Items bob gently so that they catch the eye.
        let bob = ((get_time() as f32) * 3.0 + item.pos.x as f32).sin() * t * 0.04;
        let at = view.to_screen(tile_center(item.pos)) - vec2(t, t) / 2.0 + vec2(0.0, bob);
        sprite(sheet, sprites::item(item.kind), at, t, tint);
    }

    for c in &fx.corpses {
        let k = Fx::fade_progress(c);
        let size = t * (1.0 - 0.4 * k);
        let at = view.to_screen(c.at) - vec2(size, size) / 2.0 - vec2(0.0, k * t * 0.3);
        sprite(sheet, c.sprite, at, size, Color::new(1.0, 0.5, 0.5, 1.0 - k));
    }

    for m in game.monsters.iter().filter(|m| game.map.is_visible(m.pos)) {
        let pos = fx.monster(m.id, m.pos);
        let at = view.to_screen(pos) - vec2(t, t) / 2.0;
        let tint = if m.awake { WHITE } else { Color::new(0.75, 0.75, 0.85, 1.0) };
        sprite(sheet, sprites::monster(m.kind), at, t, tint);
        if m.stats.hp < m.stats.max_hp {
            let w = t * 0.8;
            let bar = vec2(at.x + (t - w) / 2.0, at.y - t * 0.08);
            draw_rectangle(bar.x, bar.y, w, t * 0.07, Color::new(0.0, 0.0, 0.0, 0.7));
            draw_rectangle(bar.x, bar.y, w * m.stats.hp as f32 / m.stats.max_hp as f32, t * 0.07, BAD);
        }
        if !m.awake {
            let z = ((get_time() as f32) * 2.0 + m.id as f32).sin() * t * 0.05;
            ui::text_in("z", at.x + t * 0.8, at.y + t * 0.1 + z, t * 0.4, Face::Bold, Color::new(0.7, 0.75, 1.0, 0.8));
        }
    }

    for f in &fx.flames {
        if f.age < 0.0 {
            continue;
        }
        let k = Fx::flame_progress(f);
        let at = view.to_screen(f.at);
        draw_circle(at.x, at.y, t * 0.42 * (1.0 - 0.5 * k), Color::new(1.0, 0.45, 0.05, 0.7 * (1.0 - k)));
        draw_circle(at.x, at.y, t * 0.24 * (1.0 - 0.6 * k), Color::new(1.0, 0.85, 0.3, 0.9 * (1.0 - k)));
    }

    let hero = view.to_screen(fx.hero(game)) - vec2(t, t) / 2.0;
    let tint = if game.state == State::Dead { Color::new(0.6, 0.2, 0.2, 0.7) } else { WHITE };
    sprite(sheet, sprites::HERO, hero, t, tint);

    for f in &fx.floaters {
        if f.age < 0.0 {
            continue;
        }
        let k = Fx::float_progress(f);
        let at = view.to_screen(f.at) - vec2(0.0, t * (0.4 + 0.8 * k));
        let size = (t * 0.42).max(20.0);
        let alpha = 1.0 - k * k;
        let x = at.x - ui::measure(&f.text, size, Face::Bold) / 2.0;
        ui::shadowed(&f.text, x, at.y, size, Face::Bold, Color { a: alpha, ..f.color });
    }
}

/// The cells each visible monster is about to hit, pulsing red; where blows overlap the red
/// deepens, and the hero's cell is outlined when they stand in one.
fn draw_threats(game: &Game, view: &View) {
    let t = view.tile;
    let pulse = 0.5 + 0.5 * (get_time() as f32 * 5.0).sin();
    for m in game.monsters.iter().filter(|m| game.map.is_visible(m.pos)) {
        let Some(cells) = &m.intent else { continue };
        for &c in cells.iter().filter(|&&c| game.map.is_explored(c)) {
            let at = view.to_screen(tile_center(c)) - vec2(t, t) / 2.0;
            draw_rectangle(at.x + 2.0, at.y + 2.0, t - 4.0, t - 4.0, Color::new(0.75, 0.0, 0.05, 0.34 + 0.16 * pulse));
            draw_rectangle_lines(at.x + 2.0, at.y + 2.0, t - 4.0, t - 4.0, 3.0, Color::new(1.0, 0.15, 0.15, 0.9));
        }
    }
    if game.threatened(game.player) {
        let at = view.to_screen(tile_center(game.player)) - vec2(t, t) / 2.0;
        draw_rectangle_lines(at.x, at.y, t, t, 4.0, Color::new(1.0, 0.2, 0.15, 0.6 + 0.4 * pulse));
    }
}

/// Where the ability being aimed would land, in each of the eight directions.
fn draw_aim(game: &Game, view: &View, ability: Ability) {
    let t = view.tile;
    let me = game.player;
    let free = |p: Pos| game.map.tile(p).walkable() && game.monster_at(p).is_none();
    let mark = |p: Pos, color: Color| {
        let at = view.to_screen(tile_center(p)) - vec2(t, t) / 2.0;
        draw_rectangle(at.x + 3.0, at.y + 3.0, t - 6.0, t - 6.0, Color { a: 0.22, ..color });
        draw_rectangle_lines(at.x + 3.0, at.y + 3.0, t - 6.0, t - 6.0, 3.0, color);
    };
    for p in map::Map::neighbors(me) {
        let (dx, dy) = (p.x - me.x, p.y - me.y);
        let at = |n: i32| Pos::new(me.x + dx * n, me.y + dy * n);
        match ability {
            Ability::Dash if free(at(1)) && free(at(2)) => mark(at(2), Color::new(0.4, 0.75, 1.0, 1.0)),
            Ability::Bash if game.monster_at(at(1)).is_some() => mark(at(1), GOLDEN),
            Ability::Throw => {
                for n in 1..=5 {
                    let c = at(n);
                    if !game.map.tile(c).walkable() {
                        break;
                    }
                    if game.monster_at(c).is_some() {
                        mark(c, Color::new(1.0, 0.95, 0.35, 1.0));
                        break;
                    }
                    mark(c, Color::new(1.0, 0.95, 0.35, 0.35));
                }
            }
            _ => {}
        }
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn bar(x: f32, y: f32, w: f32, ratio: f32, color: Color, caption: &str) {
    let h = 28.0;
    draw_rectangle(x, y, w, h, Color::new(0.15, 0.14, 0.17, 1.0));
    draw_rectangle(x, y, w * ratio.clamp(0.0, 1.0), h, color);
    ui::gradient(
        Rect::new(x, y, w * ratio.clamp(0.0, 1.0), h / 2.0),
        Color::new(1.0, 1.0, 1.0, 0.18),
        Color::new(1.0, 1.0, 1.0, 0.0),
        true,
    );
    ui::centered(caption, x + w / 2.0, y + 20.0, 18.0, Face::Bold, WHITE);
}

fn draw_hud(game: &Game, sheet: &Texture2D) {
    let (x, y, w) = (16.0, 16.0, 320.0);
    ui::panel(Rect::new(x, y, w, 360.0));
    let title = if game.depth == LAST_DEPTH {
        "Floor 10 · the Amulet".to_string()
    } else if Game::has_exit(game.depth) {
        format!("Floor {} · a way out", game.depth)
    } else {
        format!("Floor {}", game.depth)
    };
    ui::text_in(&title, x + 16.0, y + 38.0, 28.0, Face::Title, GOLDEN);

    let s = game.stats;
    let ratio = s.hp as f32 / s.max_hp as f32;
    let hp_color = if ratio > 0.5 {
        Color::new(0.25, 0.7, 0.3, 1.0)
    } else if ratio > 0.25 {
        Color::new(0.85, 0.65, 0.15, 1.0)
    } else {
        Color::new(0.8, 0.2, 0.2, 1.0)
    };
    bar(x + 16.0, y + 54.0, w - 32.0, ratio, hp_color, &format!("HP {}/{}", s.hp, s.max_hp));
    let xp = game.xp as f32 / game.xp_to_next() as f32;
    let caption = format!("Level {} · {}/{} XP", game.level, game.xp, game.xp_to_next());
    bar(x + 16.0, y + 88.0, w - 32.0, xp, Color::new(0.25, 0.4, 0.85, 1.0), &caption);
    let torch = game.torch as f32 / TORCH_MAX as f32;
    let (caption, color) = if game.torch > 0 {
        let low = game.torch < TORCH_MAX / 5;
        let color = if low { Color::new(0.75, 0.25, 0.1, 1.0) } else { Color::new(0.9, 0.5, 0.12, 1.0) };
        (format!("Torch {}/{}", game.torch, TORCH_MAX), color)
    } else {
        ("Torch out · the dark hunts you".to_string(), BAD)
    };
    bar(x + 16.0, y + 122.0, w - 32.0, torch, color, &caption);

    let y = y + 34.0;
    ui::text(&format!("Attack {}", s.attack), x + 16.0, y + 146.0, 21.0, TEXT);
    ui::text(&format!("Defense {}", s.defense), x + 166.0, y + 146.0, 21.0, TEXT);

    // The equipment: a dagger when no weapon was bought, nothing for the other slots.
    let mut gx = x + 12.0;
    let weapon = game.gear().find(|g| g.slot == Slot::Weapon).map_or(sprites::DAGGER, sprites::gear);
    for icon in std::iter::once(weapon).chain(game.gear().filter(|g| g.slot != Slot::Weapon).map(sprites::gear)) {
        sprite(sheet, icon, vec2(gx, y + 160.0), 40.0, WHITE);
        gx += 46.0;
    }

    sprite(sheet, sprites::POTION, vec2(x + 12.0, y + 202.0), 40.0, WHITE);
    ui::text(&format!("{}  (P)", game.potions), x + 56.0, y + 230.0, 21.0, TEXT);
    sprite(sheet, sprites::GOLD, vec2(x + 156.0, y + 202.0), 40.0, WHITE);
    ui::text(&game.gold.to_string(), x + 200.0, y + 230.0, 21.0, GOLDEN);
    // Gold only counts once out of the dungeon alive.
    let stake_x = x + 208.0 + ui::measure(&game.gold.to_string(), 21.0, Face::Regular);
    ui::text("at stake", stake_x, y + 230.0, 14.0, DIM);

    // The abilities, with their key and the turns before they are ready.
    for (i, ability) in Ability::ALL.into_iter().enumerate() {
        let r = Rect::new(x + 16.0 + i as f32 * 98.0, y + 250.0, 92.0, 48.0);
        let ready = game.can_use(ability).is_ok();
        ui::card(r, false);
        let key = ["D", "F", "T"][i];
        let label = ["dash", "bash", "fire"][i];
        let color = if ready { TEXT } else { DIM };
        ui::text_in(key, r.x + 10.0, r.y + 31.0, 22.0, Face::Bold, if ready { GOLDEN } else { DIM });
        ui::text(label, r.x + 34.0, r.y + 30.0, 17.0, color);
        let cd = game.cooldown(ability);
        if cd > 0 {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
            ui::centered(&cd.to_string(), r.x + r.w / 2.0, r.y + 33.0, 24.0, Face::Bold, TEXT);
        } else if ability == Ability::Throw && game.torch < THROW_COST {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
        }
    }
    ui::text(&format!("turn {} · seed {}", game.turn, game.seed), x + 16.0, y + 318.0, 14.0, DIM);
}

fn draw_log(game: &Game) {
    let size = ui::size();
    let lines = 6;
    let (w, line_h) = ((size.x * 0.48).clamp(420.0, 720.0), 27.0);
    let h = lines as f32 * line_h + 18.0;
    let (x, y) = (16.0, size.y - h - 16.0);
    ui::panel(Rect::new(x, y, w, h));
    let start = game.log.len().saturating_sub(lines);
    let shown = &game.log[start..];
    for (i, (msg, tone)) in shown.iter().enumerate() {
        let base = match tone {
            Tone::Normal => TEXT,
            Tone::Good => GOOD,
            Tone::Bad => BAD,
        };
        // Older lines fade out.
        let age = (shown.len() - 1 - i) as f32;
        let color = Color { a: 1.0 - age * 0.14, ..base };
        ui::text(msg, x + 14.0, y + 29.0 + i as f32 * line_h, 20.0, color);
    }
}

fn draw_help() {
    let lines = [
        ("arrows  hjkl", "move, attack"),
        ("y u b n", "diagonals"),
        ("click", "walk there"),
        ("D  F  T", "dash, bash, fire"),
        ("red cells", "blows to come"),
        (">  or  Enter", "go down"),
        ("<", "leave by a way out"),
        (".  or  Space", "wait"),
        ("P", "drink a potion"),
        ("+  −  wheel", "text size, zoom"),
        ("Esc", "save, back to menu"),
    ];
    let size = ui::size();
    let (w, line_h) = (330.0, 24.0);
    let h = lines.len() as f32 * line_h + 16.0;
    let (x, y) = (size.x - w - 16.0, size.y - h - 16.0);
    ui::panel(Rect::new(x, y, w, h));
    for (i, (keys, what)) in lines.iter().enumerate() {
        let ly = y + 26.0 + i as f32 * line_h;
        ui::text_in(keys, x + 14.0, ly, 17.0, Face::Bold, TEXT);
        ui::text(what, x + 150.0, ly, 17.0, DIM);
    }
}

fn draw_tooltip(game: &Game, cell: Pos) {
    if !game.map.is_visible(cell) {
        return;
    }
    let text = if let Some(i) = game.monster_at(cell) {
        let m = &game.monsters[i];
        let state = if m.stunned > 0 {
            " · stunned"
        } else if m.intent.is_some() {
            " · about to strike"
        } else if m.awake {
            ""
        } else {
            " · asleep"
        };
        format!(
            "{} · {} · {}/{} HP · atk {} def {}{state}",
            m.kind.name,
            m.kind.pattern.name(),
            m.stats.hp,
            m.stats.max_hp,
            m.stats.attack,
            m.stats.defense
        )
    } else if let Some(i) = game.item_at(cell) {
        match game.items[i].kind {
            ItemKind::Potion => "healing potion".into(),
            ItemKind::Gold(n) => format!("{n} gold"),
            ItemKind::Amulet => "the Amulet".into(),
            ItemKind::Oil => format!("flask of oil · +{} torch", delve_core::game::OIL),
        }
    } else if game.map.tile(cell) == Tile::StairsDown {
        "stairs down".into()
    } else if game.map.tile(cell) == Tile::Exit {
        "a way out · < to leave with your gold".into()
    } else {
        return;
    };
    let size = ui::size();
    let m = ui::mouse();
    let (w, h) = (ui::measure(&text, 19.0, Face::Regular) + 24.0, 36.0);
    let x = (m.x + 20.0).min(size.x - w - 4.0);
    let y = (m.y + 20.0).min(size.y - h - 4.0);
    ui::panel(Rect::new(x, y, w, h));
    ui::text(&text, x + 12.0, y + 25.0, 19.0, TEXT);
}

fn draw_ending(game: &Game, ending: &Ending) {
    let size = ui::size();
    draw_rectangle(0.0, 0.0, size.x, size.y, Color::new(0.0, 0.0, 0.0, 0.6));
    let shown = ending.scores.len().min(5);
    let (w, h) = (640.0, 312.0 + shown as f32 * 30.0);
    let (x, y) = ((size.x - w) / 2.0, (size.y - h) / 2.0);
    let cx = size.x / 2.0;
    ui::panel(Rect::new(x, y, w, h));
    let (title, color) = match game.state {
        State::Won => ("Victory!", GOLDEN),
        State::Escaped => ("Escaped!", GOOD),
        _ => ("You died", BAD),
    };
    let tw = ui::measure(title, 58.0, Face::Title);
    ui::shadowed(title, cx - tw / 2.0, y + 72.0, 58.0, Face::Title, color);
    let cause = match (game.state, game.killed_by.as_deref()) {
        (State::Won, _) => format!("You seized the Amulet on floor {}.", game.depth),
        (State::Escaped, _) => format!("You climbed out from floor {}.", game.depth),
        (_, Some(name)) => format!("Killed by a {name} on floor {}.", game.depth),
        _ => format!("Died on floor {}.", game.depth),
    };
    ui::centered(&cause, cx, y + 116.0, 24.0, Face::Regular, TEXT);
    let summary = format!("Level {} · {} kills", game.level, game.kills);
    ui::centered(&summary, cx, y + 148.0, 20.0, Face::Regular, DIM);
    let (gold, color) = if game.gold_is_safe() {
        (format!("+{} gold to your purse · {} in all", game.gold, ending.purse), GOLDEN)
    } else {
        (format!("{} gold lost in the dark · {} left in your purse", game.gold, ending.purse), BAD)
    };
    ui::centered(&gold, cx, y + 180.0, 21.0, Face::Bold, color);
    let rank = match ending.rank {
        Some(0) => " · best run yet!".to_string(),
        Some(r) => format!(" · #{} of your best runs", r + 1),
        None => String::new(),
    };
    ui::centered(&format!("Score {}{rank}", game.score()), cx, y + 220.0, 26.0, Face::Bold, TEXT);

    for (i, s) in ending.scores.iter().take(shown).enumerate() {
        let color = if ending.rank == Some(i) { GOLDEN } else { DIM };
        score_line(s, i, x + 70.0, y + 262.0 + i as f32 * 30.0, color);
    }
    ui::centered("Enter  back to the menu          R  new run", cx, y + h - 22.0, 19.0, Face::Bold, TEXT);
}

/// One row of the high scores.
pub fn score_line(s: &Score, i: usize, x: f32, y: f32, color: Color) {
    let how = if s.won {
        "seized the Amulet".to_string()
    } else if s.escaped {
        format!("escaped · floor {}", s.depth)
    } else {
        format!("{} · floor {}", s.killed_by.as_deref().unwrap_or("died"), s.depth)
    };
    ui::text_in(&format!("#{}", i + 1), x, y, 20.0, Face::Bold, color);
    ui::text_in(&s.score.to_string(), x + 50.0, y, 20.0, Face::Bold, color);
    ui::text(&format!("level {}", s.level), x + 140.0, y, 20.0, color);
    ui::text(&how, x + 250.0, y, 20.0, color);
}
