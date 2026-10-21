//! The screens between runs: the title, the armory and the best runs. Each one reads the
//! input and draws itself in the same pass.

use delve_core::game::{Game, START_POTIONS, START_STATS};
use delve_core::gear::{self, BuyError, Gear, Profile, Slot};
use delve_core::save::Score;
use macroquad::prelude::*;

use crate::render::{self, View};
use crate::sprites;
use crate::ui::{self, BAD, DIM, Face, GOLDEN, GOOD, TEXT, sprite};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Title,
    Armory,
    Scores,
    /// The title alone, for the cover picture of the game's page.
    Cover,
}

/// What the player asked for on a menu.
pub enum Command {
    Continue,
    NewRun,
    Quit,
    Go(Page),
    /// Buy this piece in the armory.
    Buy(&'static Gear),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    NewRun,
    Armory,
    Scores,
    Quit,
}

impl Entry {
    fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::NewRun => "New run",
            Entry::Armory => "Armory",
            Entry::Scores => "Best runs",
            Entry::Quit => "Quit",
        }
    }
}

/// The keys that move around menus, pressed this frame.
struct Nav {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    ok: bool,
    back: bool,
}

impl Nav {
    fn read() -> Nav {
        let any = |keys: &[KeyCode]| keys.iter().any(|&k| is_key_pressed(k));
        Nav {
            up: any(&[KeyCode::Up, KeyCode::K, KeyCode::W, KeyCode::Kp8]),
            down: any(&[KeyCode::Down, KeyCode::J, KeyCode::S, KeyCode::Kp2]),
            left: any(&[KeyCode::Left, KeyCode::H, KeyCode::A, KeyCode::Kp4]),
            right: any(&[KeyCode::Right, KeyCode::L, KeyCode::D, KeyCode::Kp6]),
            ok: any(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]),
            back: any(&[KeyCode::Escape, KeyCode::Backspace]),
        }
    }
}

pub struct Menu {
    pub page: Page,
    /// A dungeon slowly drifting behind the menus.
    backdrop: Game,
    clock: f32,
    selected: usize,
    /// Row (slot) and column (piece) chosen in the armory.
    armory: (usize, usize),
    /// A line shown for a moment, e.g. after a purchase, and when it appeared.
    notice: Option<(String, Color, f64)>,
    /// The last mouse position, to tell hovering from a still pointer.
    last_mouse: Vec2,
}

impl Menu {
    pub fn new() -> Self {
        Menu {
            page: Page::Title,
            backdrop: Game::new(424_242),
            clock: 0.0,
            selected: 0,
            armory: (0, 0),
            notice: None,
            last_mouse: Vec2::ZERO,
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, color: Color) {
        self.notice = Some((text.into(), color, get_time()));
    }

    /// Reads this frame's input and draws the current page.
    pub fn frame(&mut self, sheet: &Texture2D, profile: &Profile, scores: &[Score], has_save: bool) -> Option<Command> {
        self.clock += get_frame_time();
        clear_background(ui::BACKGROUND);
        set_camera(&ui::ui_camera());
        self.draw_backdrop(sheet);
        let nav = Nav::read();
        let moved = ui::mouse().distance(self.last_mouse) > 0.5;
        self.last_mouse = ui::mouse();
        let command = match self.page {
            Page::Title => self.title(sheet, profile, has_save, &nav, moved),
            Page::Armory => self.armory(sheet, profile, &nav, moved),
            Page::Scores => self.scores(scores, &nav),
            Page::Cover => {
                self.cover(sheet);
                None
            }
        };
        self.draw_notice();
        set_default_camera();
        command
    }

    fn draw_backdrop(&self, sheet: &Texture2D) {
        let t = self.clock;
        let map = &self.backdrop.map;
        let center = vec2(40.0 + 26.0 * (t * 0.021).sin(), 20.0 + 10.0 * (t * 0.033).sin());
        let view = View { center, tile: 56.0, shake: Vec2::ZERO };
        let dim = Color::new(0.42, 0.38, 0.4, 1.0);
        render::draw_map(sheet, map, &view, dim);
        for (i, m) in self.backdrop.monsters.iter().enumerate() {
            let bob = (t * 2.0 + i as f32).sin() * 3.0;
            let at = view.to_screen(vec2(m.pos.x as f32, m.pos.y as f32)) - vec2(28.0, 28.0 - bob);
            sprite(sheet, sprites::monster(m.kind), at, 56.0, dim);
        }
        let size = ui::size();
        draw_rectangle(0.0, 0.0, size.x, size.y, Color::new(0.03, 0.02, 0.05, 0.45));
        ui::vignette(0.8);
    }

    fn draw_notice(&mut self) {
        let Some((text, color, since)) = &self.notice else { return };
        let age = (get_time() - since) as f32;
        if age > 2.5 {
            self.notice = None;
            return;
        }
        let alpha = (2.5 - age).min(1.0);
        let size = ui::size();
        let w = ui::measure(text, 22.0, Face::Bold) + 40.0;
        let r = Rect::new((size.x - w) / 2.0, size.y - 120.0, w, 44.0);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.05, 0.08, 0.92 * alpha));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color { a: alpha, ..*color });
        ui::centered(text, size.x / 2.0, r.y + 30.0, 22.0, Face::Bold, Color { a: alpha, ..*color });
    }

    fn title(
        &mut self,
        sheet: &Texture2D,
        profile: &Profile,
        has_save: bool,
        nav: &Nav,
        moved: bool,
    ) -> Option<Command> {
        let entries: Vec<Entry> = [Entry::Continue, Entry::NewRun, Entry::Armory, Entry::Scores, Entry::Quit]
            .into_iter()
            .filter(|&e| e != Entry::Continue || has_save)
            .collect();
        let n = entries.len();
        self.selected = self.selected.min(n - 1);
        if nav.up {
            self.selected = (self.selected + n - 1) % n;
        }
        if nav.down {
            self.selected = (self.selected + 1) % n;
        }

        let size = ui::size();
        let cx = size.x / 2.0;
        let title_y = size.y * 0.27;
        let tw = ui::measure("DELVE", 132.0, Face::Title);
        ui::shadowed("DELVE", cx - tw / 2.0, title_y, 132.0, Face::Title, GOLDEN);
        ui::centered("Find the Amulet on the tenth floor.", cx, title_y + 48.0, 22.0, Face::Regular, DIM);

        // The hero and a demon face each other on both sides of the title.
        let bob = |phase: f32| (self.clock * 2.4 + phase).sin() * 4.0;
        sprite(sheet, sprites::HERO, vec2(cx - tw / 2.0 - 150.0, title_y - 104.0 + bob(0.0)), 112.0, WHITE);
        let demon = delve_core::game::BESTIARY.iter().find(|k| k.name == "demon").expect("a demon");
        sprite(sheet, sprites::monster(demon), vec2(cx + tw / 2.0 + 38.0, title_y - 104.0 + bob(1.7)), 112.0, WHITE);

        let (bw, bh, gap) = (380.0, 58.0, 14.0);
        let top = title_y + 100.0;
        let mut clicked = None;
        for (i, entry) in entries.iter().enumerate() {
            let r = Rect::new(cx - bw / 2.0, top + i as f32 * (bh + gap), bw, bh);
            let hovered = r.contains(ui::mouse());
            if hovered && moved {
                self.selected = i;
            }
            if hovered && is_mouse_button_pressed(MouseButton::Left) {
                clicked = Some(i);
            }
            let selected = self.selected == i;
            ui::card(r, selected);
            let color = if selected { GOLDEN } else { TEXT };
            ui::centered(entry.label(), cx, r.y + 38.0, 26.0, Face::Bold, color);
            if selected {
                ui::text_in("›", r.x + 22.0, r.y + 40.0, 32.0, Face::Bold, GOLDEN);
            }
        }

        purse(sheet, profile.purse);
        ui::text("+ / −  text size", 20.0, size.y - 20.0, 17.0, DIM);
        let version = concat!("v", env!("CARGO_PKG_VERSION"));
        ui::text(version, size.x - ui::measure(version, 17.0, Face::Regular) - 20.0, size.y - 20.0, 17.0, DIM);

        let chosen = clicked.or(nav.ok.then_some(self.selected));
        if nav.back {
            return Some(Command::Quit);
        }
        Some(match entries[chosen?] {
            Entry::Continue => Command::Continue,
            Entry::NewRun => Command::NewRun,
            Entry::Armory => Command::Go(Page::Armory),
            Entry::Scores => Command::Go(Page::Scores),
            Entry::Quit => Command::Quit,
        })
    }

    fn cover(&self, sheet: &Texture2D) {
        let size = ui::size();
        let (cx, cy) = (size.x / 2.0, size.y * 0.5);
        draw_rectangle(0.0, 0.0, size.x, size.y, Color::new(0.02, 0.01, 0.03, 0.55));
        // The warm light of a torch behind the title, in many faint layers so that it
        // fades without rings.
        for i in 0..60 {
            let r = size.y * (0.62 - i as f32 * 0.009);
            draw_circle(cx, cy - 20.0, r, Color::new(1.0, 0.55, 0.15, 0.012));
        }
        let size_t = 170.0;
        let tw = ui::measure("DELVE", size_t, Face::Title);
        ui::shadowed("DELVE", cx - tw / 2.0, cy + 20.0, size_t, Face::Title, GOLDEN);
        let tagline = "A roguelike about light, gold, and knowing when to leave.";
        ui::shadowed(tagline, cx - ui::measure(tagline, 30.0, Face::Bold) / 2.0, cy + 110.0, 30.0, Face::Bold, TEXT);
        sprite(sheet, sprites::HERO, vec2(cx - tw / 2.0 - 200.0, cy - 150.0), 176.0, WHITE);
        let demon = delve_core::game::BESTIARY.iter().find(|k| k.name == "demon").expect("a demon");
        sprite(sheet, sprites::monster(demon), vec2(cx + tw / 2.0 + 24.0, cy - 150.0), 176.0, WHITE);
    }

    fn armory(&mut self, sheet: &Texture2D, profile: &Profile, nav: &Nav, moved: bool) -> Option<Command> {
        let rows: Vec<Vec<&'static Gear>> = Slot::ALL.iter().map(|&s| gear::in_slot(s).collect()).collect();
        let (mut row, mut col) = self.armory;
        if nav.up {
            row = (row + rows.len() - 1) % rows.len();
        }
        if nav.down {
            row = (row + 1) % rows.len();
        }
        col = col.min(rows[row].len() - 1);
        if nav.left {
            col = (col + rows[row].len() - 1) % rows[row].len();
        }
        if nav.right {
            col = (col + 1) % rows[row].len();
        }

        let size = ui::size();
        let (cw, ch, gap) = (240.0, 112.0, 16.0);
        let side = 300.0;
        let width = 150.0 + 3.0 * (cw + gap) + 24.0 + side;
        let left = ((size.x - width) / 2.0).max(16.0);
        let top = 150.0;
        ui::shadowed("Armory", left, 92.0, 60.0, Face::Title, GOLDEN);
        ui::text("Gold brought back from the dungeon buys gear that every new hero carries.", left, 126.0, 19.0, DIM);
        purse(sheet, profile.purse);

        let mut clicked = None;
        for (r, pieces) in rows.iter().enumerate() {
            let y = top + r as f32 * (ch + gap);
            let slot = pieces[0].slot;
            ui::text_in(slot.name(), left, y + 50.0, 24.0, Face::Bold, TEXT);
            let worn = profile.loadout().into_iter().find(|g| g.slot == slot).map_or("—", |g| g.name);
            ui::text(worn, left, y + 76.0, 16.0, DIM);
            for (c, piece) in pieces.iter().enumerate() {
                let rect = Rect::new(left + 150.0 + c as f32 * (cw + gap), y, cw, ch);
                let hovered = rect.contains(ui::mouse());
                if hovered && moved {
                    (row, col) = (r, c);
                }
                if hovered && is_mouse_button_pressed(MouseButton::Left) {
                    (row, col) = (r, c);
                    clicked = Some(*piece);
                }
                gear_card(sheet, profile, piece, rect, (row, col) == (r, c));
            }
        }
        self.armory = (row, col);

        // What a new hero gets with the gear owned, and what the chosen piece would add.
        let chosen = rows[row][col];
        let x = left + 150.0 + 3.0 * (cw + gap) + 24.0;
        let panel = Rect::new(x, top, side, 4.0 * (ch + gap) - gap);
        ui::panel(panel);
        ui::text_in("Your hero sets out with", x + 18.0, top + 38.0, 20.0, Face::Bold, TEXT);
        let now = gear::equip(START_STATS, START_POTIONS, &profile.loadout());
        let mut with = profile.clone();
        with.purse = i32::MAX;
        let _ = with.buy(chosen);
        let then = gear::equip(START_STATS, START_POTIONS, &with.loadout());
        let lines = [
            ("Health", now.0.max_hp, then.0.max_hp),
            ("Attack", now.0.attack, then.0.attack),
            ("Defense", now.0.defense, then.0.defense),
            ("Potions", now.1, then.1),
        ];
        for (i, (name, a, b)) in lines.iter().enumerate() {
            let ly = top + 86.0 + i as f32 * 40.0;
            ui::text(name, x + 18.0, ly, 21.0, DIM);
            ui::text_in(&a.to_string(), x + 150.0, ly, 21.0, Face::Bold, TEXT);
            if b > a {
                ui::text_in(&format!("→ {b}"), x + 198.0, ly, 21.0, Face::Bold, GOOD);
            }
        }

        // Why the chosen piece cannot be bought, if it cannot.
        let why = match profile.can_buy(chosen) {
            Ok(()) => format!("Enter: buy for {} gold", chosen.price),
            Err(BuyError::AlreadyOwned) => "Already yours.".to_string(),
            Err(BuyError::Locked) => {
                let before = gear::in_slot(chosen.slot).take_while(|g| g.name != chosen.name).last();
                format!("Buy the {} first.", before.map_or("previous one", |g| g.name))
            }
            Err(BuyError::TooExpensive) => format!("{} more gold needed.", chosen.price - profile.purse),
        };
        let color = if profile.can_buy(chosen).is_ok() { GOLDEN } else { DIM };
        ui::text(&why, x + 18.0, top + 270.0, 18.0, color);

        let weapon =
            profile.loadout().into_iter().find(|g| g.slot == Slot::Weapon).map_or(sprites::DAGGER, sprites::gear);
        sprite(sheet, sprites::HERO, vec2(x + side / 2.0 - 80.0, panel.y + panel.h - 130.0), 96.0, WHITE);
        sprite(sheet, weapon, vec2(x + side / 2.0 + 10.0, panel.y + panel.h - 110.0), 64.0, WHITE);

        let hint = "arrows  choose      Enter  buy      Esc  back";
        ui::centered(hint, size.x / 2.0, size.y - 30.0, 19.0, Face::Bold, DIM);

        if nav.back {
            return Some(Command::Go(Page::Title));
        }
        clicked.or(nav.ok.then_some(chosen)).map(Command::Buy)
    }

    fn scores(&mut self, scores: &[Score], nav: &Nav) -> Option<Command> {
        let size = ui::size();
        let (w, h) = (720.0, 180.0 + scores.len().max(1) as f32 * 36.0);
        let (x, y) = ((size.x - w) / 2.0, ((size.y - h) / 2.0).max(16.0));
        ui::panel(Rect::new(x, y, w, h));
        ui::centered("Best runs", size.x / 2.0, y + 70.0, 52.0, Face::Title, GOLDEN);
        if scores.is_empty() {
            ui::centered("No run has ended yet. Go and delve!", size.x / 2.0, y + 130.0, 21.0, Face::Regular, DIM);
        }
        for (i, s) in scores.iter().enumerate() {
            let color = if i == 0 { GOLDEN } else { TEXT };
            render::score_line(s, i, x + 60.0, y + 130.0 + i as f32 * 36.0, color);
        }
        ui::centered("Esc  back", size.x / 2.0, y + h - 24.0, 19.0, Face::Bold, DIM);
        let click = is_mouse_button_pressed(MouseButton::Left);
        (nav.back || nav.ok || click).then_some(Command::Go(Page::Title))
    }
}

/// The gold in the purse, top right.
fn purse(sheet: &Texture2D, gold: i32) {
    let size = ui::size();
    let text = gold.to_string();
    let w = ui::measure(&text, 28.0, Face::Bold);
    let r = Rect::new(size.x - w - 96.0, 20.0, w + 76.0, 56.0);
    ui::panel(r);
    sprite(sheet, sprites::GOLD, vec2(r.x + 10.0, r.y + 8.0), 40.0, WHITE);
    ui::text_in(&text, r.x + 58.0, r.y + 38.0, 28.0, Face::Bold, GOLDEN);
}

fn gear_card(sheet: &Texture2D, profile: &Profile, piece: &Gear, r: Rect, selected: bool) {
    let state = profile.can_buy(piece);
    ui::card(r, selected);
    let faded = state == Err(BuyError::Locked);
    let tint = if faded { Color::new(0.35, 0.35, 0.4, 1.0) } else { WHITE };
    sprite(sheet, sprites::gear(piece), vec2(r.x + 10.0, r.y + 22.0), 68.0, tint);
    let (x, name_color) = (r.x + 88.0, if faded { DIM } else { TEXT });
    let mut name = piece.name.to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    // Long names shrink to fit the card.
    let size = (20.0 * (r.w - 98.0) / ui::measure(&name, 20.0, Face::Bold)).min(20.0);
    ui::text_in(&name, x, r.y + 36.0, size, Face::Bold, name_color);
    ui::text(&piece.describe(), x, r.y + 62.0, 16.0, DIM);
    let (status, color) = match state {
        Err(BuyError::AlreadyOwned) => ("owned".to_string(), GOOD),
        Err(BuyError::Locked) => (format!("{} gold", piece.price), DIM),
        Err(BuyError::TooExpensive) => (format!("{} gold", piece.price), BAD),
        Ok(()) => (format!("{} gold", piece.price), GOLDEN),
    };
    ui::text_in(&status, x, r.y + 94.0, 18.0, Face::Bold, color);
    if faded {
        ui::text("locked", r.x + r.w - ui::measure("locked", 14.0, Face::Regular) - 10.0, r.y + 94.0, 14.0, DIM);
    }
}
