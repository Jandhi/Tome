//! Layered road naming.
//!
//! Gives each geometric road (`road_labels`) a name, picking the most specific
//! layer that applies, then logging the distribution so we can see the mix over
//! a few runs:
//!
//! 1. **Landmark** — a trade building fronts the road → `Smith Street`,
//!    `Mill Lane` (from the `BuildClaim::Structure`/`ProductionArea` type).
//! 2. **Destination** — the road reaches a gate → `Northgate Road`; or it's the
//!    main arterial nearest the town centre → `High Street`.
//! 3. **Generic** — culture- and biome-flavoured filler (`Birch Lane`,
//!    `Dune Road`).
//!
//! The suffix (Street/Lane/Close/…) comes from the road's tier + culture; names
//! are de-duplicated.

use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::World;
use crate::generator::buildings_v2::Culture;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::noise::RNG;

use super::path::{Path, PathPriority};

/// All road-naming vocabulary + tuning, loaded from `data/road_names.yaml`.
#[derive(Debug, Deserialize)]
struct RoadNamesCfg {
    landmark_fraction: f64,
    frontage_radius: i32,
    flavour: FlavourCfg,
    /// structure type -> name stem.
    trades: HashMap<String, String>,
    /// culture key -> generic stems contributed by that culture.
    cultures: HashMap<String, Vec<String>>,
    /// ordered, most-specific first; empty `contains` = fallback.
    biomes: Vec<BiomeRule>,
    /// culture key -> tier -> suffix pool.
    suffixes: HashMap<String, TierSuffixes>,
}

#[derive(Debug, Deserialize)]
struct FlavourCfg {
    old_pct: i32,
    new_pct: i32,
}

#[derive(Debug, Deserialize)]
struct BiomeRule {
    contains: Vec<String>,
    words: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TierSuffixes {
    high: Vec<String>,
    medium: Vec<String>,
    low: Vec<String>,
}

impl RoadNamesCfg {
    /// Generic stems for a biome: the first matching rule's words plus the
    /// culture's words.
    fn generic_pool(&self, biome_name: &str, culture: Culture) -> Vec<&str> {
        let biome_words = self
            .biomes
            .iter()
            .find(|r| r.contains.is_empty() || r.contains.iter().any(|s| biome_name.contains(s)))
            .map(|r| r.words.as_slice())
            .unwrap_or(&[]);
        let culture_words = self
            .cultures
            .get(culture_key(culture))
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        biome_words
            .iter()
            .chain(culture_words)
            .map(String::as_str)
            .collect()
    }

    /// Suffix pool for a road's culture + tier (medieval is the fallback culture).
    fn suffix_pool(&self, culture: Culture, tier: PathPriority) -> &[String] {
        let tiers = self
            .suffixes
            .get(culture_key(culture))
            .or_else(|| self.suffixes.get("medieval"))
            .expect("road_names.yaml: no suffixes for culture or 'medieval' fallback");
        match tier {
            PathPriority::High => &tiers.high,
            PathPriority::Medium => &tiers.medium,
            PathPriority::Low => &tiers.low,
        }
    }
}

/// The yaml key for a culture's vocabulary.
fn culture_key(culture: Culture) -> &'static str {
    match culture {
        Culture::Desert => "desert",
        Culture::Japanese => "japanese",
        Culture::Medieval => "medieval",
    }
}

/// Which layer named a road — tallied for the distribution log.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Landmark,
    Gate,
    Centre,
    Generic,
}

impl Layer {
    fn label(self) -> &'static str {
        match self {
            Layer::Landmark => "landmark",
            Layer::Gate => "gate",
            Layer::Centre => "centre",
            Layer::Generic => "generic",
        }
    }
}

/// Name every road in `road_labels`. See module docs. Logs the per-layer
/// distribution and each road's name.
pub fn name_roads_layered(
    world: &World,
    road_labels: &HashMap<Point2D, u32>,
    paths: &[Path],
    gates: &[(Point3D, Cardinal)],
    culture: Culture,
    rng: &mut RNG,
) -> HashMap<u32, String> {
    let cfg: RoadNamesCfg = match load_yaml("road_names.yaml") {
        Ok(c) => c,
        Err(e) => {
            log::warn!("road_names.yaml failed to load ({e}); roads stay numbered");
            return HashMap::new();
        }
    };

    // Invert: road id -> its cells.
    let mut cells_of: HashMap<u32, Vec<Point2D>> = HashMap::new();
    for (&c, &rid) in road_labels {
        cells_of.entry(rid).or_default().push(c);
    }
    if cells_of.is_empty() {
        return HashMap::new();
    }

    // Tier per road: the highest priority among paths whose cells it contains.
    let mut tier_of: HashMap<u32, PathPriority> = HashMap::new();
    for path in paths {
        for p in path.points() {
            if let Some(&rid) = road_labels.get(&p.drop_y()) {
                let e = tier_of.entry(rid).or_insert(PathPriority::Low);
                if tier_rank(path.priority()) > tier_rank(*e) {
                    *e = path.priority();
                }
            }
        }
    }

    // Gate road ids: the road number at (or nearest to) each gate, with its
    // cardinal — so the road can be named after the gate.
    let mut gate_dir: HashMap<u32, Cardinal> = HashMap::new();
    // The road runs through the gate, so it's at/right beside the gate cell —
    // a tight radius avoids tagging a road that merely passes nearby.
    for (pos, card) in gates {
        if let Some(rid) = road_at(road_labels, pos.drop_y(), 2) {
            gate_dir.entry(rid).or_insert(*card);
        }
    }

    // Town-centre proxy: centroid of all road cells. The longest High-tier road
    // nearest it becomes the high street.
    let (mut sx, mut sz, mut n) = (0i64, 0i64, 0i64);
    for cells in cells_of.values() {
        for c in cells {
            sx += c.x as i64;
            sz += c.y as i64;
            n += 1;
        }
    }
    let centre = Point2D::new((sx / n.max(1)) as i32, (sz / n.max(1)) as i32);
    let high_street = cells_of
        .iter()
        .filter(|(rid, _)| matches!(tier_of.get(rid), Some(PathPriority::High)))
        .min_by_key(|(_, cells)| {
            // nearest-to-centre, tie-broken toward longer roads
            let d = cells.iter().map(|c| c.distance_squared(&centre)).min().unwrap_or(i32::MAX);
            (d, -(cells.len() as i32))
        })
        .map(|(&rid, _)| rid);

    // One road per trade: the road that fronts a trade most keeps it as a
    // landmark; the rest fall through to the next layer (often generic). This is
    // what kills the "Smith Lane / Smith Walk / Old Smith Lane" pile-up.
    let mut owner_of_trade: HashMap<String, (u32, usize)> = HashMap::new();
    for (&rid, cells) in &cells_of {
        if let Some((stem, count)) = frontage_trade(world, cells, &cfg) {
            let e = owner_of_trade.entry(stem).or_insert((rid, 0));
            if count > e.1 {
                *e = (rid, count);
            }
        }
    }
    // Cap how many roads carry a trade name to a fraction of the network, so the
    // town isn't mostly "industry" streets. Keep the strongest-frontage trades;
    // the rest fall through to gate/centre/generic for variety.
    let cap = ((cells_of.len() as f64 * cfg.landmark_fraction).round() as usize).max(1);
    let mut owners: Vec<(String, u32, usize)> = owner_of_trade
        .into_iter()
        .map(|(stem, (rid, count))| (stem, rid, count))
        .collect();
    owners.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1))); // frontage desc, id stable
    owners.truncate(cap);
    let landmark_stem: HashMap<u32, String> =
        owners.into_iter().map(|(stem, rid, _)| (rid, stem)).collect();

    // Build names, most-specific layer first, de-duplicating as we go.
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut counts = [0usize; 4]; // landmark, gate, centre, generic

    let mut ids: Vec<u32> = cells_of.keys().copied().collect();
    ids.sort_unstable();
    for rid in ids {
        let cells = &cells_of[&rid];
        let tier = tier_of.get(&rid).copied().unwrap_or(PathPriority::Low);
        let pool = cfg.suffix_pool(culture, tier);
        let suf = pool[rid as usize % pool.len()].as_str();
        let flav = flavour(&cfg, rng); // occasional Old/New, only landmark/generic

        let (name, layer) = if let Some(stem) = landmark_stem.get(&rid) {
            (dedup(&mut used, &format!("{flav}{stem}"), suf), Layer::Landmark)
        } else if let Some(card) = gate_dir.get(&rid) {
            // A gate already reads "Northgate", a road suffix still fits.
            (dedup(&mut used, &format!("{}gate", cardinal_word(*card)), suf), Layer::Gate)
        } else if Some(rid) == high_street {
            (dedup(&mut used, "High", suf), Layer::Centre)
        } else {
            let biome = world.get_surface_biome_at(cells[cells.len() / 2]);
            // OOB centre cell -> no biome hint; generic_pool handles an unknown
            // biome name by falling back to its culture-only word list.
            let biome_name = biome.as_ref().map(|b| b.name()).unwrap_or("");
            let pool = cfg.generic_pool(biome_name, culture);
            (generic_name(&pool, rid, suf, flav, &mut used), Layer::Generic)
        };

        log::info!("  road {rid}: '{name}' ({})", layer.label());
        counts[match layer {
            Layer::Landmark => 0,
            Layer::Gate => 1,
            Layer::Centre => 2,
            Layer::Generic => 3,
        }] += 1;
        names.insert(rid, name);
    }

    println!(
        "road names [{} roads]: {} landmark, {} gate, {} centre, {} generic",
        names.len(),
        counts[0],
        counts[1],
        counts[2],
        counts[3],
    );
    names
}

/// The most-fronting trade along `cells` as `(name stem, frontage cell count)`,
/// if any. Counts `Structure`/`ProductionArea` claims within `cfg.frontage_radius`
/// of the road cells (the centreline is set back from footprints by the
/// pavement+verge). The count lets the caller give a trade to the road that
/// fronts it most, so the same building doesn't name several roads.
fn frontage_trade(world: &World, cells: &[Point2D], cfg: &RoadNamesCfg) -> Option<(String, usize)> {
    let r = cfg.frontage_radius;
    let mut tally: HashMap<String, usize> = HashMap::new();
    for &c in cells {
        for dx in -r..=r {
            for dz in -r..=r {
                let n = Point2D::new(c.x + dx, c.y + dz);
                match world.get_claim(n) {
                    Some(BuildClaim::Structure(s)) | Some(BuildClaim::ProductionArea(s)) => {
                        *tally.entry(s.structure_type.0.clone()).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    // Require a real frontage (a few cells), not a single corner clip.
    tally
        .into_iter()
        .filter(|(_, n)| *n >= 4)
        .max_by_key(|(_, n)| *n)
        .map(|(trade, n)| {
            let stem = cfg.trades.get(&trade).cloned().unwrap_or_else(|| title_case(&trade));
            (stem, n)
        })
}

/// A unique filler name from `pool`: tries each stem (rotated by id) with the
/// road's suffix until one is unused, so more generics don't mean a wall of
/// "Old Dune".
fn generic_name(
    pool: &[&str],
    rid: u32,
    suffix: &str,
    flav: &str,
    used: &mut std::collections::HashSet<String>,
) -> String {
    if pool.is_empty() {
        return dedup(used, &format!("{flav}Road"), suffix);
    }
    for k in 0..pool.len() {
        let stem = pool[(rid as usize + k) % pool.len()];
        let cand = format!("{flav}{stem} {suffix}");
        if used.insert(cand.clone()) {
            return cand;
        }
    }
    // Whole pool exhausted with this suffix: fall back to Old/New prefixing.
    dedup(used, &format!("{flav}{}", pool[rid as usize % pool.len()]), suffix)
}

/// An occasional "Old "/"New " prefix for flavour — rolled from the RNG, so it
/// varies run to run. Probabilities come from the config.
fn flavour(cfg: &RoadNamesCfg, rng: &mut RNG) -> &'static str {
    let roll = rng.rand_i32_range(0, 100);
    if roll < cfg.flavour.old_pct {
        "Old "
    } else if roll < cfg.flavour.old_pct + cfg.flavour.new_pct {
        "New "
    } else {
        ""
    }
}

/// Combine `stem` + `suffix` into a unique name, prefixing Old/New/etc. (or
/// finally a number) on collision.
fn dedup(
    used: &mut std::collections::HashSet<String>,
    stem: &str,
    suffix: &str,
) -> String {
    let base = format!("{stem} {suffix}");
    if used.insert(base.clone()) {
        return base;
    }
    for pre in ["Old", "New", "Upper", "Lower", "Little", "Great"] {
        // Don't double a prefix the name already leads with (e.g. flavour "Old").
        if base.starts_with(pre) {
            continue;
        }
        let cand = format!("{pre} {base}");
        if used.insert(cand.clone()) {
            return cand;
        }
    }
    let mut i = 2;
    loop {
        let cand = format!("{base} {i}");
        if used.insert(cand.clone()) {
            return cand;
        }
        i += 1;
    }
}

/// Road id at `cell`, else the nearest road id within `radius` (rings outward).
fn road_at(road_labels: &HashMap<Point2D, u32>, cell: Point2D, radius: i32) -> Option<u32> {
    if let Some(&rid) = road_labels.get(&cell) {
        return Some(rid);
    }
    for r in 1..=radius {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                if let Some(&rid) = road_labels.get(&Point2D::new(cell.x + dx, cell.y + dz)) {
                    return Some(rid);
                }
            }
        }
    }
    None
}

fn cardinal_word(c: Cardinal) -> &'static str {
    match c {
        Cardinal::North => "North",
        Cardinal::East => "East",
        Cardinal::South => "South",
        Cardinal::West => "West",
    }
}

fn tier_rank(p: PathPriority) -> u8 {
    match p {
        PathPriority::High => 2,
        PathPriority::Medium => 1,
        PathPriority::Low => 0,
    }
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
