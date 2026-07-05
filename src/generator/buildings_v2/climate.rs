//! Build-area climate → settlement culture selection.
//!
//! A settlement adopts one [`Culture`], chosen from the build area's biome map so
//! the town fits its surroundings. The mapping:
//!
//! - **Arid** (desert, savanna, badlands) → Desert.
//! - **Tropical** (jungle, swamp, mangrove, lush) → Japanese.
//! - **Cherry grove** → Japanese, *forced*: any cherry cell in the build area
//!   makes the town Japanese outright. It's the iconic Japanese landmark, and a
//!   build area was almost certainly chosen around it on purpose — losing the
//!   draw to surrounding plains reads as a bug, not variety.
//! - **Bamboo jungle** → Japanese, weighted heavier ([`JAPANESE_SIGNAL_WEIGHT`])
//!   than an ordinary cell but still a weight, not an override.
//! - **Water** (oceans, rivers) → no vote. Water says nothing about what culture
//!   fits the land, so a river-wrapped or coastal build area is judged by its
//!   actual terrain instead of being diluted toward the default.
//! - Everything else — temperate forest/plains and cold/boreal — → Medieval (the
//!   default), plus unknown.
//!
//! Selection sums a per-cell culture weight over the whole biome map and makes one
//! weighted random draw, so a build area straddling biomes leans toward its
//! dominant climate but can still surface a minority culture — and a heavy cherry/
//! bamboo cell outweighs many plain ones.

use crate::minecraft::Biome;
use crate::noise::RNG;

use super::Culture;

/// Per-cell Japanese weight for a bamboo jungle cell. Set well above 1 so a
/// comparatively small stand in the build area dominates the summed weights and
/// flips the town Japanese, while staying a weight (not a hard override) so a
/// lone stray cell among a sea of plains doesn't necessarily decide it. Cherry
/// groves go further: any cherry cell forces Japanese outright (see
/// [`select_culture`]).
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
    /// Cherry grove → Japanese, forced: any cherry cell decides the culture.
    Cherry,
    /// Cold: taiga, snowy, frozen, peaks → Medieval.
    Boreal,
    /// Open water: oceans and rivers. No culture vote — water doesn't say what
    /// culture fits the land, so it shouldn't dilute coastal/river build areas
    /// toward the Medieval default.
    Water,
    /// The temperate middle (plains, forest, birch, dark-forest, meadow) → the
    /// Medieval default. Also the catch-all for unknown biomes.
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
            | "ice_spikes" => Boreal,

            // Open water — no culture vote.
            "ocean" | "deep_ocean" | "cold_ocean" | "deep_cold_ocean"
            | "frozen_ocean" | "deep_frozen_ocean" | "lukewarm_ocean"
            | "deep_lukewarm_ocean" | "warm_ocean" | "river" | "frozen_river" => Water,

            // Plains, forests, meadows, beaches, unknown → temperate.
            _ => Temperate,
        }
    }

    /// Per-cell culture weights `(medieval, japanese, desert)`. Arid → Desert,
    /// tropical/bamboo → Japanese (bamboo heavy), water → no vote, everything
    /// else → Medieval. Cherry never reaches the draw — [`select_culture`]
    /// short-circuits on it — but weighs in like bamboo for completeness.
    fn culture_weights(self) -> (f32, f32, f32) {
        match self {
            Climate::Arid => (0.0, 0.0, 1.0),
            Climate::Tropical => (0.0, 1.0, 0.0),
            Climate::Bamboo | Climate::Cherry => (0.0, JAPANESE_SIGNAL_WEIGHT, 0.0),
            Climate::Water => (0.0, 0.0, 0.0),
            Climate::Boreal | Climate::Temperate => (1.0, 0.0, 0.0),
        }
    }
}

/// Pick a settlement culture from the build area's biome map. A cherry grove
/// anywhere in the area forces Japanese outright — it's the iconic landmark and
/// the build area was almost certainly chosen around it. Otherwise sums per-cell
/// culture weights over every cell and makes one weighted random draw on `rng`,
/// so the choice fits the dominant climate yet a straddling area can surface a
/// minority culture — and a bamboo stand, weighted heavily, pulls the town
/// Japanese even when it's a minority of the build area. Water cells carry no
/// weight, so coastal/river areas are judged by their land. Deterministic in
/// `rng`; an empty or all-water map (or the synthetic offline world's all-unknown
/// one) falls back to Medieval.
pub fn select_culture(biome_map: &[Vec<Biome>], rng: &mut RNG) -> Culture {
    let (mut med, mut jap, mut des) = (0.0f32, 0.0f32, 0.0f32);
    for column in biome_map {
        for biome in column {
            let climate = Climate::from_biome(biome);
            if climate == Climate::Cherry {
                return Culture::Japanese;
            }
            let (m, j, d) = climate.culture_weights();
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
        assert_eq!(Climate::from_biome(&biome("ocean")), Climate::Water);
        assert_eq!(Climate::from_biome(&biome("river")), Climate::Water);
        assert_eq!(Climate::from_biome(&biome("frozen_river")), Climate::Water);
        // Beaches are land: they vote.
        assert_eq!(Climate::from_biome(&biome("beach")), Climate::Temperate);
        // Unknown / unmatched falls through to temperate.
        assert_eq!(Climate::from_biome(&Biome::unknown()), Climate::Temperate);
    }

    /// Water carries no culture weight: a jungle island in a sea of ocean cells
    /// resolves Japanese every time (the ocean must not dilute it toward
    /// Medieval), and an all-water map falls back to Medieval.
    #[test]
    fn water_does_not_dilute_the_land_vote() {
        // 20% jungle, 80% ocean — pre-fix the ocean out-voted the jungle 4:1.
        let map = mixed("ocean", "jungle", 0.20, 10);
        for seed in 0..50i64 {
            let mut rng = RNG::new(seed);
            assert_eq!(select_culture(&map, &mut rng), Culture::Japanese);
            assert_eq!(select_culture(&uniform("ocean", 8), &mut rng), Culture::Medieval);
        }
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

    /// A modest bamboo stand inside an otherwise temperate build area flips the
    /// town Japanese a clear majority of the time, because it is weighted
    /// `JAPANESE_SIGNAL_WEIGHT`× a plain cell. With ~20% bamboo the Japanese
    /// weight (0.20·10 = 2.0) dwarfs the Medieval weight (0.80).
    #[test]
    fn bamboo_stand_flips_temperate_town() {
        let map = mixed("plains", "bamboo_jungle", 0.20, 10);
        let mut jap = 0;
        const N: i64 = 2000;
        for seed in 0..N {
            let mut rng = RNG::new(seed);
            if select_culture(&map, &mut rng) == Culture::Japanese {
                jap += 1;
            }
        }
        let frac = jap as f32 / N as f32;
        assert!(frac > 0.65, "bamboo_jungle: only {frac:.2} of towns went Japanese");
    }

    /// Live analysis (needs a running server): for each named candidate area, set
    /// the build area, load the world's biome map, and report the climate/culture
    /// breakdown the generator would see — biome tallies, summed culture weights,
    /// draw probabilities, and the actual pick across seeds 0..20 using the same
    /// RNG keying as `generate_settlement` (`from_seed_and_string("culture_select")`).
    /// Pure analysis: nothing is placed; the original build area is restored.
    /// Run with `cargo test analyze_candidate_area_cultures -- --nocapture`.
    #[tokio::test]
    async fn analyze_candidate_area_cultures() {
        use crate::editor::World;
        use crate::http_mod::GDMCHTTPProvider;
        use crate::noise::Seed;

        let areas: [(&str, (i32, i32), (i32, i32)); 4] = [
            ("Sunflower Plains & Cherry Hill", (3000, 10800), (4000, 11800)),
            ("Swamp Lake", (0, 400), (800, 1200)),
            ("The Caldera", (3800, 3200), (4200, 3500)),
            ("Savanna Valley", (4000, 2800), (4800, 3600)),
        ];

        let provider = GDMCHTTPProvider::new();
        let original = provider.get_build_area().await.ok();

        for (name, (x1, z1), (x2, z2)) in areas {
            provider
                .command(vec![format!("buildarea set {x1} -60 {z1} {x2} 200 {z2}")])
                .await
                .expect("failed to set build area");
            let world = World::new(&provider)
                .await
                .expect("failed to load world for area");

            let biome_map = world.get_ground_biome_map();
            let mut biome_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            let (mut med, mut jap, mut des) = (0.0f64, 0.0f64, 0.0f64);
            let mut cells = 0usize;
            for column in biome_map {
                for biome in column {
                    *biome_counts.entry(biome.name()).or_default() += 1;
                    let (m, j, d) = Climate::from_biome(biome).culture_weights();
                    med += m as f64;
                    jap += j as f64;
                    des += d as f64;
                    cells += 1;
                }
            }
            let total = med + jap + des;

            println!("\n=== {name} ({x1},{z1}) → ({x2},{z2}), {cells} cells ===");
            let mut sorted: Vec<_> = biome_counts.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            for (biome, count) in &sorted {
                let climate = Climate::from_biome(&Biome::from(*biome));
                println!(
                    "  {:>6.2}% {:<40} {:?}",
                    100.0 * *count as f64 / cells as f64,
                    biome,
                    climate
                );
            }
            println!(
                "  weights: Medieval {:.0} ({:.1}%) | Japanese {:.0} ({:.1}%) | Desert {:.0} ({:.1}%)",
                med, 100.0 * med / total,
                jap, 100.0 * jap / total,
                des, 100.0 * des / total,
            );

            let picks: Vec<String> = (0..20i64)
                .map(|seed| {
                    let mut rng = RNG::from_seed_and_string(Seed(seed), "culture_select");
                    format!("{:?}", select_culture(biome_map, &mut rng))
                })
                .collect();
            println!("  picks for seeds 0..20: {}", picks.join(", "));
        }

        if let Some(area) = original {
            let (o, m) = (area.origin, area.max());
            provider
                .command(vec![format!(
                    "buildarea set {} {} {} {} {} {}",
                    o.x, o.y, o.z, m.x, m.y, m.z
                )])
                .await
                .expect("failed to restore original build area");
        }
    }

    /// Cherry is a hard override: any cherry cell — even a single one in a sea of
    /// plains — forces the town Japanese on every seed. A build area containing a
    /// cherry grove was chosen for it; losing the draw would read as a bug.
    #[test]
    fn any_cherry_cell_forces_japanese() {
        let mut sliver = uniform("plains", 10);
        for z in 0..3 {
            sliver[9][z] = biome("cherry_grove"); // 3 cells in a corner
        }
        let mut lone = uniform("plains", 10);
        lone[7][4] = biome("cherry_grove"); // one cell
        for seed in 0..50i64 {
            let mut rng = RNG::new(seed);
            assert_eq!(select_culture(&sliver, &mut rng), Culture::Japanese);
            assert_eq!(select_culture(&lone, &mut rng), Culture::Japanese);
        }
    }
}
