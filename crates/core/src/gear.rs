//! Equipment bought between runs with the gold brought back from the dungeon. It is kept
//! for good: every new hero starts with the best piece owned in each slot.

use serde::{Deserialize, Serialize};

use crate::game::Stats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Weapon,
    Shield,
    Vitality,
    Supplies,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Weapon, Slot::Shield, Slot::Vitality, Slot::Supplies];

    pub fn name(self) -> &'static str {
        match self {
            Slot::Weapon => "Weapon",
            Slot::Shield => "Shield",
            Slot::Vitality => "Vitality",
            Slot::Supplies => "Supplies",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gear {
    pub name: &'static str,
    pub slot: Slot,
    pub price: i32,
    /// Attack for a weapon, defense for a shield, max HP for vitality, potions for supplies.
    pub bonus: i32,
}

impl Gear {
    pub fn describe(&self) -> String {
        match self.slot {
            Slot::Weapon => format!("+{} attack", self.bonus),
            Slot::Shield => format!("+{} defense", self.bonus),
            Slot::Vitality => format!("+{} max HP", self.bonus),
            Slot::Supplies => format!("+{} potion{}", self.bonus, if self.bonus > 1 { "s" } else { "" }),
        }
    }
}

/// Within a slot, from the cheapest to the best: each piece must be bought in turn.
pub const ARMORY: &[Gear] = &[
    Gear { name: "short sword", slot: Slot::Weapon, price: 60, bonus: 2 },
    Gear { name: "long sword", slot: Slot::Weapon, price: 220, bonus: 4 },
    Gear { name: "war axe", slot: Slot::Weapon, price: 600, bonus: 7 },
    Gear { name: "wooden shield", slot: Slot::Shield, price: 50, bonus: 1 },
    Gear { name: "iron shield", slot: Slot::Shield, price: 200, bonus: 2 },
    Gear { name: "dragon shield", slot: Slot::Shield, price: 650, bonus: 4 },
    Gear { name: "troll tonic", slot: Slot::Vitality, price: 80, bonus: 10 },
    Gear { name: "giant's draught", slot: Slot::Vitality, price: 280, bonus: 25 },
    Gear { name: "potion belt", slot: Slot::Supplies, price: 40, bonus: 1 },
    Gear { name: "alchemist's satchel", slot: Slot::Supplies, price: 160, bonus: 2 },
];

pub fn by_name(name: &str) -> Option<&'static Gear> {
    ARMORY.iter().find(|g| g.name == name)
}

pub fn in_slot(slot: Slot) -> impl Iterator<Item = &'static Gear> {
    ARMORY.iter().filter(move |g| g.slot == slot)
}

/// What lasts from one run to the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Gold brought back from the dungeon and not spent yet.
    pub purse: i32,
    /// Names of the pieces bought.
    pub owned: Vec<String>,
    /// Size of the interface, 1 being the default.
    #[serde(default = "one")]
    pub ui_scale: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyError {
    AlreadyOwned,
    /// The previous piece of the slot must be bought first.
    Locked,
    TooExpensive,
}

impl std::fmt::Display for BuyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BuyError::AlreadyOwned => "you already own it",
            BuyError::Locked => "buy the previous one first",
            BuyError::TooExpensive => "not enough gold",
        })
    }
}

impl Default for Profile {
    fn default() -> Self {
        Profile { purse: 0, owned: vec![], ui_scale: 1.0 }
    }
}

impl Profile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn owns(&self, gear: &Gear) -> bool {
        self.owned.iter().any(|n| n == gear.name)
    }

    /// Whether `gear` can be bought now, and why not.
    pub fn can_buy(&self, gear: &Gear) -> Result<(), BuyError> {
        if self.owns(gear) {
            return Err(BuyError::AlreadyOwned);
        }
        let earlier = in_slot(gear.slot).take_while(|g| g.name != gear.name);
        if earlier.into_iter().any(|g| !self.owns(g)) {
            return Err(BuyError::Locked);
        }
        if self.purse < gear.price {
            return Err(BuyError::TooExpensive);
        }
        Ok(())
    }

    pub fn buy(&mut self, gear: &Gear) -> Result<(), BuyError> {
        self.can_buy(gear)?;
        self.purse -= gear.price;
        self.owned.push(gear.name.to_string());
        Ok(())
    }

    /// The best piece owned in each slot: what a new hero takes along.
    pub fn loadout(&self) -> Vec<&'static Gear> {
        Slot::ALL.iter().filter_map(|&slot| in_slot(slot).filter(|g| self.owns(g)).last()).collect()
    }
}

/// Stats and potions of a new hero carrying `gear`.
pub fn equip(mut stats: Stats, mut potions: i32, gear: &[&Gear]) -> (Stats, i32) {
    for g in gear {
        match g.slot {
            Slot::Weapon => stats.attack += g.bonus,
            Slot::Shield => stats.defense += g.bonus,
            Slot::Vitality => {
                stats.max_hp += g.bonus;
                stats.hp += g.bonus;
            }
            Slot::Supplies => potions += g.bonus,
        }
    }
    (stats, potions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_has_gear_and_names_are_unique() {
        for slot in Slot::ALL {
            assert!(in_slot(slot).count() > 0);
        }
        for g in ARMORY {
            assert_eq!(by_name(g.name), Some(g));
        }
    }

    #[test]
    fn gear_is_bought_in_order_with_enough_gold() {
        let mut p = Profile::new();
        let short = by_name("short sword").unwrap();
        let long = by_name("long sword").unwrap();
        assert_eq!(p.buy(short), Err(BuyError::TooExpensive));
        p.purse = 1000;
        assert_eq!(p.buy(long), Err(BuyError::Locked));
        assert_eq!(p.buy(short), Ok(()));
        assert_eq!(p.buy(short), Err(BuyError::AlreadyOwned));
        assert_eq!(p.buy(long), Ok(()));
        assert_eq!(p.purse, 1000 - short.price - long.price);
        assert_eq!(p.loadout(), [long]);
    }

    #[test]
    fn equipment_strengthens_a_new_hero() {
        let base = Stats { hp: 30, max_hp: 30, attack: 5, defense: 1 };
        let gear = [by_name("long sword").unwrap(), by_name("troll tonic").unwrap(), by_name("potion belt").unwrap()];
        let (stats, potions) = equip(base, 1, &gear);
        assert_eq!(stats, Stats { hp: 40, max_hp: 40, attack: 9, defense: 1 });
        assert_eq!(potions, 2);
    }
}
