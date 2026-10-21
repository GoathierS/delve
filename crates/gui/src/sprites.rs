//! Which tile of Kenney's Tiny Dungeon sheet (CC0, 12 × 11 tiles of 16 px) draws what.

use delve_core::game::{ItemKind, Kind};
use delve_core::gear::{Gear, Slot};
use delve_core::map::{Pos, Tile};
use macroquad::prelude::Rect;

pub const SHEET: &[u8] = include_bytes!("../assets/tiny-dungeon.png");
pub const TILE: f32 = 16.0;
const COLUMNS: u32 = 12;

pub const HERO: u32 = 97;
const WALL: u32 = 40;
const FLOOR: u32 = 48;
const FLOOR_SPECKLED: u32 = 49;
const FLOOR_TILED: u32 = 42;
const STAIRS: u32 = 39;
const EXIT: u32 = 45;
pub const POTION: u32 = 115;
pub const GOLD: u32 = 89;
const AMULET: u32 = 44;
pub const DAGGER: u32 = 103;
const OIL: u32 = 113;

/// Where tile `index` lies in the sheet.
pub fn source(index: u32) -> Rect {
    Rect::new((index % COLUMNS) as f32 * TILE, (index / COLUMNS) as f32 * TILE, TILE, TILE)
}

pub fn monster(kind: &Kind) -> u32 {
    match kind.name {
        "rat" => 123,
        "kobold" => 124,
        "goblin" => 108,
        "orc" => 112,
        "giant spider" => 122,
        "troll" => 109,
        "wraith" => 121,
        "demon" => 110,
        "shade" => 111,
        "bat" => 120,
        _ => 111,
    }
}

pub fn gear(gear: &Gear) -> u32 {
    match gear.name {
        "short sword" => 104,
        "long sword" => 106,
        "war axe" => 118,
        "wooden shield" => 101,
        "iron shield" => 102,
        "dragon shield" => 29,
        "troll tonic" => 114,
        "giant's draught" => 116,
        "potion belt" => 115,
        "alchemist's satchel" => 127,
        _ => slot_icon(gear.slot),
    }
}

/// What stands for an empty slot.
pub fn slot_icon(slot: Slot) -> u32 {
    match slot {
        Slot::Weapon => DAGGER,
        Slot::Shield => 101,
        Slot::Vitality => 114,
        Slot::Supplies => POTION,
    }
}

pub fn item(kind: ItemKind) -> u32 {
    match kind {
        ItemKind::Potion => POTION,
        ItemKind::Gold(_) => GOLD,
        ItemKind::Amulet => AMULET,
        ItemKind::Oil => OIL,
    }
}

/// Floors get a few variants, picked by position so that they stay put.
pub fn tile(tile: Tile, p: Pos) -> u32 {
    match tile {
        Tile::Wall => WALL,
        Tile::StairsDown => STAIRS,
        Tile::Exit => EXIT,
        Tile::Floor => {
            let h = (p.x as u32).wrapping_mul(73_856_093) ^ (p.y as u32).wrapping_mul(19_349_663);
            match h % 23 {
                0 | 1 => FLOOR_SPECKLED,
                2 => FLOOR_TILED,
                _ => FLOOR,
            }
        }
    }
}
