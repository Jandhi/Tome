//! Per-culture building-style catalogs.
//!
//! A [`BuildingStyle`] is a *recipe* over existing palette files: a base palette
//! plus per-axis variant pools (woods / stones / roofs). Rolling a style merges
//! the base with one random pick from each non-empty pool, so **internal
//! variance is just pool size** — a one-entry pool is a matched set, a four-entry
//! pool jitters house-to-house.
//!
//! A settlement picks a few styles from its culture's catalog and applies them in
//! a weighted 60/30/10 mix (dominant / secondary / accent) so a town reads as one
//! coherent material story with a clear secondary texture and rare punctuation.
//! See `settlement.rs` for the town-composition layer; this module only defines
//! the catalog and how one style rolls into a concrete [`Palette`].

use crate::generator::data::LoadedData;
use crate::generator::materials::{Palette, PaletteId};
use crate::minecraft::{Biome, BiomeWoodtype};
use crate::noise::RNG;

use super::Culture;

/// Target share of buildings in a *variable*-wood style (pool >1 entry) that
/// should use the biome's local timber, so a town visibly leans on what grows
/// around it while keeping the curated variety. Locked single-entry pools ignore
/// the bias entirely.
const LOCAL_WOOD_FRACTION: f32 = 0.6;

/// A named palette recipe: a base palette plus variant pools the build draws one
/// pick each from. Empty pools are skipped (so e.g. desert's flat roofs keep the
/// base roof material). The base palette carries the fixed identity — walls,
/// interior floor, flowers, accent colours — that the pools never touch.
#[derive(Debug, Clone)]
pub struct BuildingStyle {
    pub name: &'static str,
    pub base: PaletteId,
    pub woods: Vec<PaletteId>,
    pub stones: Vec<PaletteId>,
    pub roofs: Vec<PaletteId>,
    /// Decorative accent palettes (set the `accent` material role). Picked like
    /// the other uniform pools and merged onto the palette. The flat-roof
    /// parapet renders the accent as a coloured band (see `flat_roof.rs`); other
    /// roofs ignore it for now. Empty = no accent (parapet stays the base stone).
    pub accents: Vec<PaletteId>,
    /// A rare/landmark style. A settlement's [`StyleScheme`] keeps `rare` styles
    /// out of the dominant/secondary slots so they never blanket a town — they
    /// only ever appear as the ~10% accent.
    pub rare: bool,
}

impl BuildingStyle {
    /// Merge the base palette with one pick from each non-empty pool. Pool size
    /// *is* the internal variance: a single-entry pool always yields the same
    /// material (a matched set), a multi-entry pool rolls per call.
    ///
    /// When `local_wood` is set and the wood pool is *variable* (>1 entry), the
    /// biome's local wood is weighted into the wood roll heavily enough to land
    /// at ~[`LOCAL_WOOD_FRACTION`] of buildings, so a town leans on the timber
    /// that grows around it. A locked single-entry wood pool ignores the bias,
    /// keeping matched/accent styles intact. Pass `None` for no bias.
    pub fn roll_palette(&self, rng: &mut RNG, data: &LoadedData, local_wood: Option<&PaletteId>) -> Palette {
        let fetch = |id: &PaletteId| {
            data.palettes
                .get(id)
                .unwrap_or_else(|| panic!("style '{}' palette {:?} not found", self.name, id))
                .clone()
        };

        let mut palette = fetch(&self.base);

        // Wood: biome-biased pick when the pool is variable.
        if !self.woods.is_empty() {
            // Only honour a local wood that actually has a palette file (some
            // biomes — e.g. mangrove — have a wood type but no palette yet).
            let local = local_wood.filter(|id| data.palettes.contains_key(*id));
            let wood = pick_wood(rng, &self.woods, local);
            palette = palette.merged_with(&fetch(&wood));
        }

        // Stone + roof + accent: uniform picks from the curated pool.
        for pool in [&self.stones, &self.roofs, &self.accents] {
            if pool.is_empty() {
                continue;
            }
            let id = &pool[rng.rand_i32_range(0, pool.len() as i32) as usize];
            palette = palette.merged_with(&fetch(id));
        }

        palette
    }

    /// Mark this style rare (landmark) — see [`BuildingStyle::rare`].
    fn rare(mut self) -> Self {
        self.rare = true;
        self
    }
}

/// Pick a wood-palette id from `pool`, biased toward `local`. A single-entry
/// pool is locked (returns it, ignoring the bias). For a variable pool, curated
/// entries each carry weight 1 and the local wood gets enough extra weight to
/// win ~[`LOCAL_WOOD_FRACTION`] of rolls (it may also be a curated entry, which
/// just stacks the odds slightly). `local` not in `pool` is injected.
fn pick_wood(rng: &mut RNG, pool: &[PaletteId], local: Option<&PaletteId>) -> PaletteId {
    let n = pool.len() as i32;
    if n <= 1 {
        return pool[0].clone();
    }
    let local = match local {
        Some(id) => id,
        None => return pool[rng.rand_i32_range(0, n) as usize].clone(),
    };
    // frac = w / (n + w)  ⇒  w = n * frac / (1 - frac)
    let w = ((n as f32) * LOCAL_WOOD_FRACTION / (1.0 - LOCAL_WOOD_FRACTION))
        .round()
        .max(1.0) as i32;
    let r = rng.rand_i32_range(0, n + w);
    if r < n {
        pool[r as usize].clone()
    } else {
        local.clone()
    }
}

/// The wood-palette id matching a biome's local timber, if any (`None` for
/// nether/end/cave biomes with no wood, or an unknown biome). Pass the result to
/// [`BuildingStyle::roll_palette`] to bias a building toward local wood.
pub fn local_wood_palette(biome: Biome) -> Option<PaletteId> {
    BiomeWoodtype::from_biome(biome).map(|w| w.get_wood_palette_id())
}

/// Terse constructor: string slices in, [`PaletteId`]s out.
fn style(name: &'static str, base: &str, woods: &[&str], stones: &[&str], roofs: &[&str]) -> BuildingStyle {
    let ids = |xs: &[&str]| xs.iter().map(|s| PaletteId::from(*s)).collect();
    BuildingStyle {
        name,
        base: base.into(),
        woods: ids(woods),
        stones: ids(stones),
        roofs: ids(roofs),
        accents: Vec::new(),
        rare: false,
    }
}

/// A desert decoration style: smooth-sandstone body (no wood/roof jitter) with a
/// single terracotta accent that the flat-roof parapet renders as a coloured
/// crown band.
fn desert_accent(name: &'static str, stones: &[&str], accent: &str) -> BuildingStyle {
    BuildingStyle { accents: vec![accent.into()], ..style(name, "desert_sandstone", &[], stones, &[]) }
}

impl Culture {
    /// Catalog of building styles a settlement of this culture samples from. A
    /// town picks a dominant / secondary / accent trio out of this list and
    /// applies them 60/30/10, so the order here is just the menu — not a fixed
    /// town composition. Styles vary in *internal* variance via their pool sizes
    /// (see [`BuildingStyle`]): the everyday styles jitter wood/stone/roof for an
    /// organic look, the prestige/accent styles stay tight so they read as
    /// deliberate landmarks.
    pub fn style_catalog(&self) -> Vec<BuildingStyle> {
        match self {
            // White-walled timber town. The everyday "wattle" style carries the
            // variance; the stone hall is the rare imposing accent.
            Culture::Medieval => vec![
                // Tidy matched set — spruce frame, brick walls, plank roof.
                style("Timber Spruce", "medieval_spruce", &["spruce"], &["stone_bricks"], &["medieval_roof"]),
                // The bustling organic core: every axis jitters.
                style("Whitewash Wattle", "medieval_spruce",
                    &["oak", "birch", "spruce", "dark_oak"],
                    &["cobblestone", "stone_bricks"],
                    &["oak_wood_roof", "red_wood_roof", "brick_roof"]),
                // Rougher cottages — cobble base, warm roofs.
                style("Fieldstone", "medieval_spruce", &["oak", "dark_oak"], &["cobblestone"], &["brick_roof", "red_wood_roof"]),
                // Dark timber-frame, the tudor look.
                style("Dark Tudor", "medieval_spruce", &["dark_oak"], &["stone_bricks", "cobblestone"], &["red_wood_roof"]),
                // Rare accent: all stone, deepslate + blackstone roof — a landmark.
                style("Stone Hall", "medieval_spruce", &["dark_oak"], &["deepslate"], &["blackstone_roof"]).rare(),
                // Warm accent: acacia trade house.
                style("Acacia Trade", "medieval_spruce", &["acacia"], &["stone_bricks"], &["acacia_wood_roof"]).rare(),
            ],
            // Blackstone-and-timber register. Two bases (dark blackstone vs light
            // cherry) give the cultural split; the shrine is the teal accent.
            Culture::Japanese => vec![
                style("Dark Blackstone", "japanese_dark_blackstone", &["dark_oak"], &["blackstone"], &["red_wood_roof"]),
                // White-concrete walls under a blackstone-brick roof — the
                // white-and-slate house (the Dark Blackstone base, slate roof).
                style("White Slate", "japanese_dark_blackstone", &["dark_oak"], &["blackstone"], &["blackstone_roof"]),
                // White-concrete walls under a bamboo-mosaic roof — the light,
                // woven-bamboo house.
                style("Bamboo Roof", "japanese_dark_blackstone", &["dark_oak"], &["blackstone"], &["bamboo_roof"]),
                // White-concrete walls under a pink cherry-plank roof — the
                // blossom house.
                style("Cherry Roof", "japanese_dark_blackstone", &["dark_oak"], &["blackstone"], &["cherry_wood_roof"]),
                style("Light Cherry", "japanese_light_cherry", &["cherry"], &["blackstone"], &["red_wood_roof"]),
                style("Cypress Town", "japanese_dark_blackstone", &["dark_oak", "spruce"], &["blackstone"], &["red_wood_roof", "acacia_wood_roof"]),
                style("Cherry & Ink", "japanese_light_cherry", &["cherry", "dark_oak"], &["blackstone", "deepslate"], &["red_wood_roof", "blue_wood_roof"]),
                // Rare accent: teal/warped roof, ink-dark stone — temple/shrine.
                style("Teal Shrine", "japanese_dark_blackstone", &["dark_oak"], &["deepslate"], &["blue_wood_roof"]).rare(),
                // Rarest accent: red-concrete walls *and* posts under a teal
                // (warped/prismarine) roof — the vermilion-and-teal temple. Empty
                // wood pool so the base palette's concrete pillars survive; locked
                // stone/roof so it always reads as the deliberate landmark it is.
                style("Vermilion Hall", "japanese_red_lacquer", &[], &["blackstone"], &["blue_wood_roof"]).rare(),
                // House-tier twin of the temple: same red-concrete walls/posts, but
                // a blackstone-brick roof instead of the teal — the everyday
                // vermilion house. Roof pool overrides the base's teal roof.
                style("Vermilion House", "japanese_red_lacquer", &[], &["blackstone"], &["blackstone_roof"]),
            ],
            // Sun-bleached sandstone. Inherently low-variance — variety comes from
            // base swaps (red sandstone, prismarine noble) rather than jitter.
            // Roofs are flat, so the roof pool stays empty (base roof kept). Empty
            // wood pool too: the base's sandstone pillar (matching the wall) stays
            // instead of being overridden by a wood palette's stripped log.
            Culture::Desert => vec![
                style("Sandstone", "desert_sandstone", &[], &["sandstone"], &[]),
                style("Red Sandstone", "desert_sandstone", &[], &["red_sandstone"], &[]),
                style("Mixed Sands", "desert_sandstone", &[], &["sandstone", "red_sandstone"], &[]),
                // Rare accent: dark-prismarine roof, the noble/coastal house.
                style("Prismarine Noble", "desert_prismarine", &[], &["sandstone"], &[]).rare(),
                // Sandstone bodies with a terracotta parapet crown — the painted-
                // trim houses. The accent only colours the roofline band.
                desert_accent("Green Trim", &["sandstone"], "accent_green_terracotta"),
                desert_accent("Azure Trim", &["sandstone"], "accent_light_blue_terracotta"),
                desert_accent("Ochre Trim", &["sandstone"], "accent_orange_terracotta"),
            ],
        }
    }
}

/// A settlement's chosen style composition: a dominant / secondary / accent trio
/// from the culture's catalog, applied to buildings in a weighted 60/30/10 mix so
/// the town reads as one coherent material story with a clear secondary texture
/// and rare punctuation. Built once per settlement (see `settlement.rs`), then
/// [`district_variant`](StyleScheme::district_variant) derives a per-district
/// twist.
#[derive(Clone)]
pub struct StyleScheme {
    dominant: BuildingStyle,
    secondary: BuildingStyle,
    accent: BuildingStyle,
}

impl StyleScheme {
    /// Pick the trio from `culture`'s catalog. Dominant and secondary are drawn
    /// from the everyday styles, so a [`rare`](BuildingStyle::rare) landmark never
    /// blankets a town; the 10% accent slot prefers a rare style, falling back to
    /// a remaining everyday one. Deterministic in `rng`.
    pub fn generate(culture: Culture, rng: &mut RNG) -> Self {
        let catalog = culture.style_catalog();
        let mut common: Vec<BuildingStyle> = catalog.iter().filter(|s| !s.rare).cloned().collect();
        let mut rare: Vec<BuildingStyle> = catalog.into_iter().filter(|s| s.rare).collect();
        // Degenerate catalog (everything flagged rare): treat them as common.
        if common.is_empty() {
            common = rare.clone();
        }

        fn take(v: &mut Vec<BuildingStyle>, rng: &mut RNG) -> BuildingStyle {
            v.remove(rng.rand_i32_range(0, v.len() as i32) as usize)
        }

        let dominant = take(&mut common, rng);
        let secondary = if common.is_empty() { dominant.clone() } else { take(&mut common, rng) };
        let accent = if !rare.is_empty() {
            take(&mut rare, rng)
        } else if !common.is_empty() {
            take(&mut common, rng)
        } else {
            secondary.clone()
        };

        Self { dominant, secondary, accent }
    }

    /// Derive a per-district variant of this town scheme so each quarter reads as
    /// its own while staying clearly part of the city. The town's **dominant**
    /// stays the district's dominant (so the 60% plurality material is constant
    /// city-wide — the shared thread) and the town's **accent** is inherited
    /// unchanged (landmarks stay consistent); only the 30% **secondary** is
    /// re-rolled to a different everyday style from the culture's catalog, giving
    /// the district its local texture. Deterministic in `rng`. Falls back to the
    /// town scheme's own secondary when the catalog has no other everyday style.
    pub fn district_variant(&self, culture: Culture, rng: &mut RNG) -> StyleScheme {
        let alternatives: Vec<BuildingStyle> = culture
            .style_catalog()
            .into_iter()
            .filter(|s| !s.rare && s.name != self.dominant.name)
            .collect();
        let secondary = if alternatives.is_empty() {
            self.secondary.clone()
        } else {
            alternatives[rng.rand_i32_range(0, alternatives.len() as i32) as usize].clone()
        };
        StyleScheme {
            dominant: self.dominant.clone(),
            secondary,
            accent: self.accent.clone(),
        }
    }

    /// Pick a style for one building: 60% dominant, 30% secondary, 10% accent.
    pub fn next_style(&self, rng: &mut RNG) -> &BuildingStyle {
        match rng.rand_i32_range(0, 100) {
            0..60 => &self.dominant,
            60..90 => &self.secondary,
            _ => &self.accent,
        }
    }

    pub fn dominant(&self) -> &BuildingStyle { &self.dominant }
    pub fn secondary(&self) -> &BuildingStyle { &self.secondary }
    pub fn accent(&self) -> &BuildingStyle { &self.accent }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::data::LoadedData;

    /// Every style in every culture's catalog must reference palette files that
    /// exist — `roll_palette` panics on a missing base or variant id, so rolling
    /// each style a few times offline catches a typo in the catalog without a
    /// live server.
    #[test]
    fn catalog_styles_roll_cleanly() {
        let data = LoadedData::load().expect("Failed to load data");
        let mut rng = RNG::new(7);

        for culture in [Culture::Medieval, Culture::Japanese, Culture::Desert] {
            let catalog = culture.style_catalog();
            assert!(catalog.len() >= 3, "{culture:?} needs ≥3 styles for a 60/30/10 mix");
            for bstyle in &catalog {
                // Roll several times so multi-entry pools exercise each branch,
                // both unbiased and with a local-wood bias toward an existing
                // (oak) and a missing (mangrove) wood palette.
                let oak = PaletteId::from("oak");
                let mangrove = PaletteId::from("mangrove"); // no palette file → ignored
                for local in [None, Some(&oak), Some(&mangrove)] {
                    for _ in 0..8 {
                        let _ = bstyle.roll_palette(&mut rng, &data, local);
                    }
                }
            }
        }
    }

    /// In a variable wood pool, the local wood should win a clear majority
    /// (~`LOCAL_WOOD_FRACTION`); a locked single-entry pool must ignore the bias.
    #[test]
    fn local_wood_bias_dominates_variable_pools() {
        let variable = vec![PaletteId::from("oak"), PaletteId::from("spruce"), PaletteId::from("dark_oak")];
        let locked = vec![PaletteId::from("dark_oak")];
        let local = PaletteId::from("birch"); // not in either pool
        let mut rng = RNG::new(99);

        let mut local_hits = 0;
        const N: i32 = 2000;
        for _ in 0..N {
            if pick_wood(&mut rng, &variable, Some(&local)) == local {
                local_hits += 1;
            }
            // Locked pool never yields the (out-of-pool) local wood.
            assert_eq!(pick_wood(&mut rng, &locked, Some(&local)), locked[0]);
        }
        let frac = local_hits as f32 / N as f32;
        assert!((frac - LOCAL_WOOD_FRACTION).abs() < 0.06, "local fraction {frac} off target");
    }

    /// A district variant keeps the town's dominant and accent, only swaps the
    /// secondary, and never promotes a rare style into the secondary slot.
    #[test]
    fn district_variant_keeps_dominant_and_accent() {
        for culture in [Culture::Medieval, Culture::Japanese, Culture::Desert] {
            let mut rng = RNG::new(3);
            let town = StyleScheme::generate(culture, &mut rng);
            let mut saw_different_secondary = false;
            for _ in 0..40 {
                let d = town.district_variant(culture, &mut rng);
                assert_eq!(d.dominant().name, town.dominant().name, "{culture:?} dominant changed");
                assert_eq!(d.accent().name, town.accent().name, "{culture:?} accent changed");
                assert!(!d.secondary().rare, "{culture:?} district secondary is rare");
                if d.secondary().name != town.secondary().name {
                    saw_different_secondary = true;
                }
            }
            // Every culture's catalog has ≥2 everyday styles, so the secondary
            // should differ from the town's at least sometimes.
            assert!(saw_different_secondary, "{culture:?} never varied the district secondary");
        }
    }

    /// The scheme keeps rare styles out of the dominant/secondary slots and
    /// applies the trio at roughly 60/30/10.
    #[test]
    fn scheme_excludes_rare_and_weights_60_30_10() {
        for culture in [Culture::Medieval, Culture::Japanese, Culture::Desert] {
            let mut rng = RNG::new(5);
            for _ in 0..50 {
                let scheme = StyleScheme::generate(culture, &mut rng);
                assert!(!scheme.dominant().rare, "{culture:?} dominant is rare");
                assert!(!scheme.secondary().rare, "{culture:?} secondary is rare");
            }

            // Distribution check on one scheme.
            let scheme = StyleScheme::generate(culture, &mut rng);
            let (mut d, mut s, mut a) = (0, 0, 0);
            const N: i32 = 3000;
            for _ in 0..N {
                let name = scheme.next_style(&mut rng).name;
                if name == scheme.dominant().name { d += 1; }
                else if name == scheme.secondary().name { s += 1; }
                else { a += 1; }
            }
            // Loose bounds (names can collide when slots fall back to a clone).
            assert!(d >= s && s >= a, "{culture:?} weights out of order: {d}/{s}/{a}");
            assert!(a > 0, "{culture:?} accent never appeared");
        }
    }
}
