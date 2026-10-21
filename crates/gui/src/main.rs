//! delve: a small roguelike, drawn with macroquad.

mod fx;
mod menu;
mod render;
mod sprites;
mod ui;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use delve_core::game::{Ability, Action, Game, State, Tone};
use delve_core::gear::Profile;
use delve_core::map::Pos;
use delve_core::save::{Score, Storage};
use macroquad::miniquad::conf::{Icon, Platform};
use macroquad::prelude::*;

use fx::{Fx, tile_center};
use menu::{Command, Menu, Page};
use render::{Ending, View};

/// Held keys repeat after this delay, then at this interval.
const REPEAT_DELAY: f64 = 0.22;
const REPEAT_EVERY: f64 = 0.075;
/// Pause between steps when walking to a clicked cell.
const TRAVEL_STEP: f32 = 0.07;

fn window_conf() -> Conf {
    Conf {
        window_title: "delve".into(),
        window_width: 1440,
        window_height: 900,
        window_resizable: true,
        high_dpi: true,
        icon: Some(icon()),
        // Lets the desktop match the window with delve.desktop, and show its icon.
        platform: Platform { linux_wm_class: "delve", ..Default::default() },
        ..Default::default()
    }
}

/// The window icon: the hero's sprite, enlarged without blur to each size.
fn icon() -> Icon {
    let sheet = Image::from_file_with_format(sprites::SHEET, Some(ImageFormat::Png)).expect("embedded sheet");
    let src = sprites::source(sprites::HERO);
    let scaled = |n: usize| {
        let mut out = vec![0; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let (sx, sy) = (src.x as u32 + (x * 16 / n) as u32, src.y as u32 + (y * 16 / n) as u32);
                let c: [u8; 4] = sheet.get_pixel(sx, sy).into();
                out[(y * n + x) * 4..][..4].copy_from_slice(&c);
            }
        }
        out
    };
    Icon {
        small: scaled(16).try_into().unwrap(),
        medium: scaled(32).try_into().unwrap(),
        big: scaled(64).try_into().unwrap(),
    }
}

fn random_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0) % 1_000_000
}

const MOVES: &[(KeyCode, (i32, i32))] = &[
    (KeyCode::Left, (-1, 0)),
    (KeyCode::H, (-1, 0)),
    (KeyCode::Kp4, (-1, 0)),
    (KeyCode::Right, (1, 0)),
    (KeyCode::L, (1, 0)),
    (KeyCode::Kp6, (1, 0)),
    (KeyCode::Up, (0, -1)),
    (KeyCode::K, (0, -1)),
    (KeyCode::Kp8, (0, -1)),
    (KeyCode::Down, (0, 1)),
    (KeyCode::J, (0, 1)),
    (KeyCode::Kp2, (0, 1)),
    (KeyCode::Y, (-1, -1)),
    (KeyCode::Kp7, (-1, -1)),
    (KeyCode::U, (1, -1)),
    (KeyCode::Kp9, (1, -1)),
    (KeyCode::B, (-1, 1)),
    (KeyCode::Kp1, (-1, 1)),
    (KeyCode::N, (1, 1)),
    (KeyCode::Kp3, (1, 1)),
];

/// A run under way, or just over.
struct Run {
    game: Game,
    fx: Fx,
    /// Set once the run is over and entered in the scores: the table, the rank of this
    /// run and the purse with its gold.
    ending: Option<(Vec<Score>, Option<usize>, i32)>,
    camera: Vec2,
    zoom: f32,
    /// Remaining steps of a click-to-move walk.
    travel: Vec<Pos>,
    travel_timer: f32,
    /// When each movement key went down, and when it last fired.
    held: HashMap<KeyCode, (f64, f64)>,
    /// The ability waiting for a direction, if any.
    aim: Option<Ability>,
}

/// The key of each ability, in [`Ability::ALL`] order.
const ABILITY_KEYS: [KeyCode; 3] = [KeyCode::D, KeyCode::F, KeyCode::T];

/// What the player asked for during a run.
enum Leave {
    Stay,
    Menu,
    NewRun,
}

impl Run {
    fn new(game: Game) -> Self {
        let camera = tile_center(game.player);
        Run {
            game,
            fx: Fx::default(),
            ending: None,
            camera,
            zoom: 3.0,
            travel: vec![],
            travel_timer: 0.0,
            held: HashMap::new(),
            aim: None,
        }
    }

    fn view(&self) -> View {
        let shake = if self.fx.shake > 0.0 {
            vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * self.fx.shake * 24.0
        } else {
            Vec2::ZERO
        };
        View { center: self.camera, tile: sprites::TILE * self.zoom, shake }
    }

    fn act(&mut self, action: Action, storage: &Storage) {
        let depth = self.game.depth;
        self.game.act(action);
        self.fx.on_events(&self.game);
        if self.game.depth != depth {
            self.travel.clear();
            self.camera = tile_center(self.game.player);
            // Autosave on each new floor, so a crash costs at most one floor.
            if let Err(e) = storage.save(&self.game) {
                self.game.say(format!("Could not save: {e}"), Tone::Bad);
            }
        }
        if self.game.state != State::Playing && self.ending.is_none() {
            self.travel.clear();
            let (scores, rank) = storage.finish(&self.game).unwrap_or_else(|e| {
                self.game.say(format!("Could not record the score: {e}"), Tone::Bad);
                (storage.scores(), None)
            });
            self.ending = Some((scores, rank, storage.profile().purse));
        }
    }

    /// Leaving mid-run keeps the run for next time.
    fn quit(&mut self, storage: &Storage) {
        if self.game.state == State::Playing
            && let Err(e) = storage.save(&self.game)
        {
            eprintln!("delve: could not save: {e}");
        }
    }

    /// Movement keys: one step per press, then repeating while held.
    fn held_move(&mut self) -> Option<(i32, i32)> {
        let now = get_time();
        let mut fired = None;
        for &(key, dir) in MOVES {
            if is_key_pressed(key) {
                self.held.insert(key, (now, now));
                fired = Some(dir);
            } else if is_key_down(key) {
                if let Some((down, last)) = self.held.get_mut(&key)
                    && now - *down > REPEAT_DELAY
                    && now - *last > REPEAT_EVERY
                {
                    *last = now;
                    fired = Some(dir);
                }
            } else {
                self.held.remove(&key);
            }
        }
        fired
    }

    fn handle_input(&mut self, typed: &[char], storage: &Storage) -> Leave {
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            self.zoom = (self.zoom + wheel.signum() * 0.5).clamp(1.5, 6.0);
        }
        if self.game.state != State::Playing {
            if is_key_pressed(KeyCode::R) {
                return Leave::NewRun;
            }
            let back = [KeyCode::Escape, KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space];
            if back.iter().any(|&k| is_key_pressed(k)) {
                return Leave::Menu;
            }
            return Leave::Stay;
        }
        if is_key_pressed(KeyCode::Escape) {
            if self.aim.take().is_some() {
                return Leave::Stay;
            }
            self.quit(storage);
            return Leave::Menu;
        }

        // An ability's key starts aiming it; the next direction, or a click, fires it.
        for (key, ability) in ABILITY_KEYS.into_iter().zip(Ability::ALL) {
            if is_key_pressed(key) {
                match self.game.can_use(ability) {
                    Ok(()) => {
                        self.aim = if self.aim == Some(ability) { None } else { Some(ability) };
                        self.travel.clear();
                    }
                    Err(why) => self.game.say(why, Tone::Normal),
                }
            }
        }
        if let Some(ability) = self.aim {
            let mut dir = self.held_move();
            if is_mouse_button_pressed(MouseButton::Left) {
                let (cell, me) = (self.view().to_cell(ui::mouse()), self.game.player);
                if cell != me {
                    dir = Some(((cell.x - me.x).signum(), (cell.y - me.y).signum()));
                }
            }
            if let Some((dx, dy)) = dir {
                self.aim = None;
                self.act(Action::Use(ability, dx, dy), storage);
            }
            return Leave::Stay;
        }

        let mut action = self.held_move().map(|(dx, dy)| Action::Move(dx, dy));
        for &c in typed {
            action = match c {
                '>' => Some(Action::Descend),
                '<' => Some(Action::Escape),
                '.' | ' ' | '5' => Some(Action::Wait),
                'p' | 'P' => Some(Action::Drink),
                _ => action,
            };
        }
        // Enter takes whatever way lies underfoot.
        let here = self.game.map.tile(self.game.player);
        let way = if here == delve_core::map::Tile::Exit { Action::Escape } else { Action::Descend };
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            action = Some(way);
        }
        if action.is_some() {
            self.travel.clear();
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            let cell = self.view().to_cell(ui::mouse());
            let me = self.game.player;
            if cell.distance(me) == 1 {
                action = Some(Action::Move(cell.x - me.x, cell.y - me.y));
                self.travel.clear();
            } else if cell == me && here != delve_core::map::Tile::Floor {
                action = Some(way);
            } else if let Some(path) = self.game.path_to(cell) {
                self.travel = path.into_iter().rev().collect();
                self.travel_timer = 0.0;
            }
        }

        if let Some(action) = action {
            self.act(action, storage);
        } else {
            self.walk(storage);
        }
        Leave::Stay
    }

    /// One step of a click-to-move walk, stopping as soon as something shows up.
    fn walk(&mut self, storage: &Storage) {
        self.travel_timer -= get_frame_time();
        if self.travel_timer > 0.0 {
            return;
        }
        let Some(next) = self.travel.pop() else { return };
        self.travel_timer = TRAVEL_STEP;
        let me = self.game.player;
        if next.distance(me) != 1 || self.game.monster_at(next).is_some() {
            self.travel.clear();
            return;
        }
        let seen_before = self.visible_monsters();
        let hp = self.game.stats.hp;
        self.act(Action::Move(next.x - me.x, next.y - me.y), storage);
        let danger = self.game.threatened(self.game.player);
        if self.visible_monsters() > seen_before || self.game.stats.hp < hp || self.game.player != next || danger {
            self.travel.clear();
            self.game.say("You stop: something is nearby.", Tone::Normal);
        }
    }

    fn visible_monsters(&self) -> usize {
        self.game.monsters.iter().filter(|m| self.game.map.is_visible(m.pos)).count()
    }

    fn update(&mut self) {
        let dt = get_frame_time();
        self.fx.update(&self.game, dt);
        // The camera trails the hero smoothly.
        let target = self.fx.hero(&self.game);
        self.camera += (target - self.camera) * (dt * 8.0).min(1.0);
    }

    fn draw(&self, sheet: &Texture2D) {
        let view = self.view();
        let hover = Some(view.to_cell(ui::mouse()));
        let ending = self.ending.as_ref().map(|(scores, rank, purse)| Ending { scores, rank: *rank, purse: *purse });
        render::draw(&self.game, &self.fx, &view, sheet, hover, self.aim, ending);
    }
}

/// For screenshots: walks the hero next to the nearest monster and trades a few blows,
/// so that the picture shows creatures and effects.
fn stage_fight(app: &mut Run, storage: &Storage) {
    let Some(target) = app.game.monsters.iter().map(|m| m.pos).min_by_key(|p| p.distance(app.game.player)) else {
        return;
    };
    let dist = app.game.map.distances(target, |_| true);
    let next_to = delve_core::map::Map::neighbors(target)
        .filter(|&p| app.game.map.tile(p).walkable() && app.game.monster_at(p).is_none())
        .min_by_key(|&p| dist[delve_core::map::Map::index(p)]);
    if let Some(p) = next_to {
        app.game.player = p;
        app.game.refresh_sight();
        app.camera = tile_center(p);
        for _ in 0..2 {
            app.act(Action::Move(target.x - p.x, target.y - p.y), storage);
        }
    }
    // An orc joins in and marks its cleave, so that the picture shows a coming blow.
    let me = app.game.player;
    let free = delve_core::map::Map::neighbors(me)
        .filter(|&p| app.game.map.tile(p).walkable() && app.game.monster_at(p).is_none())
        .max_by_key(|&p| (p.x - me.x).abs() + (p.y - me.y).abs() == 1);
    if let Some(pos) = free {
        add_monster(app, "orc", pos, 99);
        app.act(Action::Wait, storage);
    }
}

fn add_monster(app: &mut Run, name: &str, pos: Pos, hp: i32) {
    let kind = delve_core::game::BESTIARY.iter().find(|k| k.name == name).unwrap();
    let stats = delve_core::game::Stats { hp, max_hp: hp, attack: kind.attack, defense: kind.defense };
    let monster = delve_core::game::Monster {
        id: 900 + app.game.monsters.len() as u32,
        kind,
        pos,
        stats,
        awake: true,
        intent: None,
        stunned: 0,
    };
    app.game.monsters.push(monster);
}

/// The whole program: the menus, and the run being played if any.
struct Delve {
    storage: Storage,
    profile: Profile,
    sheet: Texture2D,
    menu: Menu,
    run: Option<Run>,
}

impl Delve {
    fn new(storage: Storage, sheet: Texture2D) -> Self {
        let profile = storage.profile();
        ui::set_user_scale(profile.ui_scale);
        Delve { storage, profile, sheet, menu: Menu::new(), run: None }
    }

    fn new_run(&mut self) {
        let game = Game::equipped(random_seed(), &self.profile.loadout());
        self.run = Some(Run::new(game));
    }

    fn resume(&mut self) {
        match self.storage.load() {
            Ok(Some(mut game)) => {
                game.say("Welcome back.", Tone::Normal);
                self.run = Some(Run::new(game));
            }
            Ok(None) => self.new_run(),
            Err(e) => self.menu.notify(e.to_string(), ui::BAD),
        }
    }

    /// `+` and `-` resize the interface on every screen; the choice is kept.
    fn resize(&mut self, typed: &[char]) {
        let step = typed.iter().map(|c| match c {
            '+' | '=' => 0.1,
            '-' => -0.1,
            _ => 0.0,
        });
        let step: f32 = step.sum();
        if step != 0.0 {
            ui::set_user_scale(ui::user_scale() + step);
            self.profile.ui_scale = ui::user_scale();
            if let Err(e) = self.storage.save_profile(&self.profile) {
                eprintln!("delve: could not save the settings: {e}");
            }
        }
    }

    /// One frame; false once the player has quit.
    fn frame(&mut self) -> bool {
        let mut typed = vec![];
        while let Some(c) = get_char_pressed() {
            typed.push(c);
        }
        self.resize(&typed);

        if let Some(run) = &mut self.run {
            let leave = run.handle_input(&typed, &self.storage);
            run.update();
            run.draw(&self.sheet);
            match leave {
                Leave::Stay => {}
                Leave::Menu => {
                    self.run = None;
                    self.profile = self.storage.profile();
                }
                Leave::NewRun => {
                    self.profile = self.storage.profile();
                    self.new_run();
                }
            }
            return true;
        }

        let scores = self.storage.scores();
        let has_save = self.storage.has_save();
        match self.menu.frame(&self.sheet, &self.profile, &scores, has_save) {
            None => {}
            Some(Command::Continue) => self.resume(),
            Some(Command::NewRun) => self.new_run(),
            Some(Command::Quit) => return false,
            Some(Command::Go(page)) => self.menu.page = page,
            Some(Command::Buy(gear)) => match self.profile.buy(gear) {
                Ok(()) => {
                    if let Err(e) = self.storage.save_profile(&self.profile) {
                        self.menu.notify(format!("Could not save: {e}"), ui::BAD);
                    } else {
                        self.menu.notify(format!("You bought the {}.", gear.name), ui::GOOD);
                    }
                }
                Err(e) => self.menu.notify(format!("{}: {e}.", gear.name), ui::BAD),
            },
        }
        true
    }

    fn quit(&mut self) {
        if let Some(run) = &mut self.run {
            run.quit(&self.storage);
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let sheet = Texture2D::from_file_with_format(sprites::SHEET, Some(ImageFormat::Png));
    sheet.set_filter(FilterMode::Nearest);
    ui::load_fonts();
    prevent_quit();

    // `DELVE_SCREENSHOT=out.png delve` renders a few frames of a fixed scene, saves one
    // and quits, away from the real save; used for the README. `DELVE_SCENE` picks the
    // scene: `fight` (the default), `aim`, `exit`, `death`, `menu` or `armory`.
    let screenshot = std::env::var_os("DELVE_SCREENSHOT");
    if let Some(path) = screenshot {
        let dir = std::env::temp_dir().join("delve-screenshot");
        let _ = std::fs::remove_dir_all(&dir);
        let mut delve = Delve::new(Storage::new(dir), sheet);
        stage(&mut delve, std::env::var("DELVE_SCENE").as_deref().unwrap_or("fight"));
        let mut frame = 0;
        loop {
            frame += 1;
            delve.frame();
            // Read the picture before `next_frame` presents it and starts the next one.
            if frame == 12 {
                // Translucent layers leave holes in the framebuffer's alpha, which a window
                // ignores but a PNG keeps.
                let mut picture = get_screen_data();
                picture.bytes.as_chunks_mut::<4>().0.iter_mut().for_each(|px| px[3] = 255);
                picture.export_png(&path.to_string_lossy());
                return;
            }
            next_frame().await;
        }
    }

    let mut delve = Delve::new(Storage::default_location(), sheet);
    loop {
        if is_quit_requested() {
            delve.quit();
            break;
        }
        if !delve.frame() {
            delve.quit();
            break;
        }
        next_frame().await;
    }
}

/// Sets up the scene of a screenshot.
fn stage(delve: &mut Delve, scene: &str) {
    let mut profile = Profile::new();
    profile.purse = 340;
    for name in ["short sword", "wooden shield", "potion belt"] {
        profile.buy(delve_core::gear::by_name(name).unwrap()).ok();
        profile.purse += 100;
    }
    delve.profile = profile;
    match scene {
        "menu" => {}
        "armory" => {
            delve.menu.page = Page::Armory;
        }
        _ => {
            let mut run = Run::new(Game::equipped(7, &delve.profile.loadout()));
            if scene == "exit" {
                // Down to the first way out, the hero standing next to it.
                while !Game::has_exit(run.game.depth) {
                    run.game.player = run.game.map.rooms.last().unwrap().center();
                    run.act(Action::Descend, &delve.storage);
                }
                let exit = run.game.map.rooms[run.game.map.rooms.len() / 2].center();
                run.game.monsters.clear();
                run.game.player = Pos::new(exit.x - 1, exit.y);
                run.game.refresh_sight();
                run.camera = tile_center(run.game.player);
                delve.run = Some(run);
                return;
            }
            stage_fight(&mut run, &delve.storage);
            if scene == "death" {
                let me = run.game.player;
                let free = delve_core::map::Map::neighbors(me)
                    .find(|&p| run.game.map.tile(p).walkable() && run.game.monster_at(p).is_none());
                if let Some(pos) = free {
                    add_monster(&mut run, "troll", pos, 99);
                    run.game.monsters.last_mut().unwrap().stats.attack = 99;
                }
                for _ in 0..20 {
                    if run.game.state != State::Playing {
                        break;
                    }
                    run.act(Action::Wait, &delve.storage);
                }
            }
            if scene == "aim" {
                run.aim = Some(Ability::Throw);
            }
            delve.run = Some(run);
        }
    }
}
