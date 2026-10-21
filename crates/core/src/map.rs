//! The dungeon: tiles, generation and field of view.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;

pub const WIDTH: i32 = 80;
pub const HEIGHT: i32 = 40;

const NEIGHBORS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    Wall,
    Floor,
    StairsDown,
}

impl Tile {
    pub fn walkable(self) -> bool {
        self != Tile::Wall
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Steps between two cells when moving diagonally is allowed.
    pub fn distance(self, other: Pos) -> i32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Room {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Room {
    pub fn center(&self) -> Pos {
        Pos::new(self.x + self.w / 2, self.y + self.h / 2)
    }

    /// Overlaps `other`, keeping one wall between them.
    fn touches(&self, other: &Room) -> bool {
        self.x <= other.x + other.w
            && self.x + self.w >= other.x
            && self.y <= other.y + other.h
            && self.y + self.h >= other.y
    }

    pub fn random_cell(&self, rng: &mut Rng) -> Pos {
        Pos::new(rng.range(self.x, self.x + self.w - 1), rng.range(self.y, self.y + self.h - 1))
    }
}

#[derive(Serialize, Deserialize)]
pub struct Map {
    tiles: Vec<Tile>,
    /// Seen at least once: drawn dimmed when out of sight.
    explored: Vec<bool>,
    visible: Vec<bool>,
    pub rooms: Vec<Room>,
}

impl Map {
    /// Rooms joined by L-shaped corridors, each to the previous one, so every room is
    /// reachable. The stairs go in the last room, far from the start in the first.
    pub fn generate(rng: &mut Rng) -> Self {
        let cells = (WIDTH * HEIGHT) as usize;
        let mut map = Map {
            tiles: vec![Tile::Wall; cells],
            explored: vec![false; cells],
            visible: vec![false; cells],
            rooms: vec![],
        };
        for _ in 0..60 {
            let (w, h) = (rng.range(5, 12), rng.range(4, 8));
            let room = Room { x: rng.range(1, WIDTH - w - 2), y: rng.range(1, HEIGHT - h - 2), w, h };
            if map.rooms.iter().any(|r| r.touches(&room)) {
                continue;
            }
            map.carve_room(&room);
            if let Some(prev) = map.rooms.last() {
                let (a, b) = (prev.center(), room.center());
                if rng.chance(50) {
                    map.carve_h(a.x, b.x, a.y);
                    map.carve_v(a.y, b.y, b.x);
                } else {
                    map.carve_v(a.y, b.y, a.x);
                    map.carve_h(a.x, b.x, b.y);
                }
            }
            map.rooms.push(room);
        }
        let stairs = map.rooms.last().expect("at least one room fits").center();
        map.set(stairs, Tile::StairsDown);
        map
    }

    fn idx(p: Pos) -> usize {
        (p.y * WIDTH + p.x) as usize
    }

    /// Index of `p` in per-cell vectors such as [`Map::distances`]. `p` must be in bounds.
    pub fn index(p: Pos) -> usize {
        Self::idx(p)
    }

    /// The up to 8 in-bounds cells around `p`.
    pub fn neighbors(p: Pos) -> impl Iterator<Item = Pos> {
        NEIGHBORS.iter().map(move |(dx, dy)| Pos::new(p.x + dx, p.y + dy)).filter(|&n| Self::in_bounds(n))
    }

    /// Steps from `from` to every walkable cell that `allowed` lets through, diagonals
    /// included; -1 where unreachable. Index it with [`Map::index`].
    pub fn distances(&self, from: Pos, allowed: impl Fn(Pos) -> bool) -> Vec<i32> {
        let mut dist = vec![-1; (WIDTH * HEIGHT) as usize];
        if !Self::in_bounds(from) {
            return dist;
        }
        dist[Self::idx(from)] = 0;
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(p) = queue.pop_front() {
            let d = dist[Self::idx(p)];
            for n in Self::neighbors(p) {
                if dist[Self::idx(n)] < 0 && self.tile(n).walkable() && allowed(n) {
                    dist[Self::idx(n)] = d + 1;
                    queue.push_back(n);
                }
            }
        }
        dist
    }

    pub fn in_bounds(p: Pos) -> bool {
        (0..WIDTH).contains(&p.x) && (0..HEIGHT).contains(&p.y)
    }

    pub fn tile(&self, p: Pos) -> Tile {
        if Self::in_bounds(p) { self.tiles[Self::idx(p)] } else { Tile::Wall }
    }

    pub fn set(&mut self, p: Pos, t: Tile) {
        self.tiles[Self::idx(p)] = t;
    }

    pub fn is_visible(&self, p: Pos) -> bool {
        Self::in_bounds(p) && self.visible[Self::idx(p)]
    }

    pub fn is_explored(&self, p: Pos) -> bool {
        Self::in_bounds(p) && self.explored[Self::idx(p)]
    }

    fn carve_room(&mut self, r: &Room) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.set(Pos::new(x, y), Tile::Floor);
            }
        }
    }

    fn carve_h(&mut self, x1: i32, x2: i32, y: i32) {
        for x in x1.min(x2)..=x1.max(x2) {
            self.set(Pos::new(x, y), Tile::Floor);
        }
    }

    fn carve_v(&mut self, y1: i32, y2: i32, x: i32) {
        for y in y1.min(y2)..=y1.max(y2) {
            self.set(Pos::new(x, y), Tile::Floor);
        }
    }

    /// Recomputes what can be seen from `from`: every cell within `radius` whose straight
    /// line to `from` crosses no wall. Walls themselves are seen, which outlines rooms.
    pub fn compute_fov(&mut self, from: Pos, radius: i32) {
        self.visible.fill(false);
        for y in from.y - radius..=from.y + radius {
            for x in from.x - radius..=from.x + radius {
                let p = Pos::new(x, y);
                let (dx, dy) = (x - from.x, y - from.y);
                if !Self::in_bounds(p) || dx * dx + dy * dy > radius * radius + radius {
                    continue;
                }
                if self.line_clear(from, p) {
                    let i = Self::idx(p);
                    self.visible[i] = true;
                    self.explored[i] = true;
                }
            }
        }
        // Lines to a corridor's side walls clip corners and miss them: also show the walls
        // bordering a visible floor, so that corridors are outlined.
        let mut walls = vec![];
        for y in from.y - radius..=from.y + radius {
            for x in from.x - radius..=from.x + radius {
                let p = Pos::new(x, y);
                if self.tile(p) == Tile::Wall
                    && Self::in_bounds(p)
                    && !self.visible[Self::idx(p)]
                    && NEIGHBORS.iter().any(|(dx, dy)| {
                        let n = Pos::new(x + dx, y + dy);
                        self.tile(n).walkable() && self.is_visible(n)
                    })
                {
                    walls.push(Self::idx(p));
                }
            }
        }
        for i in walls {
            self.visible[i] = true;
            self.explored[i] = true;
        }
    }

    /// No wall strictly between `a` and `b` (Bresenham).
    fn line_clear(&self, a: Pos, b: Pos) -> bool {
        let (dx, dy) = ((b.x - a.x).abs(), -(b.y - a.y).abs());
        let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
        let (mut x, mut y, mut err) = (a.x, a.y, dx + dy);
        loop {
            if (x, y) == (b.x, b.y) {
                return true;
            }
            if (x, y) != (a.x, a.y) && !self.tile(Pos::new(x, y)).walkable() {
                return false;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flood fill from the first room reaches every floor cell, stairs included.
    #[test]
    fn every_floor_is_reachable() {
        for seed in 0..50 {
            let map = Map::generate(&mut Rng::new(seed));
            let start = map.rooms[0].center();
            let mut seen = vec![false; (WIDTH * HEIGHT) as usize];
            let mut stack = vec![start];
            while let Some(p) = stack.pop() {
                if !map.tile(p).walkable() || seen[Map::idx(p)] {
                    continue;
                }
                seen[Map::idx(p)] = true;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    stack.push(Pos::new(p.x + dx, p.y + dy));
                }
            }
            for y in 0..HEIGHT {
                for x in 0..WIDTH {
                    let p = Pos::new(x, y);
                    assert!(!map.tile(p).walkable() || seen[Map::idx(p)], "seed {seed}: {p:?} unreachable");
                }
            }
            assert!(map.rooms.len() >= 4, "seed {seed}: only {} rooms", map.rooms.len());
        }
    }

    /// A floor cell is visible exactly when it is in range and nothing blocks the line.
    #[test]
    fn walls_block_sight() {
        for seed in 0..20 {
            let mut map = Map::generate(&mut Rng::new(seed));
            let from = map.rooms[0].center();
            map.compute_fov(from, 8);
            for y in from.y - 8..=from.y + 8 {
                for x in from.x - 8..=from.x + 8 {
                    let p = Pos::new(x, y);
                    let (dx, dy) = (x - from.x, y - from.y);
                    if !map.tile(p).walkable() || dx * dx + dy * dy > 8 * 8 + 8 {
                        continue;
                    }
                    assert_eq!(map.is_visible(p), map.line_clear(from, p), "seed {seed}: {p:?}");
                }
            }
            assert!(map.is_visible(from));
        }
    }
}
