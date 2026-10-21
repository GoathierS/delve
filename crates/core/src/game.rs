//! The rules: the hero, monsters, items, combat and turns.

use serde::{Deserialize, Serialize};

use crate::gear::{self, Gear};
use crate::map::{Map, Pos, Tile};
use crate::rng::Rng;

pub const FOV_RADIUS: i32 = 8;
/// The Amulet lies on this floor; bringing it back is not required, picking it up wins.
pub const LAST_DEPTH: i32 = 10;
const LOG_LEN: usize = 100;
/// A new hero, before equipment.
pub const START_STATS: Stats = Stats { hp: 30, max_hp: 30, attack: 5, defense: 1 };
pub const START_POTIONS: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub hp: i32,
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    pub glyph: char,
    pub hp: i32,
    pub attack: i32,
    pub defense: i32,
    pub xp: i32,
    /// Shallowest floor it appears on.
    pub min_depth: i32,
}

pub const BESTIARY: &[Kind] = &[
    Kind { name: "rat", glyph: 'r', hp: 5, attack: 2, defense: 0, xp: 1, min_depth: 1 },
    Kind { name: "kobold", glyph: 'k', hp: 8, attack: 3, defense: 0, xp: 2, min_depth: 1 },
    Kind { name: "goblin", glyph: 'g', hp: 10, attack: 4, defense: 1, xp: 3, min_depth: 2 },
    Kind { name: "orc", glyph: 'o', hp: 16, attack: 5, defense: 2, xp: 5, min_depth: 3 },
    Kind { name: "giant spider", glyph: 's', hp: 14, attack: 7, defense: 1, xp: 6, min_depth: 4 },
    Kind { name: "troll", glyph: 'T', hp: 28, attack: 7, defense: 3, xp: 10, min_depth: 5 },
    Kind { name: "wraith", glyph: 'W', hp: 22, attack: 9, defense: 2, xp: 12, min_depth: 7 },
    Kind { name: "demon", glyph: 'D', hp: 60, attack: 12, defense: 5, xp: 40, min_depth: 9 },
];

/// Saves name the kind of a monster; the bestiary itself lives in the code.
mod kind_by_name {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    use super::{BESTIARY, Kind};

    pub fn serialize<S: Serializer>(kind: &&'static Kind, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(kind.name)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<&'static Kind, D::Error> {
        let name = String::deserialize(d)?;
        BESTIARY.iter().find(|k| k.name == name).ok_or_else(|| D::Error::custom(format!("unknown monster {name}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    /// Stable while the monster lives, so that a frontend can follow it between turns.
    pub id: u32,
    #[serde(with = "kind_by_name")]
    pub kind: &'static Kind,
    pub pos: Pos,
    pub stats: Stats,
    /// Has noticed the hero and hunts them down.
    pub awake: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Potion,
    Gold(i32),
    Amulet,
}

impl ItemKind {
    pub fn glyph(self) -> char {
        match self {
            ItemKind::Potion => '!',
            ItemKind::Gold(_) => '$',
            ItemKind::Amulet => '"',
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Playing,
    Dead,
    Won,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Move(i32, i32),
    Wait,
    Descend,
    Drink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    Normal,
    Good,
    Bad,
}

/// What happened during the last action, for frontends to animate. Not saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// `target` lost `amount` HP; `to_hero` when the hero was hit.
    Hit {
        target: Pos,
        amount: i32,
        to_hero: bool,
    },
    Killed {
        pos: Pos,
        kind: &'static Kind,
    },
    Healed {
        amount: i32,
    },
    LevelUp,
    PickedUp {
        pos: Pos,
        kind: ItemKind,
    },
    Descended,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    rng: Rng,
    pub seed: u64,
    pub map: Map,
    pub depth: i32,
    pub player: Pos,
    pub stats: Stats,
    pub level: i32,
    pub xp: i32,
    pub potions: i32,
    pub gold: i32,
    pub kills: i32,
    pub turn: u32,
    pub monsters: Vec<Monster>,
    pub items: Vec<Item>,
    pub log: Vec<(String, Tone)>,
    pub state: State,
    /// Who dealt the last blow, for the epitaph.
    pub killed_by: Option<String>,
    /// Names of the equipment the hero set out with.
    #[serde(default)]
    pub gear: Vec<String>,
    next_id: u32,
    #[serde(skip)]
    pub events: Vec<Event>,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self::equipped(seed, &[])
    }

    /// A new run whose hero carries `gear`, bought in the armory.
    pub fn equipped(seed: u64, gear: &[&Gear]) -> Self {
        let (stats, potions) = gear::equip(START_STATS, START_POTIONS, gear);
        let mut game = Game {
            rng: Rng::new(seed),
            seed,
            map: Map::generate(&mut Rng::new(seed)),
            depth: 0,
            player: Pos::new(0, 0),
            stats,
            level: 1,
            xp: 0,
            potions,
            gold: 0,
            kills: 0,
            turn: 0,
            monsters: vec![],
            items: vec![],
            log: vec![],
            state: State::Playing,
            killed_by: None,
            gear: gear.iter().map(|g| g.name.to_string()).collect(),
            next_id: 0,
            events: vec![],
        };
        game.enter_next_floor();
        game.say("You enter the dungeon. Find the Amulet on floor 10.", Tone::Normal);
        game
    }

    /// The equipment the hero set out with.
    pub fn gear(&self) -> impl Iterator<Item = &'static Gear> + '_ {
        self.gear.iter().filter_map(|name| gear::by_name(name))
    }

    pub fn xp_to_next(&self) -> i32 {
        self.level * 10
    }

    /// Adds a line to the message log.
    pub fn say(&mut self, msg: impl Into<String>, tone: Tone) {
        self.log.push((msg.into(), tone));
        if self.log.len() > LOG_LEN {
            self.log.remove(0);
        }
    }

    fn enter_next_floor(&mut self) {
        self.depth += 1;
        self.map = Map::generate(&mut self.rng);
        self.monsters.clear();
        self.items.clear();
        let rooms = self.map.rooms.clone();
        self.player = rooms[0].center();
        if self.depth == LAST_DEPTH {
            // No way further down: the Amulet waits where the stairs would be.
            let end = rooms.last().unwrap().center();
            self.map.set(end, Tile::Floor);
            self.items.push(Item { kind: ItemKind::Amulet, pos: end });
        }
        for room in &rooms[1..] {
            for _ in 0..self.rng.range(0, 1 + self.depth / 3) {
                let pos = room.random_cell(&mut self.rng);
                if self.free(pos) {
                    let kind = self.random_kind();
                    let stats = Stats { hp: kind.hp, max_hp: kind.hp, attack: kind.attack, defense: kind.defense };
                    self.next_id += 1;
                    self.monsters.push(Monster { id: self.next_id, kind, pos, stats, awake: false });
                }
            }
            if self.rng.chance(35) {
                let pos = room.random_cell(&mut self.rng);
                let kind = if self.rng.chance(40) {
                    ItemKind::Potion
                } else {
                    ItemKind::Gold(self.rng.range(5, 15) * self.depth)
                };
                if self.map.tile(pos) == Tile::Floor && self.item_at(pos).is_none() {
                    self.items.push(Item { kind, pos });
                }
            }
        }
        self.map.compute_fov(self.player, FOV_RADIUS);
    }

    /// Mostly monsters of this depth, sometimes a weaker one.
    fn random_kind(&mut self) -> &'static Kind {
        let allowed: Vec<&'static Kind> = BESTIARY.iter().filter(|k| k.min_depth <= self.depth).collect();
        let n = allowed.len() as i32;
        let i = if self.rng.chance(60) { self.rng.range((n - 2).max(0), n - 1) } else { self.rng.range(0, n - 1) };
        allowed[i as usize]
    }

    fn free(&self, p: Pos) -> bool {
        self.map.tile(p).walkable() && p != self.player && self.monster_at(p).is_none()
    }

    pub fn monster_at(&self, p: Pos) -> Option<usize> {
        self.monsters.iter().position(|m| m.pos == p)
    }

    pub fn item_at(&self, p: Pos) -> Option<usize> {
        self.items.iter().position(|i| i.pos == p)
    }

    fn damage(&mut self, attack: i32, defense: i32) -> i32 {
        (attack + self.rng.range(-1, 2) - defense).max(1)
    }

    /// Plays one hero action, then the monsters' turn if the action took time.
    pub fn act(&mut self, action: Action) {
        self.events.clear();
        if self.state != State::Playing {
            return;
        }
        let took_time = match action {
            Action::Move(dx, dy) => self.player_step(dx, dy),
            Action::Wait => true,
            Action::Descend => {
                if self.map.tile(self.player) == Tile::StairsDown {
                    self.enter_next_floor();
                    self.events.push(Event::Descended);
                    self.say(format!("You descend to floor {}.", self.depth), Tone::Normal);
                    return;
                }
                self.say("There are no stairs here.", Tone::Normal);
                false
            }
            Action::Drink => {
                if self.potions == 0 {
                    self.say("You have no potion.", Tone::Normal);
                    false
                } else {
                    self.potions -= 1;
                    let heal = self.stats.max_hp / 2;
                    self.stats.hp = (self.stats.hp + heal).min(self.stats.max_hp);
                    self.events.push(Event::Healed { amount: heal });
                    self.say(format!("You drink a potion and recover {heal} HP."), Tone::Good);
                    true
                }
            }
        };
        if took_time && self.state == State::Playing {
            self.end_turn();
        }
    }

    fn player_step(&mut self, dx: i32, dy: i32) -> bool {
        let to = Pos::new(self.player.x + dx, self.player.y + dy);
        if let Some(i) = self.monster_at(to) {
            self.player_attack(i);
            return true;
        }
        if !self.map.tile(to).walkable() {
            return false;
        }
        self.player = to;
        self.pick_up();
        if self.map.tile(to) == Tile::StairsDown {
            self.say("Stairs lead down here (press >).", Tone::Normal);
        }
        true
    }

    fn pick_up(&mut self) {
        let Some(i) = self.item_at(self.player) else { return };
        let item = self.items.remove(i);
        self.events.push(Event::PickedUp { pos: item.pos, kind: item.kind });
        match item.kind {
            ItemKind::Potion => {
                self.potions += 1;
                self.say("You pick up a healing potion.", Tone::Good);
            }
            ItemKind::Gold(n) => {
                self.gold += n;
                self.say(format!("You pick up {n} gold."), Tone::Good);
            }
            ItemKind::Amulet => {
                self.say("You seize the Amulet! You have won.", Tone::Good);
                self.state = State::Won;
            }
        }
    }

    fn player_attack(&mut self, i: usize) {
        let defense = self.monsters[i].stats.defense;
        let dmg = self.damage(self.stats.attack, defense);
        let m = &mut self.monsters[i];
        m.stats.hp -= dmg;
        m.awake = true;
        let (name, kind, pos) = (m.kind.name, m.kind, m.pos);
        self.events.push(Event::Hit { target: pos, amount: dmg, to_hero: false });
        if m.stats.hp > 0 {
            self.say(format!("You hit the {name} for {dmg}."), Tone::Normal);
            return;
        }
        let xp = m.kind.xp;
        self.monsters.remove(i);
        self.events.push(Event::Killed { pos, kind });
        self.kills += 1;
        self.say(format!("You kill the {name}."), Tone::Good);
        self.gain_xp(xp);
    }

    fn gain_xp(&mut self, xp: i32) {
        self.xp += xp;
        while self.xp >= self.xp_to_next() {
            self.xp -= self.xp_to_next();
            self.level += 1;
            self.stats.max_hp += 8;
            self.stats.hp = self.stats.max_hp;
            self.stats.attack += 1;
            if self.level % 3 == 0 {
                self.stats.defense += 1;
            }
            self.events.push(Event::LevelUp);
            self.say(format!("You reach level {}! You feel stronger.", self.level), Tone::Good);
        }
    }

    /// The steps to walk to `target` through explored cells, for click-to-move. Empty if
    /// the hero is already there, `None` if no known way leads there.
    pub fn path_to(&self, target: Pos) -> Option<Vec<Pos>> {
        if !self.map.is_explored(target) || !self.map.tile(target).walkable() {
            return None;
        }
        let dist = self.map.distances(target, |p| self.map.is_explored(p));
        if dist[Map::index(self.player)] < 0 {
            return None;
        }
        let mut path = vec![];
        let mut at = self.player;
        while at != target {
            at = Map::neighbors(at).filter(|&p| dist[Map::index(p)] >= 0).min_by_key(|&p| dist[Map::index(p)])?;
            path.push(at);
        }
        Some(path)
    }

    pub fn score(&self) -> i32 {
        self.depth * 100 + self.gold + self.kills * 10 + if self.state == State::Won { 1000 } else { 0 }
    }

    fn end_turn(&mut self) {
        self.turn += 1;
        self.map.compute_fov(self.player, FOV_RADIUS);
        // Every awake monster walks down this map: unlike a straight line, it goes
        // around corners and pillars.
        let to_hero = self.map.distances(self.player, |_| true);
        for i in 0..self.monsters.len() {
            self.monster_turn(i, &to_hero);
            if self.state != State::Playing {
                return;
            }
        }
        if self.turn.is_multiple_of(8) && self.stats.hp < self.stats.max_hp {
            self.stats.hp += 1;
        }
    }

    fn monster_turn(&mut self, i: usize, to_hero: &[i32]) {
        let m = &self.monsters[i];
        if !m.awake {
            if !self.map.is_visible(m.pos) {
                return;
            }
            self.monsters[i].awake = true;
        }
        let m = &self.monsters[i];
        if m.pos.distance(self.player) == 1 {
            let (attack, name) = (m.stats.attack, m.kind.name);
            let dmg = self.damage(attack, self.stats.defense);
            self.stats.hp -= dmg;
            self.events.push(Event::Hit { target: self.player, amount: dmg, to_hero: true });
            self.say(format!("The {name} hits you for {dmg}."), Tone::Bad);
            if self.stats.hp <= 0 {
                self.stats.hp = 0;
                self.killed_by = Some(name.to_string());
                self.state = State::Dead;
                self.say("You die...", Tone::Bad);
            }
            return;
        }
        let from = m.pos;
        let dist = |p: Pos| to_hero[Map::index(p)];
        // Prefer straight steps on ties: diagonal zigzags look odd.
        let best = Map::neighbors(from)
            .filter(|&p| self.free(p) && dist(p) >= 0)
            .min_by_key(|&p| (dist(p), (p.x - from.x).abs() + (p.y - from.y).abs()));
        if let Some(p) = best.filter(|&p| dist(from) < 0 || dist(p) < dist(from)) {
            self.monsters[i].pos = p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_game() {
        let (a, b) = (Game::new(9), Game::new(9));
        assert_eq!(a.player, b.player);
        assert_eq!(
            a.monsters.iter().map(|m| m.pos).collect::<Vec<_>>(),
            b.monsters.iter().map(|m| m.pos).collect::<Vec<_>>()
        );
    }

    #[test]
    fn walls_stop_the_hero() {
        let mut g = Game::new(3);
        g.monsters.clear();
        let room = g.map.rooms[0];
        g.player = Pos::new(room.x, room.y);
        if g.map.tile(Pos::new(room.x - 1, room.y)) == Tile::Wall {
            g.act(Action::Move(-1, 0));
            assert_eq!(g.player, Pos::new(room.x, room.y));
        }
    }

    #[test]
    fn killing_gives_xp_and_levels_up() {
        let mut g = Game::new(4);
        g.gain_xp(10);
        assert_eq!(g.level, 2);
        assert_eq!(g.stats.max_hp, 38);
        assert_eq!(g.stats.hp, 38);
    }

    #[test]
    fn potion_heals_without_exceeding_max() {
        let mut g = Game::new(5);
        g.monsters.clear();
        g.stats.hp = 25;
        g.act(Action::Drink);
        assert_eq!(g.stats.hp, 30);
        assert_eq!(g.potions, 0);
        g.act(Action::Drink);
        assert_eq!(g.potions, 0);
    }

    #[test]
    fn stairs_lead_deeper_and_amulet_wins() {
        let mut g = Game::new(6);
        while g.depth < LAST_DEPTH {
            g.player = g.map.rooms.last().unwrap().center();
            g.act(Action::Descend);
        }
        assert_eq!(g.depth, LAST_DEPTH);
        let amulet = g.items.iter().find(|i| i.kind == ItemKind::Amulet).unwrap().pos;
        g.monsters.clear();
        g.player = Pos::new(amulet.x - 1, amulet.y);
        g.act(Action::Move(1, 0));
        assert_eq!(g.state, State::Won);
    }

    #[test]
    fn monsters_can_kill_the_hero() {
        let mut g = Game::new(8);
        g.monsters.clear();
        let kind = &BESTIARY[BESTIARY.len() - 1];
        let pos = Pos::new(g.player.x + 1, g.player.y);
        let stats = Stats { hp: 999, max_hp: 999, attack: 999, defense: 0 };
        g.monsters.push(Monster { id: 99, kind, pos, stats, awake: true });
        g.act(Action::Wait);
        assert_eq!(g.state, State::Dead);
        assert_eq!(g.killed_by.as_deref(), Some("demon"));
    }

    #[test]
    fn monsters_walk_around_walls() {
        let mut g = Game::new(11);
        g.monsters.clear();
        // Put a monster in the next room: the straight line to the hero crosses walls,
        // yet it must reach them by the corridor.
        let pos = g.map.rooms[1].center();
        let stats = Stats { hp: 50, max_hp: 50, attack: 0, defense: 0 };
        g.monsters.push(Monster { id: 1, kind: &BESTIARY[0], pos, stats, awake: true });
        for _ in 0..200 {
            g.act(Action::Wait);
            if g.monsters[0].pos.distance(g.player) == 1 {
                return;
            }
        }
        panic!("the monster never reached the hero");
    }

    #[test]
    fn path_to_follows_explored_cells() {
        let mut g = Game::new(12);
        g.monsters.clear();
        let room = g.map.rooms[0];
        let target = Pos::new(room.x, room.y);
        let path = g.path_to(target).unwrap();
        assert_eq!(path.last(), Some(&target));
        assert!(path.windows(2).all(|w| w[0].distance(w[1]) == 1));
        // The far end of the dungeon is unexplored: no known way there.
        assert_eq!(g.path_to(g.map.rooms.last().unwrap().center()), None);
    }

    #[test]
    fn events_describe_the_last_action() {
        let mut g = Game::new(13);
        g.monsters.clear();
        g.stats.hp = 10;
        g.act(Action::Drink);
        assert_eq!(g.events, [Event::Healed { amount: 15 }]);
        g.act(Action::Wait);
        assert!(g.events.is_empty());
    }
}
