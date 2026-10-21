//! Plays hundreds of runs with a simple but sensible bot and prints how they went, to tune
//! the numbers of the game: how deep runs go, what kills, whether oil runs short, how much
//! gold reaches the purse and how long the armory takes to buy.
//!
//!     cargo run --release -p delve-core --example balance -- [--runs 300] [--seed 1]
//!
//! The bot only uses what a player sees: explored cells, visible monsters and their marked
//! blows. It dodges, strikes first when a blow would kill, uses its abilities, drinks when
//! in danger, gathers items, explores and goes down. It plays with three tempers, which only
//! differ at ways out: cautious leaves at the first one with gold, balanced leaves when
//! weakened, greedy never leaves and goes for the Amulet.

use std::collections::BTreeMap;

use delve_core::game::{
    Ability, Action, BESTIARY, Event, Game, ItemKind, LAST_DEPTH, OIL, Pattern, State, THROW_COST, TORCH_MAX,
};
use delve_core::gear::{self, ARMORY};
use delve_core::map::{self, Map, Pos, Tile};

/// A run that has not ended after this many turns counts as stuck.
const MAX_TURNS: u32 = 8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Temper {
    Cautious,
    Balanced,
    Greedy,
}

impl Temper {
    fn name(self) -> &'static str {
        match self {
            Temper::Cautious => "cautious",
            Temper::Balanced => "balanced",
            Temper::Greedy => "greedy",
        }
    }

    /// Whether to take the way out of this floor rather than go deeper.
    fn leaves(self, g: &Game) -> bool {
        let hp = g.stats.hp * 100 / g.stats.max_hp;
        match self {
            Temper::Cautious => g.gold > 0,
            Temper::Balanced => match g.depth {
                d if d <= 3 => hp < 50 || g.torch < 100 || g.potions == 0,
                d if d <= 6 => !(hp >= 80 && g.potions >= 1 && g.torch >= 150),
                _ => true,
            },
            Temper::Greedy => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    Died(String),
    Escaped,
    Won,
    Stuck,
}

/// What one run did.
struct Run {
    outcome: Outcome,
    depth: i32,
    gold: i32,
    banked: i32,
    turns: u32,
    torch_out: bool,
    oil: u32,
    shades: u32,
    potions_drunk: u32,
    abilities: u32,
    /// For each floor left, the turns it took to see the stairs, and to leave it.
    floors: Vec<(u32, u32)>,
}

fn free(g: &Game, p: Pos) -> bool {
    g.map.tile(p).walkable() && p != g.player && g.monster_at(p).is_none()
}

fn dir(from: Pos, to: Pos) -> (i32, i32) {
    ((to.x - from.x).signum(), (to.y - from.y).signum())
}

fn cells() -> impl Iterator<Item = Pos> {
    (0..map::HEIGHT).flat_map(|y| (0..map::WIDTH).map(move |x| Pos::new(x, y)))
}

/// Whether the hero's blow kills monster `i` whatever the dice (they add -1 to +2).
fn sure_kill(g: &Game, i: usize) -> bool {
    let m = &g.monsters[i];
    m.stats.hp <= (g.stats.attack - 1 - m.stats.defense).max(1)
}

/// A step toward `target` through explored cells, avoiding marked blows; `None` if blocked.
fn step_toward(g: &Game, target: Pos) -> Option<Action> {
    let dist = g.map.distances(target, |p| g.map.is_explored(p));
    let d = |p: Pos| dist[Map::index(p)];
    let here = d(g.player);
    Map::neighbors(g.player)
        .filter(|&p| d(p) >= 0 && (here < 0 || d(p) < here) && free(g, p) && !g.threatened(p))
        .min_by_key(|&p| d(p))
        .map(|p| {
            let (dx, dy) = dir(g.player, p);
            Action::Move(dx, dy)
        })
}

/// The nearest of `targets` by walking through explored cells.
fn nearest(g: &Game, targets: impl Iterator<Item = Pos>) -> Option<Pos> {
    let dist = g.map.distances(g.player, |p| g.map.is_explored(p));
    targets.filter(|&p| dist[Map::index(p)] >= 0).min_by_key(|&p| dist[Map::index(p)])
}

fn choose(g: &Game, leaving: bool) -> Action {
    let me = g.player;
    let near: Vec<usize> = (0..g.monsters.len()).filter(|&i| g.monsters[i].pos.distance(me) == 1).collect();
    let aims_at_me = |i: usize| g.monsters[i].intent.as_ref().is_some_and(|c| c.contains(&me));
    let hp = g.stats.hp * 100 / g.stats.max_hp;
    let threatened = g.threatened(me);
    let attack = |i: usize| {
        let (dx, dy) = dir(me, g.monsters[i].pos);
        Action::Move(dx, dy)
    };

    if hp < 35 && g.potions > 0 && (threatened || !near.is_empty()) {
        return Action::Drink;
    }
    // Blows about to land on the hero, roughly: dodging a rat's bite forever wastes the
    // torch, so the hero takes small blows and trades hits.
    let incoming: i32 = g
        .monsters
        .iter()
        .filter(|m| m.intent.as_ref().is_some_and(|c| c.contains(&me)))
        .map(|m| {
            let power = match m.kind.pattern {
                Pattern::Smash => 150,
                Pattern::Blast => 120,
                _ => 100,
            };
            (m.stats.attack * power / 100 + 1 - g.stats.defense).max(1)
        })
        .sum();
    let worth_dodging = incoming * 4 > g.stats.hp;
    if threatened
        && !worth_dodging
        && let Some(&i) = near.iter().filter(|&&i| aims_at_me(i)).min_by_key(|&&i| g.monsters[i].stats.hp)
    {
        return attack(i);
    }
    if threatened {
        // Kill whoever is about to strike, if one blow does it.
        if let Some(&i) = near.iter().find(|&&i| aims_at_me(i) && sure_kill(g, i)) {
            return attack(i);
        }
        // Step out, preferably next to as few monsters as possible.
        let safe = Map::neighbors(me)
            .filter(|&p| free(g, p) && !g.threatened(p))
            .min_by_key(|&p| g.monsters.iter().filter(|m| m.pos.distance(p) == 1).count());
        if let Some(p) = safe {
            let (dx, dy) = dir(me, p);
            return Action::Move(dx, dy);
        }
        if g.can_use(Ability::Dash).is_ok() {
            for p in Map::neighbors(me) {
                let (dx, dy) = dir(me, p);
                let to = Pos::new(me.x + 2 * dx, me.y + 2 * dy);
                if free(g, p) && free(g, to) && !g.threatened(to) {
                    return Action::Use(Ability::Dash, dx, dy);
                }
            }
        }
        if g.can_use(Ability::Bash).is_ok()
            && let Some(&i) = near.iter().find(|&&i| aims_at_me(i))
        {
            let (dx, dy) = dir(me, g.monsters[i].pos);
            return Action::Use(Ability::Bash, dx, dy);
        }
    }
    if let Some(&i) = near.iter().min_by_key(|&&i| g.monsters[i].stats.hp) {
        return attack(i);
    }
    // Fire at a dangerous monster lined up, while the torch can spare it.
    if g.can_use(Ability::Throw).is_ok() && g.torch > THROW_COST + 100 {
        for m in g.monsters.iter().filter(|m| g.map.is_visible(m.pos) && m.awake && m.stats.hp > g.stats.attack * 2) {
            let (dx, dy) = (m.pos.x - me.x, m.pos.y - me.y);
            let n = me.distance(m.pos);
            let line = dx == 0 || dy == 0 || dx.abs() == dy.abs();
            let clear = (1..n).all(|k| free(g, Pos::new(me.x + dx.signum() * k, me.y + dy.signum() * k)));
            if line && n <= 5 && clear {
                return Action::Use(Ability::Throw, dx.signum(), dy.signum());
            }
        }
    }

    let here = g.map.tile(me);
    let known = |t: Tile| nearest(g, cells().filter(|&p| g.map.is_explored(p) && g.map.tile(p) == t));
    if leaving {
        if here == Tile::Exit {
            return Action::Escape;
        }
        if let Some(exit) = known(Tile::Exit)
            && let Some(step) = step_toward(g, exit)
        {
            return step;
        }
    }
    // Items it has seen; oil only once the torch has room for most of a flask.
    let seen = g.items.iter().filter(|i| g.map.is_explored(i.pos));
    let wanted = seen.filter(|i| i.kind != ItemKind::Oil || g.torch <= TORCH_MAX - OIL / 2).map(|i| i.pos);
    if let Some(item) = nearest(g, wanted)
        && let Some(step) = step_toward(g, item)
    {
        return step;
    }
    let frontier = cells().filter(|&p| {
        g.map.is_explored(p) && g.map.tile(p).walkable() && Map::neighbors(p).any(|n| !g.map.is_explored(n)) && p != me
    });
    let frontier = nearest(g, frontier);
    let stairs = known(Tile::StairsDown);
    // Head down once the floor is explored, or when the light runs short.
    let go_down = stairs.is_some() && (frontier.is_none() || g.torch < 60);
    if go_down || frontier.is_none() {
        if here == Tile::StairsDown {
            return Action::Descend;
        }
        if let Some(step) = stairs.and_then(|s| step_toward(g, s)) {
            return step;
        }
    }
    if let Some(step) = frontier.and_then(|f| step_toward(g, f)) {
        return step;
    }
    Action::Wait
}

fn play(seed: u64, temper: Temper, gear: &[&'static gear::Gear]) -> Run {
    play_traced(seed, temper, gear, false)
}

/// Plays one run; `trace` prints every turn, to see what the bot does.
fn play_traced(seed: u64, temper: Temper, gear: &[&'static gear::Gear], trace: bool) -> Run {
    let mut g = Game::equipped(seed, gear);
    let mut run = Run {
        outcome: Outcome::Stuck,
        depth: 1,
        gold: 0,
        banked: 0,
        turns: 0,
        torch_out: false,
        oil: 0,
        shades: 0,
        potions_drunk: 0,
        abilities: 0,
        floors: vec![],
    };
    let (mut entered, mut stairs_seen) = (0, None);
    let mut leaving = false;
    let mut idle = 0;
    while g.state == State::Playing && g.turn < MAX_TURNS {
        if Game::has_exit(g.depth) && !leaving {
            leaving = temper.leaves(&g);
        }
        let (turn, depth) = (g.turn, g.depth);
        // An action refused (an ability cooling down, a wall) costs no turn: fall back
        // to waiting rather than trying it forever.
        let action = if idle >= 2 { Action::Wait } else { choose(&g, leaving) };
        if trace {
            println!(
                "turn {:4} floor {} at {:2},{:2} hp {:3} torch {:3} near {} -> {action:?}",
                g.turn,
                g.depth,
                g.player.x,
                g.player.y,
                g.stats.hp,
                g.torch,
                g.monsters.iter().filter(|m| m.pos.distance(g.player) <= 2).count()
            );
        }
        g.act(action);
        idle = if g.turn == turn && g.depth == depth && g.state == State::Playing { idle + 1 } else { 0 };
        if stairs_seen.is_none() && cells().any(|p| g.map.is_explored(p) && g.map.tile(p) == Tile::StairsDown) {
            stairs_seen = Some(g.turn - entered);
        }
        if g.depth != depth {
            leaving = false;
            run.floors.push((stairs_seen.unwrap_or(g.turn - entered), g.turn - entered));
            (entered, stairs_seen) = (g.turn, None);
        }
        match action {
            Action::Drink if g.turn != turn => run.potions_drunk += 1,
            Action::Use(..) if g.turn != turn => run.abilities += 1,
            _ => {}
        }
        for e in &g.events {
            match e {
                Event::PickedUp { kind: ItemKind::Oil, .. } => run.oil += 1,
                Event::Shade { .. } => run.shades += 1,
                _ => {}
            }
        }
        run.torch_out |= g.torch == 0;
    }
    run.outcome = match g.state {
        State::Dead => Outcome::Died(g.killed_by.clone().unwrap_or_default()),
        State::Escaped => Outcome::Escaped,
        State::Won => Outcome::Won,
        State::Playing => Outcome::Stuck,
    };
    run.depth = g.depth;
    run.gold = g.gold;
    run.banked = if g.gold_is_safe() { g.gold } else { 0 };
    run.turns = g.turn;
    run
}

fn pct(n: usize, of: usize) -> String {
    format!("{:.0}%", 100.0 * n as f64 / of.max(1) as f64)
}

fn report(title: &str, runs: &[Run]) {
    let n = runs.len();
    let count = |f: &dyn Fn(&Run) -> bool| runs.iter().filter(|r| f(r)).count();
    let mean = |f: &dyn Fn(&Run) -> f64| runs.iter().map(f).sum::<f64>() / n.max(1) as f64;
    println!("\n== {title} ({n} runs) ==");
    println!(
        "outcome      died {} · escaped {} · won {} · stuck {}",
        pct(count(&|r| matches!(r.outcome, Outcome::Died(_))), n),
        pct(count(&|r| r.outcome == Outcome::Escaped), n),
        pct(count(&|r| r.outcome == Outcome::Won), n),
        pct(count(&|r| r.outcome == Outcome::Stuck), n),
    );
    let mut depths: Vec<i32> = runs.iter().map(|r| r.depth).collect();
    depths.sort();
    println!(
        "floor        reached on average {:.1} · median {} · best tenth {}+",
        mean(&|r| r.depth as f64),
        depths[n / 2],
        depths[n * 9 / 10]
    );
    let mut deaths: BTreeMap<i32, usize> = BTreeMap::new();
    let mut killers: BTreeMap<String, usize> = BTreeMap::new();
    for r in runs {
        if let Outcome::Died(k) = &r.outcome {
            *deaths.entry(r.depth).or_default() += 1;
            *killers.entry(k.clone()).or_default() += 1;
        }
    }
    let deaths: Vec<String> = deaths.iter().map(|(d, c)| format!("{d}:{}", pct(*c, n))).collect();
    println!("deaths       by floor  {}", deaths.join("  "));
    let mut killers: Vec<(String, usize)> = killers.into_iter().collect();
    killers.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    let killers: Vec<String> = killers.iter().take(6).map(|(k, c)| format!("{k} {}", pct(*c, n))).collect();
    println!("killers      {}", killers.join(" · "));
    println!(
        "gold         found {:.0} per run · banked {:.0} per run",
        mean(&|r| r.gold as f64),
        mean(&|r| r.banked as f64)
    );
    let floors = runs.iter().map(|r| r.depth as f64).sum::<f64>();
    let turns = runs.iter().map(|r| r.turns as f64).sum::<f64>();
    println!(
        "torch        went out in {} of runs · {:.2} oil per floor · {:.1} shades per run",
        pct(count(&|r| r.torch_out), n),
        runs.iter().map(|r| r.oil as f64).sum::<f64>() / floors,
        mean(&|r| r.shades as f64)
    );
    println!(
        "pace         {:.0} turns per floor · {:.1} potions and {:.1} abilities per run",
        turns / floors,
        mean(&|r| r.potions_drunk as f64),
        mean(&|r| r.abilities as f64)
    );
    let floors: Vec<(u32, u32)> = runs.iter().flat_map(|r| r.floors.iter().copied()).collect();
    let nf = floors.len().max(1) as f64;
    println!(
        "floors done  stairs seen after {:.0} turns · floor left after {:.0} turns",
        floors.iter().map(|f| f.0 as f64).sum::<f64>() / nf,
        floors.iter().map(|f| f.1 as f64).sum::<f64>() / nf
    );
    let banked = mean(&|r| r.banked as f64);
    let armory: i32 = ARMORY.iter().map(|g| g.price).sum();
    let cheapest = ARMORY.iter().map(|g| g.price).min().unwrap_or(0);
    if banked > 0.0 {
        println!(
            "armory       first piece after {:.1} runs · all {armory} gold after {:.0} runs",
            cheapest as f64 / banked,
            armory as f64 / banked
        );
    } else {
        println!("armory       no gold ever reaches the purse");
    }
}

fn main() {
    let mut runs = 300;
    let mut first_seed = 1;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().and_then(|v| v.parse::<u64>().ok());
        match (arg.as_str(), value) {
            ("--runs", Some(n)) => runs = n,
            ("--seed", Some(s)) => first_seed = s,
            ("--trace", Some(s)) => {
                let run = play_traced(s, Temper::Balanced, &[], true);
                println!("{:?} on floor {} after {} turns", run.outcome, run.depth, run.turns);
                return;
            }
            _ => {
                eprintln!("usage: balance [--runs N] [--seed S] | --trace SEED");
                std::process::exit(2);
            }
        }
    }
    // Every piece of the armory, the best of each slot: the hero of a long campaign.
    let all: Vec<&'static gear::Gear> = {
        let mut p = gear::Profile::new();
        p.purse = i32::MAX;
        for g in ARMORY {
            p.buy(g).expect("bought in order");
        }
        p.loadout()
    };
    println!("delve balance · {} monsters · last floor {LAST_DEPTH}", BESTIARY.len());
    for temper in [Temper::Cautious, Temper::Balanced, Temper::Greedy] {
        let played: Vec<Run> = (first_seed..first_seed + runs).map(|s| play(s, temper, &[])).collect();
        report(&format!("{}, no gear", temper.name()), &played);
    }
    let played: Vec<Run> = (first_seed..first_seed + runs).map(|s| play(s, Temper::Balanced, &all)).collect();
    report("balanced, the whole armory", &played);
}
