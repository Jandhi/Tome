//! Feature-driven, per-culture naming of urban districts.
//!
//! A city is partitioned into 3–5 urban [`District`](super::District)s (see
//! `districts/constants.rs`). This names each one after what it *contains*, from a
//! deliberately SPREAD weighted mix of sources so any of them can lead:
//!   - its dominant **trade** (a smithy-heavy district → "Smith Row"),
//!   - a resident **family** (a manor's surname → "Blackwell End"),
//!   - a **green** it holds (a park → "Garden Quarter"),
//!   - its **cardinal direction** relative to the town centre ("Northgate"),
//!   - or just **generic** cultural vocab ("Old Quarter") — always available, so a
//!     featureless district still names well and the lead source stays varied.
//!
//! Composition mirrors the settlement namer ([`crate::generator::naming`]): a lead
//! word is drawn from a weighted theme, then a culture suffix is attached
//! ("Smith" + " Row", "Kaji" + "machi") — or, for the desert, the Arabic
//! quarter-prefix form ("Hayy" + as-Souk). Vocabulary lives in
//! `data/districts_names.yaml`; everything is seeded so a district's name is stable
//! for a given town seed.

use std::collections::{HashMap, HashSet};

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::World;
use crate::generator::buildings_v2::Culture;
use crate::generator::districts::{DistrictID, ParcelType};
use crate::generator::naming::{assimilated_article, culture_key};
use crate::generator::BuildClaim;
use crate::geometry::Point2D;
use crate::noise::RNG;

/// Vocabulary + tuning, loaded from `data/districts_names.yaml`.
#[derive(Debug, Deserialize)]
struct DistrictNamesCfg {
    weights: Weights,
    /// structure-type id -> trade concept.
    trades: HashMap<String, String>,
    /// culture key -> that culture's morphology + vocabulary.
    cultures: HashMap<String, CultureWords>,
}

#[derive(Debug, Deserialize)]
struct Weights {
    trade: i32,
    family: i32,
    green: i32,
    direction: i32,
    generic: i32,
}

#[derive(Debug, Deserialize)]
struct CultureWords {
    /// Suffix forms attached to the lead. A leading space makes a separate word
    /// (" Row" -> "Smith Row"); otherwise it attaches ("gate" -> "Smithgate").
    #[serde(default)]
    suffixes: Vec<String>,
    /// Arabic quarter-prefix ("Hayy"): replaces suffixes when present.
    #[serde(default)]
    prefix: Option<String>,
    /// cardinal (north/.../central) -> this culture's word.
    directions: HashMap<String, String>,
    /// trade concept -> this culture's district words (may be empty for a trade
    /// this culture has no district word for).
    trade_words: HashMap<String, Vec<String>>,
    greens: Vec<String>,
    generic: Vec<String>,
}

/// A named district. (Subtitle/gloss intentionally omitted — district names ride
/// inside the chronicle prose, not the welcome title.)
#[derive(Debug, Clone)]
pub struct DistrictName {
    pub name: String,
}

/// The features of one district that can seed its name. Pure data, so
/// [`compose_district_name`] is unit-testable without a `World`.
struct DistrictFeatures {
    /// Dominant trade concept present, if any.
    trade: Option<String>,
    /// Surnames of families (manors) sitting in the district.
    families: Vec<String>,
    /// Whether a park/green sits in the district.
    has_green: bool,
    /// Cardinal word: "north"/"east"/"south"/"west"/"central".
    direction: String,
}

/// One weighted name source.
struct Theme<'a> {
    words: Vec<&'a str>,
    weight: i32,
}

/// Name every urban district in `world`. `centre` is the town centroid (build-area
/// local coords); `manors` are `(position, surname)` of placed manor families;
/// `parks` are open-space landmark positions. Returns a name per urban
/// `DistrictID`. Empty if the vocabulary file can't be loaded (callers then fall
/// back to ungrouped landmarks).
pub fn name_districts(
    world: &World,
    culture: Culture,
    centre: Point2D,
    manors: &[(Point2D, String)],
    parks: &[Point2D],
    rng: &mut RNG,
) -> HashMap<DistrictID, DistrictName> {
    let cfg: DistrictNamesCfg = match load_yaml("districts_names.yaml") {
        Ok(c) => c,
        Err(e) => {
            log::warn!("districts_names.yaml failed to load ({e}); districts left unnamed");
            return HashMap::new();
        }
    };
    let cul = match cfg
        .cultures
        .get(culture_key(culture))
        .or_else(|| cfg.cultures.get("medieval"))
    {
        Some(c) => c,
        None => {
            log::warn!("districts_names.yaml: no vocabulary for culture or 'medieval' fallback");
            return HashMap::new();
        }
    };

    // Stable order so derived RNGs (hence names) don't depend on map iteration.
    let mut ids: Vec<DistrictID> = world
        .districts
        .iter()
        .filter(|(_, d)| d.data.parcel_type == ParcelType::Urban)
        .map(|(id, _)| *id)
        .collect();
    ids.sort_by_key(|id| id.0);

    // Town radius from district centroids — the "central" threshold for direction.
    let centroids: HashMap<DistrictID, Point2D> = ids
        .iter()
        .filter_map(|id| world.districts.get(id).map(|d| (*id, centroid(&d.data.points_2d))))
        .collect();
    let radius = centroids
        .values()
        .map(|&c| dist(c, centre))
        .fold(0.0_f32, f32::max);

    let mut out: HashMap<DistrictID, DistrictName> = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for id in ids {
        let Some(district) = world.districts.get(&id) else { continue };
        let cells = &district.data.points_2d;

        // Trade: most common mapped Structure type in the district.
        let trade = dominant_trade(world, cells, &cfg.trades);
        // Families / greens sitting inside the district.
        let families: Vec<String> = manors
            .iter()
            .filter(|(p, _)| cells.contains(p))
            .map(|(_, s)| s.clone())
            .collect();
        let has_green = parks.iter().any(|p| cells.contains(p));
        let direction = centroids
            .get(&id)
            .map(|&c| cardinal_of(c, centre, radius))
            .unwrap_or("central")
            .to_string();

        let features = DistrictFeatures { trade, families, has_green, direction };

        // A few attempts to dodge a name collision with another district; the
        // spread vocab makes clashes rare across 3–5 districts.
        let mut drng = rng.derive();
        let mut name = compose_district_name(&features, cul, &cfg.weights, &mut drng);
        for _ in 0..6 {
            if !used.contains(&name) {
                break;
            }
            name = compose_district_name(&features, cul, &cfg.weights, &mut drng);
        }
        used.insert(name.clone());
        out.insert(id, DistrictName { name });
    }
    out
}

/// Compose a single district name from its features. Pure: all randomness comes
/// from `rng`, so a fixed seed gives a fixed name.
fn compose_district_name(
    f: &DistrictFeatures,
    cul: &CultureWords,
    weights: &Weights,
    rng: &mut RNG,
) -> String {
    let mut themes: Vec<Theme> = Vec::new();
    if let Some(words) = f.trade.as_ref().and_then(|c| cul.trade_words.get(c)) {
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        if !words.is_empty() {
            themes.push(Theme { words, weight: weights.trade });
        }
    }
    if !f.families.is_empty() {
        themes.push(Theme {
            words: f.families.iter().map(String::as_str).collect(),
            weight: weights.family,
        });
    }
    if f.has_green && !cul.greens.is_empty() {
        themes.push(Theme {
            words: cul.greens.iter().map(String::as_str).collect(),
            weight: weights.green,
        });
    }
    if let Some(dirw) = cul.directions.get(&f.direction) {
        themes.push(Theme { words: vec![dirw.as_str()], weight: weights.direction });
    }
    if !cul.generic.is_empty() {
        themes.push(Theme {
            words: cul.generic.iter().map(String::as_str).collect(),
            weight: weights.generic,
        });
    }
    if themes.is_empty() {
        return "Quarter".to_string();
    }

    let ti = pick_weighted_theme(&themes, rng);
    let lead = *rng.choose(&themes[ti].words);
    attach_morphology(lead, cul, rng)
}

/// Apply the culture's district morphology to a lead word: the Arabic
/// `Hayy <art>-<lead>` prefix form, else `lead + suffix`, else the bare lead.
fn attach_morphology(lead: &str, cul: &CultureWords, rng: &mut RNG) -> String {
    if let Some(prefix) = &cul.prefix {
        let art = assimilated_article(lead);
        return format!("{prefix} {art}-{lead}");
    }
    if cul.suffixes.is_empty() {
        return lead.to_string();
    }
    let suffix = rng.choose(&cul.suffixes);
    format!("{lead}{suffix}")
}

/// Weighted pick over themes; returns the chosen index.
fn pick_weighted_theme(themes: &[Theme], rng: &mut RNG) -> usize {
    let total: i32 = themes.iter().map(|t| t.weight.max(0)).sum();
    if total <= 0 {
        return 0;
    }
    let mut r = rng.rand_i32_range(0, total);
    for (i, t) in themes.iter().enumerate() {
        let w = t.weight.max(0);
        if r < w {
            return i;
        }
        r -= w;
    }
    themes.len() - 1
}

/// The most common mapped Structure type among `cells`, as its trade concept.
/// Tie-break alphabetically by concept so the choice is deterministic.
fn dominant_trade(
    world: &World,
    cells: &HashSet<Point2D>,
    trades: &HashMap<String, String>,
) -> Option<String> {
    let mut tally: HashMap<&str, usize> = HashMap::new();
    for &c in cells {
        if let Some(BuildClaim::Structure(s)) | Some(BuildClaim::ProductionArea(s)) = world.get_claim(c)
        {
            if let Some(concept) = trades.get(&s.structure_type.0) {
                *tally.entry(concept.as_str()).or_insert(0) += 1;
            }
        }
    }
    tally
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(concept, _)| concept.to_string())
}

/// Cardinal word of `p` relative to `centre`; "central" when within the inner
/// third of `radius`. North is -Z, south +Z, east +X, west -X (Point2D.y is Z).
fn cardinal_of(p: Point2D, centre: Point2D, radius: f32) -> &'static str {
    let dx = (p.x - centre.x) as f32;
    let dz = (p.y - centre.y) as f32;
    if radius > 0.0 && (dx * dx + dz * dz).sqrt() < radius * 0.33 {
        return "central";
    }
    if dx.abs() >= dz.abs() {
        if dx >= 0.0 { "east" } else { "west" }
    } else if dz >= 0.0 {
        "south"
    } else {
        "north"
    }
}

fn centroid(cells: &HashSet<Point2D>) -> Point2D {
    let n = cells.len().max(1) as i32;
    let sum = cells.iter().fold(Point2D::ZERO, |a, &p| a + p);
    Point2D::new(sum.x / n, sum.y / n)
}

fn dist(a: Point2D, b: Point2D) -> f32 {
    let dx = (a.x - b.x) as f32;
    let dz = (a.y - b.y) as f32;
    (dx * dx + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::Seed;

    fn cfg() -> DistrictNamesCfg {
        load_yaml("districts_names.yaml").expect("districts_names.yaml parses")
    }

    fn cul<'a>(cfg: &'a DistrictNamesCfg, key: &str) -> &'a CultureWords {
        cfg.cultures.get(key).expect("culture present")
    }

    /// Every culture defines a direction word for all five cardinals (so any
    /// district can always render a direction-led name).
    #[test]
    fn every_culture_covers_all_directions() {
        let cfg = cfg();
        for key in ["medieval", "japanese", "desert"] {
            let c = cul(&cfg, key);
            for dir in ["north", "south", "east", "west", "central"] {
                assert!(c.directions.contains_key(dir), "{key} missing direction '{dir}'");
            }
            // A naming morphology must exist: suffixes OR a prefix.
            assert!(
                !c.suffixes.is_empty() || c.prefix.is_some(),
                "{key} has neither suffixes nor a prefix",
            );
            assert!(!c.generic.is_empty(), "{key} has no generic vocab");
        }
    }

    /// Same seed + same features → same name (deterministic).
    #[test]
    fn naming_is_deterministic() {
        let cfg = cfg();
        let c = cul(&cfg, "medieval");
        let f = DistrictFeatures {
            trade: Some("smith".into()),
            families: vec![],
            has_green: false,
            direction: "east".into(),
        };
        let mut a = RNG::from_seed_and_string(Seed::from(42i64), "d");
        let mut b = RNG::from_seed_and_string(Seed::from(42i64), "d");
        assert_eq!(
            compose_district_name(&f, c, &cfg.weights, &mut a),
            compose_district_name(&f, c, &cfg.weights, &mut b),
        );
    }

    /// A trade-only district (no family/green, weights forced onto trade) names
    /// from its trade words — grounding: the lead is a real feature word.
    #[test]
    fn trade_only_name_uses_trade_words() {
        let cfg = cfg();
        let c = cul(&cfg, "medieval");
        // Drop every non-trade weight so the trade theme always wins.
        let weights = Weights { trade: 1, family: 0, green: 0, direction: 0, generic: 0 };
        let smith_words = &c.trade_words["smith"];
        let f = DistrictFeatures {
            trade: Some("smith".into()),
            families: vec![],
            has_green: false,
            direction: "east".into(),
        };
        for seed in 0..20i64 {
            let mut rng = RNG::from_seed_and_string(Seed::from(seed), "t");
            let name = compose_district_name(&f, c, &weights, &mut rng);
            assert!(
                smith_words.iter().any(|w| name.starts_with(w.as_str())),
                "'{name}' is not led by a smith word {smith_words:?}",
            );
        }
    }

    /// Desert names take the Arabic quarter-prefix form, article-assimilated.
    #[test]
    fn desert_uses_hayy_prefix() {
        let cfg = cfg();
        let c = cul(&cfg, "desert");
        let weights = Weights { trade: 1, family: 0, green: 0, direction: 0, generic: 0 };
        let f = DistrictFeatures {
            trade: Some("smith".into()), // Haddad -> "Hayy al-Haddad"
            families: vec![],
            has_green: false,
            direction: "north".into(),
        };
        let mut rng = RNG::from_seed_and_string(Seed::from(1i64), "x");
        let name = compose_district_name(&f, c, &weights, &mut rng);
        assert!(name.starts_with("Hayy "), "desert name should start with Hayy: '{name}'");
        assert!(name.contains("Haddad"), "desert smith name should contain Haddad: '{name}'");
    }
}
