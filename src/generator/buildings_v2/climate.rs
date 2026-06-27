//! Build-area climate → settlement culture selection.
//!
//! A settlement adopts one [`Culture`], chosen from the build area's biome map so
//! the town fits its surroundings. The mapping:
//!
//! - **Arid** (desert, savanna, badlands) → Desert.
//! - **Tropical** (jungle, swamp, mangrove, lush) → Japanese.
//! - **Cherry grove** and **bamboo jungle** → Japanese, but weighted *much*
//!   heavier ([`JAPANESE_SIGNAL_WEIGHT`]) — they're the iconic Japanese
//!   landmarks, so even a modest grove inside the build area pulls the whole town
//!   Japanese.
//! - Everything else — temperate forest/plains and cold/boreal — → Medieval (the
//!   default), plus oceans/rivers/unknown.
//!
//! Selection sums a per-cell culture weight over the whole biome map and makes one
//! weighted random draw, so a build area straddling biomes leans toward its
//! dominant climate but can still surface a minority culture — and a heavy cherry/
//! bamboo cell outweighs many plain ones.

use crate::minecraft::Biome;
use crate::noise::RNG;

use super::Culture;

/// Per-cell Japanese weight for a cherry grove or bamboo jungle cell — the iconic
/// Japanese landmark biomes. Set well above 1 so a comparatively small grove in
/// the build area dominates the summed weights and flips the town Japanese, while
/// staying a weight (not a hard override) so a lone stray cell among a sea of
/// plains doesn't necessarily decide it. Raise it to make ever-smaller groves
/// flip the town; lower it toward 1 to make cherry/bamboo just ordinary Japanese
/// biomes.
const JAPANESE_SIGNAL_WEIGHT: f32 = 10.0;

/// Coarse climate class of one biome cell — the axis culture cares about
/// (temperature + humidity), distinct from the wood/stone biome buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Climate {
    /// Hot + dry: desert, savanna, badlands → Desert.
    Arid,
    /// Warm + wet: jungle, swamp, mangrove, lush → Japanese (ordinary weight).
    Tropical,
    /// Bamboo jungle → Japanese, weighted heavily ([`JAPANESE_SIGNAL_WEIGHT`]).
    Bamboo,
    /// Cherry grove → Japanese, weighted heavily ([`JAPANESE_SIGNAL_WEIGHT`]).
    Cherry,
    /// Cold: taiga, snowy, frozen, peaks → Medieval.
    Boreal,
    /// The temperate middle (plains, forest, birch, dark-forest, meadow) → the
    /// Medieval default. Also the catch-all for oceans/rivers/unknown biomes.
    Temperate,
}

impl Climate {
    /// Classify a biome. Arid / Tropical / Bamboo / Cherry / Boreal are matched
    /// explicitly; everything else (incl. unknown, ocean, river) is Temperate.
    pub fn from_biome(biome: &Biome) -> Climate {
        use Climate::*;
        match biome.name() {
            // Hot + dry.
            "desert" | "desert_hills" | "desert_lakes"
            | "savanna" | "savanna_plateau" | "shattered_savanna"
            | "shattered_savanna_plateau" | "windswept_savanna"
            | "badlands" | "badlands_plateau" | "wooded_badlands"
            | "wooded_badlands_plateau" | "modified_badlands_plateau"
            | "modified_wooded_badlands_plateau" | "eroded_badlands" => Arid,

            // Bamboo — heavy Japanese signal.
            "bamboo_jungle" | "bamboo_jungle_hills" => Bamboo,

            // Cherry — heavy Japanese signal.
            "cherry_grove" => Cherry,

            // Other warm + wet → ordinary Japanese.
            "jungle" | "jungle_hills" | "jungle_edge" | "modified_jungle"
            | "modified_jungle_edge" | "sparse_jungle" | "swamp" | "swamp_hills"
            | "mangrove_swamp" | "lush_caves" => Tropical,

            // Cold.
            "taiga" | "taiga_hills" | "taiga_mountains" | "snowy_taiga"
            | "snowy_taiga_hills" | "snowy_taiga_mountains" | "giant_tree_taiga"
            | "giant_tree_taiga_hills" | "giant_spruce_taiga"
            | "giant_spruce_taiga_hills" | "old_growth_pine_taiga"
            | "old_growth_spruce_taiga" | "snowy_tundra" | "snowy_plains"
            | "snowy_mountains" | "snowy_forest" | "snowy_slopes" | "snowy_beach"
            | "grove" | "frozen_peaks" | "jagged_peaks" | "stony_peaks"
            | "ice_spikes" | "frozen_river" | "frozen_ocean" => Boreal,

            // Plains, forests, meadows, oceans, rivers, unknown → temperate.
            _ => Temperate,
        }
    }

    /// Per-cell culture weights `(medieval, japanese, desert)`. Arid → Desert,
    /// tropical/bamboo/cherry → Japanese (cherry & bamboo heavy), everything else
    /// → Medieval.
    fn culture_weights(self) -> (f32, f32, f32) {
        match self {
            Climate::Arid => (0.0, 0.0, 1.0),
            Climate::Tropical => (0.0, 1.0, 0.0),
            Climate::Bamboo | Climate::Cherry => (0.0, JAPANESE_SIGNAL_WEIGHT, 0.0),
            Climate::Boreal | Climate::Temperate => (1.0, 0.0, 0.0),
        }
    }
}

/// Pick a settlement culture from the build area's biome map. Sums per-cell
/// culture weights over every cell and makes one weighted random draw on `rng`,
/// so the choice fits the dominant climate yet a straddling area can surface a
/// minority culture — and a cherry/bamboo grove, weighted heavily, pulls the town
/// Japanese even when it's a minority of the build area. Deterministic in `rng`;
/// an empty/all-unknown map (e.g. the synthetic offline world) falls back to
/// Medieval.
pub fn select_culture(biome_map: &[Vec<Biome>], rng: &mut RNG) -> Culture {
    let (mut med, mut jap, mut des) = (0.0f32, 0.0f32, 0.0f32);
    for column in biome_map {
        for biome in column {
            let (m, j, d) = Climate::from_biome(biome).culture_weights();
            med += m;
            jap += j;
            des += d;
        }
    }

    if med + jap + des <= 0.0 {
        return Culture::Medieval;
    }

    let weights = vec![
        (Culture::Medieval, med),
        (Culture::Japanese, jap),
        (Culture::Desert, des),
    ];
    *rng.choose_weighted_vec(&weights)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn biome(name: &str) -> Biome {
        Biome::from(name)
    }

    /// A uniform biome map of one biome name.
    fn uniform(name: &str, n: usize) -> Vec<Vec<Biome>> {
        vec![vec![biome(name); n]; n]
    }

    /// A square map that is `cherry_frac` of `signal` biome (top rows) and the
    /// rest `base` — models a build area straddling a grove.
    fn mixed(base: &str, signal: &str, signal_frac: f32, n: usize) -> Vec<Vec<Biome>> {
        let signal_cols = (n as f32 * signal_frac).round() as usize;
        let mut map = vec![vec![biome(base); n]; n];
        for (i, col) in map.iter_mut().enumerate() {
            if i < signal_cols {
                for cell in col.iter_mut() {
                    *cell = biome(signal);
                }
            }
        }
        map
    }

    #[test]
    fn climate_buckets_cover_the_key_biomes() {
        assert_eq!(Climate::from_biome(&biome("desert")), Climate::Arid);
        assert_eq!(Climate::from_biome(&biome("savanna")), Climate::Arid);
        assert_eq!(Climate::from_biome(&biome("jungle")), Climate::Tropical);
        assert_eq!(Climate::from_biome(&biome("mangrove_swamp")), Climate::Tropical);
        assert_eq!(Climate::from_biome(&biome("bamboo_jungle")), Climate::Bamboo);
        assert_eq!(Climate::from_biome(&biome("cherry_grove")), Climate::Cherry);
        assert_eq!(Climate::from_biome(&biome("snowy_taiga")), Climate::Boreal);
        assert_eq!(Climate::from_biome(&biome("plains")), Climate::Temperate);
        assert_eq!(Climate::from_biome(&biome("forest")), Climate::Temperate);
        // Unknown / unmatched falls through to temperate.
        assert_eq!(Climate::from_biome(&Biome::unknown()), Climate::Temperate);
    }

    /// Pure single-climate build areas resolve deterministically: arid → Desert,
    /// tropical/cherry/bamboo → Japanese, temperate/boreal → Medieval.
    #[test]
    fn pure_climates_resolve_deterministically() {
        for seed in 0..50i64 {
            let mut rng = RNG::new(seed);
            assert_eq!(select_culture(&uniform("desert", 8), &mut rng), Culture::Desert);
            assert_eq!(select_culture(&uniform("jungle", 8), &mut rng), Culture::Japanese);
            assert_eq!(select_culture(&uniform("cherry_grove", 8), &mut rng), Culture::Japanese);
            assert_eq!(select_culture(&uniform("bamboo_jungle", 8), &mut rng), Culture::Japanese);
            assert_eq!(select_culture(&uniform("plains", 8), &mut rng), Culture::Medieval);
            assert_eq!(select_culture(&uniform("forest", 8), &mut rng), Culture::Medieval);
            assert_eq!(select_culture(&uniform("snowy_taiga", 8), &mut rng), Culture::Medieval);
        }
    }

    /// A modest cherry or bamboo grove inside an otherwise temperate build area
    /// flips the town Japanese a clear majority of the time, because the grove is
    /// weighted `JAPANESE_SIGNAL_WEIGHT`× a plain cell. With ~20% grove the
    /// Japanese weight (0.20·10 = 2.0) dwarfs the Medieval weight (0.80).
    #[test]
    fn cherry_or_bamboo_grove_flips_temperate_town() {
        for signal in ["cherry_grove", "bamboo_jungle"] {
            let map = mixed("plains", signal, 0.20, 10);
            let mut jap = 0;
            const N: i64 = 2000;
            for seed in 0..N {
                let mut rng = RNG::new(seed);
                if select_culture(&map, &mut rng) == Culture::Japanese {
                    jap += 1;
                }
            }
            let frac = jap as f32 / N as f32;
            assert!(frac > 0.65, "{signal}: only {frac:.2} of towns went Japanese");
        }
    }

    /// A tiny stray patch of cherry (a few cells at the edge) does *not* reliably
    /// flip an overwhelmingly temperate build area — it's a heavy weight, not a
    /// hard override, so the town usually stays Medieval.
    #[test]
    fn a_cherry_sliver_usually_stays_medieval() {
        let map = mixed("plains", "cherry_grove", 0.03, 10); // ~3% cherry
        let mut med = 0;
        const N: i64 = 2000;
        for seed in 0..N {
            let mut rng = RNG::new(seed);
            if select_culture(&map, &mut rng) == Culture::Medieval {
                med += 1;
            }
        }
        let frac = med as f32 / N as f32;
        assert!(frac > 0.6, "a 3% cherry sliver flipped {:.2} of towns Japanese", 1.0 - frac);
    }
}
