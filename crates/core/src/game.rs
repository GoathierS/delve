//! The rules: the hero, monsters, items, combat and turns.
//!
//! Two ideas shape a run. The hero's torch burns down turn after turn and lights less and
//! less; oil feeds it, and once it dies the dark itself sends shades. And monsters show
//! their next blow before they strike: each one marks the cells it is about to hit, so a
//! fight is about where to stand, with three abilities to dodge, push and burn.

use serde::{Deserialize, Serialize};

use crate::gear::{self, Gear};
use crate::map::{Map, Pos, Tile};
use crate::rng::Rng;

/// How far a full torch lights.
pub const FOV_RADIUS: i32 = 8;
/// The Amulet lies on this floor; bringing it back is not required, picking it up wins.
pub const LAST_DEPTH: i32 = 10;
const LOG_LEN: usize = 100;
/// A new hero, before equipment.
pub const START_STATS: Stats = Stats { hp: 30, max_hp: 30, attack: 5, defense: 1 };
pub const START_POTIONS: i32 = 1;
/// Turns a full torch lasts.
pub const TORCH_MAX: i32 = 250;
/// Turns of light in a flask of oil.
pub const OIL: i32 = 100;
/// Torch burnt by throwing fire.
pub const THROW_COST: i32 = 40;
const THROW_RANGE: i32 = 5;
/// Once the torch is out, a shade rises every this many turns.
const SHADE_EVERY: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub hp: i32,
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
}

/// The shape of a monster's blow, which decides the cells it marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// The hero's cell, from next to them.
    Bite,
    /// Two cells in a straight line: reaches the hero from two cells away.
    Lunge,
    /// A sweep over every cell next to both the monster and the hero.
    Cleave,
    /// Every cell around the monster, hard.
    Smash,
    /// Four cells in a straight line.
    Bolt,
    /// The hero's cell and all around it, from up to three cells away.
    Blast,
}

impl Pattern {
    pub fn name(self) -> &'static str {
        match self {
            Pattern::Bite => "bite",
            Pattern::Lunge => "lunge",
            Pattern::Cleave => "cleave",
            Pattern::Smash => "smash",
            Pattern::Bolt => "bolt",
            Pattern::Blast => "fire blast",
        }
    }

    /// Damage, in percent of the monster's attack.
    fn power(self) -> i32 {
        match self {
            Pattern::Smash => 150,
            Pattern::Blast => 120,
            _ => 100,
        }
    }
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
    pub pattern: Pattern,
}

pub const BESTIARY: &[Kind] = &[
    Kind { name: "rat", glyph: 'r', hp: 5, attack: 2, defense: 0, xp: 1, min_depth: 1, pattern: Pattern::Bite },
    Kind { name: "kobold", glyph: 'k', hp: 8, attack: 3, defense: 0, xp: 2, min_depth: 1, pattern: Pattern::Bite },
    Kind { name: "goblin", glyph: 'g', hp: 10, attack: 4, defense: 1, xp: 3, min_depth: 2, pattern: Pattern::Lunge },
    Kind { name: "orc", glyph: 'o', hp: 16, attack: 5, defense: 2, xp: 5, min_depth: 3, pattern: Pattern::Cleave },
    Kind {
        name: "giant spider",
        glyph: 's',
        hp: 14,
        attack: 7,
        defense: 1,
        xp: 6,
        min_depth: 4,
        pattern: Pattern::Lunge,
    },
    Kind { name: "troll", glyph: 'T', hp: 28, attack: 7, defense: 3, xp: 10, min_depth: 5, pattern: Pattern::Smash },
    Kind { name: "wraith", glyph: 'W', hp: 22, attack: 9, defense: 2, xp: 12, min_depth: 7, pattern: Pattern::Bolt },
    Kind { name: "demon", glyph: 'D', hp: 60, attack: 12, defense: 5, xp: 40, min_depth: 9, pattern: Pattern::Blast },
    // Born of the dark when the torch dies; never placed with a floor.
    Kind {
        name: "shade",
        glyph: 'S',
        hp: 12,
        attack: 6,
        defense: 0,
        xp: 2,
        min_depth: i32::MAX,
        pattern: Pattern::Bite,
    },
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
    /// The cells its next blow will hit, at the end of the hero's next action.
    #[serde(default)]
    pub intent: Option<Vec<Pos>>,
    /// Turns it stays dazed, doing nothing.
    #[serde(default)]
    pub stunned: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Potion,
    Gold(i32),
    Amulet,
    /// Feeds the torch.
    Oil,
}

impl ItemKind {
    pub fn glyph(self) -> char {
        match self {
            ItemKind::Potion => '!',
            ItemKind::Gold(_) => '$',
            ItemKind::Amulet => '"',
            ItemKind::Oil => '%',
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

/// What the hero can do besides walking and bumping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ability {
    /// Two cells at once, out of reach of most blows.
    Dash,
    /// Shoves a monster back a cell: into a wall it is hurt, into another monster both
    /// are; either way it loses its blow.
    Bash,
    /// Hurls fire down a line from the torch, at the cost of some of its light.
    Throw,
}

impl Ability {
    pub const ALL: [Ability; 3] = [Ability::Dash, Ability::Bash, Ability::Throw];

    pub fn name(self) -> &'static str {
        match self {
            Ability::Dash => "dash",
            Ability::Bash => "shield bash",
            Ability::Throw => "throw fire",
        }
    }

    /// Turns before it can be used again.
    pub fn cooldown(self) -> u32 {
        match self {
            Ability::Dash => 6,
            Ability::Bash => 5,
            Ability::Throw => 8,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Move(i32, i32),
    Wait,
    Descend,
    Drink,
    /// An ability, aimed in a direction.
    Use(Ability, i32, i32),
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
    /// A marked blow found nobody: the hero stepped out of it.
    Dodged {
        pos: Pos,
    },
    Stunned {
        pos: Pos,
    },
    /// Fire flew along these cells.
    Fire {
        path: Vec<Pos>,
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
    /// A shade rose from the dark.
    Shade {
        pos: Pos,
    },
}

fn full_torch() -> i32 {
    TORCH_MAX
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
    /// Turns of light left.
    #[serde(default = "full_torch")]
    pub torch: i32,
    /// Turns spent without light, which call the shades.
    #[serde(default)]
    dark_turns: u32,
    /// Turns before each ability, in [`Ability::ALL`] order, can be used again.
    #[serde(default)]
    pub cooldowns: [u32; 3],
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
            torch: TORCH_MAX,
            dark_turns: 0,
            cooldowns: [0; 3],
            next_id: 0,
            events: vec![],
        };
        game.enter_next_floor();
        game.say("You enter the dungeon. Find the Amulet on floor 10.", Tone::Normal);
        game.say("Mind your torch: oil keeps it burning.", Tone::Normal);
        game
    }

    /// The equipment the hero set out with.
    pub fn gear(&self) -> impl Iterator<Item = &'static Gear> + '_ {
        self.gear.iter().filter_map(|name| gear::by_name(name))
    }

    pub fn xp_to_next(&self) -> i32 {
        self.level * 10
    }

    /// How far the torch lights: less and less as it burns down, barely anything once out.
    pub fn light_radius(&self) -> i32 {
        if self.torch <= 0 { 1 } else { 3 + (5 * self.torch + TORCH_MAX - 1) / TORCH_MAX }
    }

    /// Turns before `ability` can be used again; 0 when ready.
    pub fn cooldown(&self, ability: Ability) -> u32 {
        self.cooldowns[ability.index()]
    }

    /// Recomputes what the hero sees, e.g. after loading a game.
    pub fn refresh_sight(&mut self) {
        self.map.compute_fov(self.player, self.light_radius());
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
                    self.spawn(kind, pos, false);
                }
            }
            if self.rng.chance(35) {
                let pos = room.random_cell(&mut self.rng);
                let roll = self.rng.range(0, 99);
                let kind = if roll < 30 {
                    ItemKind::Potion
                } else if roll < 60 {
                    ItemKind::Oil
                } else {
                    ItemKind::Gold(self.rng.range(5, 15) * self.depth)
                };
                if self.map.tile(pos) == Tile::Floor && self.item_at(pos).is_none() {
                    self.items.push(Item { kind, pos });
                }
            }
        }
        self.refresh_sight();
    }

    fn spawn(&mut self, kind: &'static Kind, pos: Pos, awake: bool) {
        let stats = Stats { hp: kind.hp, max_hp: kind.hp, attack: kind.attack, defense: kind.defense };
        self.next_id += 1;
        self.monsters.push(Monster { id: self.next_id, kind, pos, stats, awake, intent: None, stunned: 0 });
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

    /// Whether a visible monster's marked blow covers `p`.
    pub fn threatened(&self, p: Pos) -> bool {
        self.monsters.iter().any(|m| m.intent.as_ref().is_some_and(|cells| cells.contains(&p)))
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
            Action::Use(ability, dx, dy) => self.use_ability(ability, dx.signum(), dy.signum()),
        };
        if took_time && self.state == State::Playing {
            self.end_turn();
        }
    }

    fn player_step(&mut self, dx: i32, dy: i32) -> bool {
        let to = Pos::new(self.player.x + dx, self.player.y + dy);
        if let Some(i) = self.monster_at(to) {
            let dmg = self.damage(self.stats.attack, self.monsters[i].stats.defense);
            self.strike(i, dmg, "hit");
            return true;
        }
        if !self.map.tile(to).walkable() {
            return false;
        }
        self.move_hero(to);
        true
    }

    fn move_hero(&mut self, to: Pos) {
        self.player = to;
        self.pick_up();
        if self.map.tile(to) == Tile::StairsDown {
            self.say("Stairs lead down here (press >).", Tone::Normal);
        }
    }

    /// Whether `ability` may be used now; says why not otherwise.
    pub fn can_use(&self, ability: Ability) -> Result<(), String> {
        let cd = self.cooldown(ability);
        if cd > 0 {
            return Err(format!(
                "{} is ready in {cd} turn{}.",
                capitalized(ability.name()),
                if cd > 1 { "s" } else { "" }
            ));
        }
        if ability == Ability::Throw && self.torch < THROW_COST {
            return Err("Your torch is too weak to throw fire.".into());
        }
        Ok(())
    }

    /// Plays an ability; false when it could not be used, which costs no time.
    fn use_ability(&mut self, ability: Ability, dx: i32, dy: i32) -> bool {
        if let Err(why) = self.can_use(ability) {
            self.say(why, Tone::Normal);
            return false;
        }
        if (dx, dy) == (0, 0) {
            return false;
        }
        let step = |p: Pos, n: i32| Pos::new(p.x + dx * n, p.y + dy * n);
        let used = match ability {
            Ability::Dash => {
                let (mid, to) = (step(self.player, 1), step(self.player, 2));
                if self.free(mid) && self.free(to) {
                    self.move_hero(to);
                    true
                } else {
                    self.say("No room to dash that way.", Tone::Normal);
                    false
                }
            }
            Ability::Bash => match self.monster_at(step(self.player, 1)) {
                Some(i) => {
                    self.bash(i, dx, dy);
                    true
                }
                None => {
                    self.say("There is nothing to bash there.", Tone::Normal);
                    false
                }
            },
            Ability::Throw => {
                self.torch -= THROW_COST;
                let mut path = vec![];
                let mut target = None;
                for n in 1..=THROW_RANGE {
                    let p = step(self.player, n);
                    if !self.map.tile(p).walkable() {
                        break;
                    }
                    path.push(p);
                    if let Some(i) = self.monster_at(p) {
                        target = Some(i);
                        break;
                    }
                }
                self.events.push(Event::Fire { path });
                match target {
                    Some(i) => {
                        let dmg = self.damage(6 + self.depth + self.stats.attack / 2, self.monsters[i].stats.defense);
                        self.stun(i, 1);
                        self.strike(i, dmg, "burn");
                    }
                    None => self.say("Your fire hits nothing.", Tone::Normal),
                }
                true
            }
        };
        if used {
            // One more: the end of this very turn counts it down already.
            self.cooldowns[ability.index()] = ability.cooldown() + 1;
            if ability == Ability::Throw && self.torch < TORCH_MAX / 5 {
                self.say("Your torch gutters after the throw.", Tone::Bad);
            }
        }
        used
    }

    fn bash(&mut self, i: usize, dx: i32, dy: i32) {
        let pos = self.monsters[i].pos;
        let behind = Pos::new(pos.x + dx, pos.y + dy);
        let name = self.monsters[i].kind.name;
        if let Some(j) = self.monster_at(behind) {
            let other = self.monsters[j].kind.name;
            self.say(format!("You slam the {name} into the {other}!"), Tone::Good);
            let (a, b) = (self.monsters[i].id, self.monsters[j].id);
            for id in [a, b] {
                if let Some(k) = self.monsters.iter().position(|m| m.id == id) {
                    let dmg = self.damage(self.stats.attack, self.monsters[k].stats.defense);
                    self.stun(k, 1);
                    self.strike(k, dmg, "slam");
                }
            }
        } else if self.free(behind) {
            self.monsters[i].pos = behind;
            self.stun(i, 1);
            self.say(format!("You shove the {name} back."), Tone::Good);
        } else {
            self.say(format!("You smash the {name} against the wall!"), Tone::Good);
            let dmg = self.damage(self.stats.attack + 2, self.monsters[i].stats.defense);
            self.stun(i, 2);
            self.strike(i, dmg, "smash");
        }
    }

    /// Dazes a monster, which loses the blow it was preparing.
    fn stun(&mut self, i: usize, turns: u32) {
        let m = &mut self.monsters[i];
        m.stunned = m.stunned.max(turns);
        m.intent = None;
        m.awake = true;
        let pos = m.pos;
        self.events.push(Event::Stunned { pos });
    }

    /// The hero deals `dmg` to monster `i`, which may die of it.
    fn strike(&mut self, i: usize, dmg: i32, verb: &str) {
        let m = &mut self.monsters[i];
        m.stats.hp -= dmg;
        m.awake = true;
        let (name, kind, pos) = (m.kind.name, m.kind, m.pos);
        self.events.push(Event::Hit { target: pos, amount: dmg, to_hero: false });
        if m.stats.hp > 0 {
            self.say(format!("You {verb} the {name} for {dmg}."), Tone::Normal);
            return;
        }
        let xp = m.kind.xp;
        self.monsters.remove(i);
        self.events.push(Event::Killed { pos, kind });
        self.kills += 1;
        self.say(format!("You kill the {name}."), Tone::Good);
        self.gain_xp(xp);
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
            ItemKind::Oil => {
                if self.torch == 0 {
                    self.say("You rekindle your torch with oil. The dark recoils.", Tone::Good);
                } else {
                    self.say("You feed your torch with oil.", Tone::Good);
                }
                self.torch = (self.torch + OIL).min(TORCH_MAX);
                self.dark_turns = 0;
            }
            ItemKind::Amulet => {
                self.say("You seize the Amulet! You have won.", Tone::Good);
                self.state = State::Won;
            }
        }
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
        for cd in &mut self.cooldowns {
            *cd = cd.saturating_sub(1);
        }
        self.burn_torch();
        self.refresh_sight();
        // Every awake monster walks down this map: unlike a straight line, it goes
        // around corners and pillars.
        let to_hero = self.map.distances(self.player, |_| true);
        let ids: Vec<u32> = self.monsters.iter().map(|m| m.id).collect();
        for id in ids {
            if let Some(i) = self.monsters.iter().position(|m| m.id == id) {
                self.monster_turn(i, &to_hero);
            }
            if self.state != State::Playing {
                return;
            }
        }
        if self.torch == 0 {
            self.dark_turns += 1;
            if self.dark_turns.is_multiple_of(SHADE_EVERY) {
                self.raise_shade(&to_hero);
            }
        }
        if self.turn.is_multiple_of(8) && self.stats.hp < self.stats.max_hp {
            self.stats.hp += 1;
        }
    }

    fn burn_torch(&mut self) {
        if self.torch == 0 {
            return;
        }
        self.torch -= 1;
        if self.torch == TORCH_MAX / 2 {
            self.say("Your torch burns lower.", Tone::Normal);
        } else if self.torch == TORCH_MAX / 5 {
            self.say("Your torch is guttering. Find oil!", Tone::Bad);
        } else if self.torch == 0 {
            self.say("Your torch goes out. Something stirs in the dark...", Tone::Bad);
        }
    }

    /// A shade comes for the hero from just beyond what they can see.
    fn raise_shade(&mut self, to_hero: &[i32]) {
        let spots: Vec<Pos> = (0..crate::map::HEIGHT)
            .flat_map(|y| (0..crate::map::WIDTH).map(move |x| Pos::new(x, y)))
            .filter(|&p| (3..=6).contains(&to_hero[Map::index(p)]) && self.free(p))
            .collect();
        if spots.is_empty() {
            return;
        }
        let pos = spots[self.rng.range(0, spots.len() as i32 - 1) as usize];
        let shade = BESTIARY.iter().find(|k| k.name == "shade").expect("shades in the bestiary");
        self.spawn(shade, pos, true);
        self.events.push(Event::Shade { pos });
        self.say("A shade peels away from the dark.", Tone::Bad);
    }

    /// The straight line of up to `len` walkable cells from `from` toward `dir`.
    fn line(&self, from: Pos, (dx, dy): (i32, i32), len: i32) -> Vec<Pos> {
        (1..=len)
            .map(|n| Pos::new(from.x + dx * n, from.y + dy * n))
            .take_while(|&p| self.map.tile(p).walkable())
            .collect()
    }

    /// The direction from `from` to `to` if they lie on a straight or diagonal line with
    /// nothing but floor between them, at most `reach` cells apart.
    fn aligned(&self, from: Pos, to: Pos, reach: i32) -> Option<(i32, i32)> {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let n = from.distance(to);
        if n == 0 || n > reach || !(dx == 0 || dy == 0 || dx.abs() == dy.abs()) {
            return None;
        }
        let dir = (dx.signum(), dy.signum());
        (self.line(from, dir, n).len() as i32 == n).then_some(dir)
    }

    /// The cells monster `i` would hit if it struck at the hero from where it stands, or
    /// `None` if the hero is out of its reach.
    pub fn reach(&self, i: usize) -> Option<Vec<Pos>> {
        let m = &self.monsters[i];
        let (from, hero) = (m.pos, self.player);
        let walkable = |p: &Pos| self.map.tile(*p).walkable();
        let cells: Vec<Pos> = match m.kind.pattern {
            Pattern::Bite if from.distance(hero) == 1 => vec![hero],
            Pattern::Cleave if from.distance(hero) == 1 => {
                Map::neighbors(from).filter(|p| p.distance(hero) <= 1).filter(walkable).collect()
            }
            Pattern::Smash if from.distance(hero) == 1 => Map::neighbors(from).filter(walkable).collect(),
            Pattern::Lunge => self.line(from, self.aligned(from, hero, 2)?, 2),
            Pattern::Bolt => self.line(from, self.aligned(from, hero, 4)?, 4),
            Pattern::Blast if from.distance(hero) <= 3 && self.map.is_visible(from) => {
                std::iter::once(hero).chain(Map::neighbors(hero)).filter(walkable).collect()
            }
            _ => return None,
        };
        Some(cells)
    }

    fn monster_turn(&mut self, i: usize, to_hero: &[i32]) {
        let m = &mut self.monsters[i];
        if !m.awake {
            if !self.map.is_visible(m.pos) {
                return;
            }
            m.awake = true;
        }
        if m.stunned > 0 {
            m.stunned -= 1;
            return;
        }
        // First the blow marked last turn lands, on whoever still stands in it.
        if let Some(cells) = m.intent.take() {
            let (attack, kind) = (m.stats.attack, m.kind);
            if cells.contains(&self.player) {
                let dmg = self.damage(attack * kind.pattern.power() / 100, self.stats.defense);
                self.stats.hp -= dmg;
                self.events.push(Event::Hit { target: self.player, amount: dmg, to_hero: true });
                self.say(format!("The {}'s {} hits you for {dmg}.", kind.name, kind.pattern.name()), Tone::Bad);
                if self.stats.hp <= 0 {
                    self.stats.hp = 0;
                    self.killed_by = Some(kind.name.to_string());
                    self.state = State::Dead;
                    self.say("You die...", Tone::Bad);
                    return;
                }
            } else {
                self.events.push(Event::Dodged { pos: self.player });
                self.say(format!("You dodge the {}'s {}.", kind.name, kind.pattern.name()), Tone::Normal);
            }
        }
        // Then it marks its next blow, stepping closer first if the hero is out of reach.
        if self.reach(i).is_none() {
            self.step_toward_hero(i, to_hero);
        }
        if let Some(cells) = self.reach(i) {
            let m = &mut self.monsters[i];
            if matches!(m.kind.pattern, Pattern::Smash | Pattern::Blast) && self.map.is_visible(m.pos) {
                let (name, blow) = (m.kind.name, m.kind.pattern.name());
                m.intent = Some(cells);
                self.say(format!("The {name} readies a {blow}!"), Tone::Bad);
            } else {
                m.intent = Some(cells);
            }
        }
    }

    fn step_toward_hero(&mut self, i: usize, to_hero: &[i32]) {
        let from = self.monsters[i].pos;
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

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str) -> &'static Kind {
        BESTIARY.iter().find(|k| k.name == name).unwrap()
    }

    /// A game with no monster, the hero in the middle of the first room.
    fn empty(seed: u64) -> Game {
        let mut g = Game::new(seed);
        g.monsters.clear();
        g.items.clear();
        g
    }

    /// Puts a monster next to the hero, toward `(dx, dy)`, awake and with `hp` and `attack`.
    fn neighbor(g: &mut Game, name: &str, (dx, dy): (i32, i32), hp: i32, attack: i32) -> u32 {
        let pos = Pos::new(g.player.x + dx, g.player.y + dy);
        assert!(g.map.tile(pos).walkable());
        g.spawn(kind(name), pos, true);
        let m = g.monsters.last_mut().unwrap();
        m.stats = Stats { hp, max_hp: hp, attack, defense: 0 };
        m.id
    }

    fn by_id(g: &Game, id: u32) -> Option<&Monster> {
        g.monsters.iter().find(|m| m.id == id)
    }

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
        let mut g = empty(3);
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
        let mut g = empty(5);
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
    fn monsters_mark_their_blow_then_strike() {
        let mut g = empty(8);
        neighbor(&mut g, "demon", (1, 0), 999, 999);
        g.act(Action::Wait);
        assert_eq!(g.state, State::Playing, "the first turn only marks the blow");
        assert!(g.threatened(g.player));
        g.act(Action::Wait);
        assert_eq!(g.state, State::Dead);
        assert_eq!(g.killed_by.as_deref(), Some("demon"));
    }

    #[test]
    fn stepping_out_of_a_marked_blow_dodges_it() {
        let mut g = empty(14);
        neighbor(&mut g, "rat", (1, 0), 50, 50);
        g.act(Action::Wait);
        assert!(g.threatened(g.player));
        let hp = g.stats.hp;
        // Step away to the left: the bite lands on the empty cell.
        let away = Pos::new(g.player.x - 1, g.player.y);
        assert!(g.map.tile(away).walkable());
        g.act(Action::Move(-1, 0));
        assert_eq!(g.stats.hp, hp);
        assert!(g.events.contains(&Event::Dodged { pos: away }));
    }

    #[test]
    fn a_troll_smash_covers_every_cell_around_it() {
        let mut g = empty(15);
        neighbor(&mut g, "troll", (1, 0), 50, 1);
        g.act(Action::Wait);
        let cells = g.monsters[0].intent.clone().unwrap();
        let troll = g.monsters[0].pos;
        assert!(cells.iter().all(|c| c.distance(troll) == 1));
        assert_eq!(cells.len(), Map::neighbors(troll).filter(|&p| g.map.tile(p).walkable()).count());
    }

    #[test]
    fn killing_a_monster_first_cancels_its_blow() {
        let mut g = empty(16);
        g.stats.attack = 99;
        neighbor(&mut g, "orc", (1, 0), 5, 99);
        g.act(Action::Wait);
        g.act(Action::Move(1, 0));
        assert!(g.monsters.is_empty());
        assert_eq!(g.state, State::Playing);
    }

    #[test]
    fn bashing_into_a_wall_hurts_and_stuns() {
        let mut g = empty(17);
        let room = g.map.rooms[0];
        // Hero two cells from the left wall, monster between them.
        g.player = Pos::new(room.x + 1, room.y + room.h / 2);
        let id = neighbor(&mut g, "orc", (-1, 0), 50, 50);
        g.act(Action::Wait);
        assert!(g.threatened(g.player));
        g.act(Action::Use(Ability::Bash, -1, 0));
        let orc = by_id(&g, id).unwrap();
        assert!(orc.stats.hp < 50);
        assert!(orc.intent.is_none(), "a stunned monster loses its blow");
        assert_eq!(g.stats.hp, g.stats.max_hp);
        assert_eq!(g.cooldown(Ability::Bash), Ability::Bash.cooldown());
    }

    #[test]
    fn bashing_into_open_floor_pushes_back() {
        let mut g = empty(18);
        let room = g.map.rooms[0];
        g.player = Pos::new(room.x, room.y + room.h / 2);
        let id = neighbor(&mut g, "rat", (1, 0), 50, 0);
        g.act(Action::Use(Ability::Bash, 1, 0));
        assert_eq!(by_id(&g, id).unwrap().pos, Pos::new(g.player.x + 2, g.player.y));
    }

    #[test]
    fn dash_moves_two_cells_and_then_cools_down() {
        let mut g = empty(19);
        let room = g.map.rooms[0];
        let start = Pos::new(room.x, room.y);
        g.player = start;
        g.act(Action::Use(Ability::Dash, 1, 0));
        assert_eq!(g.player, Pos::new(start.x + 2, start.y));
        let turn = g.turn;
        g.act(Action::Use(Ability::Dash, 1, 0));
        assert_eq!(g.turn, turn, "an ability cooling down costs no time");
        // Into a wall: refused, no time spent.
        g.cooldowns = [0; 3];
        g.player = start;
        g.act(Action::Use(Ability::Dash, -1, 0));
        assert_eq!(g.player, start);
        assert_eq!(g.cooldown(Ability::Dash), 0);
    }

    #[test]
    fn thrown_fire_burns_the_first_monster_and_the_torch() {
        let mut g = empty(20);
        let room = g.map.rooms[0];
        g.player = Pos::new(room.x, room.y);
        let id = neighbor(&mut g, "orc", (2, 0), 100, 0);
        g.act(Action::Use(Ability::Throw, 1, 0));
        assert!(by_id(&g, id).unwrap().stats.hp < 100);
        assert_eq!(g.torch, TORCH_MAX - THROW_COST - 1);
        g.cooldowns = [0; 3];
        g.torch = THROW_COST - 1;
        assert!(g.can_use(Ability::Throw).is_err());
    }

    #[test]
    fn the_torch_burns_down_and_oil_feeds_it() {
        let mut g = empty(21);
        assert_eq!(g.light_radius(), FOV_RADIUS);
        for _ in 0..TORCH_MAX - 10 {
            g.act(Action::Wait);
        }
        assert_eq!(g.torch, 10);
        assert!(g.light_radius() < FOV_RADIUS);
        let next = Pos::new(g.player.x + 1, g.player.y);
        g.items.push(Item { kind: ItemKind::Oil, pos: next });
        g.act(Action::Move(1, 0));
        assert_eq!(g.torch, 10 + OIL - 1);
    }

    #[test]
    fn the_dark_sends_shades() {
        let mut g = empty(22);
        g.torch = 0;
        g.stats = Stats { hp: 9999, max_hp: 9999, attack: 1, defense: 99 };
        for _ in 0..SHADE_EVERY {
            g.act(Action::Wait);
        }
        assert_eq!(g.light_radius(), 1);
        assert!(g.monsters.iter().any(|m| m.kind.name == "shade"));
    }

    #[test]
    fn monsters_walk_around_walls() {
        let mut g = empty(11);
        // Put a monster in the next room: the straight line to the hero crosses walls,
        // yet it must reach them by the corridor.
        let pos = g.map.rooms[1].center();
        g.spawn(kind("rat"), pos, true);
        g.monsters[0].stats.attack = 0;
        g.stats.defense = 99;
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
        let g = empty(12);
        let room = g.map.rooms[0];
        let target = Pos::new(room.x, room.y);
        let path = g.path_to(target).unwrap();
        assert_eq!(path.last(), Some(&target));
        assert!(path.windows(2).all(|w| w[0].distance(w[1]) == 1));
        // The far end of the dungeon is unexplored: no known way there.
        assert_eq!(g.path_to(g.map.rooms.last().unwrap().center()), None);
    }

    #[test]
    fn random_play_never_breaks() {
        for seed in 0..20 {
            let mut g = Game::new(seed);
            let mut rng = Rng::new(seed + 1000);
            for _ in 0..1000 {
                if g.state != State::Playing {
                    break;
                }
                let (dx, dy) = (rng.range(-1, 1), rng.range(-1, 1));
                let action = match rng.range(0, 19) {
                    0 => Action::Use(Ability::Dash, dx, dy),
                    1 => Action::Use(Ability::Bash, dx, dy),
                    2 => Action::Use(Ability::Throw, dx, dy),
                    3 => Action::Drink,
                    4 => Action::Descend,
                    5 => Action::Wait,
                    _ => Action::Move(dx, dy),
                };
                g.act(action);
                assert!(g.torch >= 0 && g.stats.hp >= 0);
                assert!(g.map.tile(g.player).walkable());
                assert!(g.monsters.iter().all(|m| m.pos != g.player && g.map.tile(m.pos).walkable()));
            }
        }
    }

    #[test]
    fn events_describe_the_last_action() {
        let mut g = empty(13);
        g.stats.hp = 10;
        g.act(Action::Drink);
        assert_eq!(g.events, [Event::Healed { amount: 15 }]);
        g.act(Action::Wait);
        assert!(g.events.is_empty());
    }
}
