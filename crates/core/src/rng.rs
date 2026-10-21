//! A small seeded generator (SplitMix64): the same seed always gives the same dungeon.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        debug_assert!(lo <= hi);
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i32
    }

    /// True with probability `percent`/100.
    pub fn chance(&mut self, percent: u32) -> bool {
        (self.next_u64() % 100) < percent as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_stays_in_bounds() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let v = r.range(-3, 5);
            assert!((-3..=5).contains(&v));
        }
    }
}
