//! Deterministic hashing for generator collections.
//!
//! `std::collections::HashMap`/`HashSet` default to `RandomState`, whose keys
//! are re-seeded every process. Iteration order therefore changes from run to
//! run, and any code that iterates a map/set before drawing from the RNG,
//! taking the first element, or breaking a tie makes a different town each
//! run. These aliases swap in a fixed, unkeyed FxHash-style hasher so the
//! iteration order is stable across runs on a given toolchain. (hashbrown's
//! internal layout may still change between Rust releases, so this does not
//! promise identical towns across compiler upgrades.)
//!
//! The aliases keep the names `HashMap`/`HashSet`, so signatures read the
//! same. They have no `new()`/`with_capacity()`/`from([..])` (those are
//! `RandomState`-only): use `default()`, `with_capacity_and_hasher(n,
//! Default::default())`, and `.into_iter().collect()`.
//!
//! Not suitable for maps keyed on untrusted input (no HashDoS resistance).

use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x517c_c1b7_2722_0a95;

/// FxHash-style multiply-rotate hasher with no per-process key.
#[derive(Default, Clone, Copy, Debug)]
pub struct StableHasher {
    state: u64,
}

impl StableHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.state = (self.state.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for StableHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(buf));
        }
    }

    #[inline] fn write_u8(&mut self, i: u8) { self.add(i as u64); }
    #[inline] fn write_u16(&mut self, i: u16) { self.add(i as u64); }
    #[inline] fn write_u32(&mut self, i: u32) { self.add(i as u64); }
    #[inline] fn write_u64(&mut self, i: u64) { self.add(i); }
    #[inline] fn write_u128(&mut self, i: u128) { self.add(i as u64); self.add((i >> 64) as u64); }
    #[inline] fn write_usize(&mut self, i: usize) { self.add(i as u64); }
    #[inline] fn write_i8(&mut self, i: i8) { self.add(i as u8 as u64); }
    #[inline] fn write_i16(&mut self, i: i16) { self.add(i as u16 as u64); }
    #[inline] fn write_i32(&mut self, i: i32) { self.add(i as u32 as u64); }
    #[inline] fn write_i64(&mut self, i: i64) { self.add(i as u64); }
    #[inline] fn write_i128(&mut self, i: i128) { self.write_u128(i as u128); }
    #[inline] fn write_isize(&mut self, i: isize) { self.add(i as u64); }

    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }
}

pub type StableBuildHasher = BuildHasherDefault<StableHasher>;

/// `std::collections::HashMap` with a deterministic hasher (stable iteration order).
pub type HashMap<K, V> = std::collections::HashMap<K, V, StableBuildHasher>;
/// `std::collections::HashSet` with a deterministic hasher (stable iteration order).
pub type HashSet<T> = std::collections::HashSet<T, StableBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, Hash};

    #[test]
    fn hash_is_fixed() {
        let h = |v: &(i32, &str)| {
            let mut s = StableBuildHasher::default().build_hasher();
            v.hash(&mut s);
            s.finish()
        };
        assert_eq!(h(&(3, "abc")), h(&(3, "abc")));
        assert_ne!(h(&(3, "abc")), h(&(4, "abc")));
    }

    #[test]
    fn iteration_order_matches_between_instances() {
        let a: HashSet<(i32, i32)> = (0..500).map(|i| (i * 7 % 31, i)).collect();
        let b: HashSet<(i32, i32)> = (0..500).rev().map(|i| (i * 7 % 31, i)).collect();
        // Same contents, same capacity history isn't guaranteed, so compare via a fresh rebuild.
        let a2: HashSet<(i32, i32)> = a.iter().copied().collect();
        let b2: HashSet<(i32, i32)> = a.iter().copied().collect();
        assert_eq!(a2.iter().collect::<Vec<_>>(), b2.iter().collect::<Vec<_>>());
        assert_eq!(a, b);
    }
}
