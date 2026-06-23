//! Open-space (dead-space) detection and furnishing: after buildings + roads are
//! placed, the urban cells that remain unclaimed are "green" — the leftover gaps
//! between the built-up lots.
//!
//! [`detect_regions`] flood-fills those unclaimed cells into connected
//! components ([`Region`]s) and classifies each by size and position into a
//! [`RegionType`] — plaza, nook, park, or yard. The per-type `furnish_*`
//! functions (in the submodules) then decorate each gap in place.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::World;
use crate::generator::BuildClaim;
use crate::geometry::{Point2D, CARDINALS_2D};
use crate::noise::RNG;

mod props;
mod theme;
mod nook;
mod plaza;
mod yard;
mod park;
#[cfg(test)]
mod test;
pub use nook::furnish_nook;
pub use plaza::{furnish_plaza, PlazaType};
pub use theme::Theme;
pub use yard::furnish_yard;
pub use park::{furnish_park, ParkType};

/// Where a region sits relative to the city's outer extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Touches the wall/gate or the edge of the urban area — a perimeter gap
    /// (backyards, allotments, against-the-wall strips).
    Edge,
    /// Fully ringed by built-up cells — an interior gap (courtyards, plazas,
    /// parks).
    Interior,
}

/// Cell-count threshold at/above which a region is considered "large" — big
/// enough to host a designed layout rather than a single feature.
pub const LARGE_MIN_AREA: usize = 50;

/// What a region should become, from its (kind, size) bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionType {
    /// Interior + large — a paved civic square.
    Plaza,
    /// Interior + small — a little space ringed by buildings.
    Nook,
    /// Edge + large — a perimeter green space.
    Park,
    /// Edge + small — a perimeter backyard / garden.
    Yard,
}

impl RegionType {
    pub fn name(self) -> &'static str {
        match self {
            RegionType::Plaza => "plaza",
            RegionType::Nook => "nook",
            RegionType::Park => "park",
            RegionType::Yard => "yard",
        }
    }

}

/// One connected component of green (unclaimed urban) cells — a leftover gap in
/// the built-up area.
#[derive(Debug, Clone)]
pub struct Region {
    pub cells: Vec<Point2D>,
    /// Cell count.
    pub area: usize,
    /// Edge (perimeter) vs interior, by adjacency to the wall / city extent.
    pub kind: RegionKind,
    /// `area >= LARGE_MIN_AREA` — large enough for a designed layout.
    pub large: bool,
}

impl Region {
    /// The named category this region falls into.
    pub fn region_type(&self) -> RegionType {
        match (self.kind, self.large) {
            (RegionKind::Interior, true) => RegionType::Plaza,
            (RegionKind::Interior, false) => RegionType::Nook,
            (RegionKind::Edge, true) => RegionType::Park,
            (RegionKind::Edge, false) => RegionType::Yard,
        }
    }

    /// Centroid cell of the region.
    pub fn centroid(&self) -> Point2D {
        let n = self.cells.len().max(1) as i64;
        let (sx, sz) = self
            .cells
            .iter()
            .fold((0i64, 0i64), |(ax, az), c| (ax + c.x as i64, az + c.y as i64));
        Point2D::new((sx / n) as i32, (sz / n) as i32)
    }
}

/// Per-culture word overrides for a [`PlaceSchema`] — e.g. a desert market trades
/// its generic suffixes for `Souk`/`Bazaar`. An empty list falls back to the
/// schema's default words for that part.
#[derive(Debug, Default, Deserialize)]
struct CultureWords {
    #[serde(default)]
    stems: Vec<String>,
    #[serde(default)]
    suffixes: Vec<String>,
}

/// A `<stem> <suffix>` naming schema for one open-space kind, with optional
/// per-culture word lists (keyed by culture: `desert`, `medieval`, `japanese`).
#[derive(Debug, Deserialize)]
struct PlaceSchema {
    stems: Vec<String>,
    suffixes: Vec<String>,
    #[serde(default)]
    cultures: HashMap<String, CultureWords>,
}

impl PlaceSchema {
    /// A unique "<stem> <suffix>" for `culture`, rolled from the RNG; falls back
    /// to numbering if the (small) combination space is exhausted. Culture words
    /// override the defaults; an empty culture list keeps the default for that
    /// part, so a desert market can swap suffixes (Souk/Bazaar) but reuse stems.
    fn pick(&self, culture: &str, rng: &mut RNG, used: &mut HashSet<String>) -> Option<String> {
        let cw = self.cultures.get(culture);
        let stems = match cw {
            Some(c) if !c.stems.is_empty() => &c.stems,
            _ => &self.stems,
        };
        let suffixes = match cw {
            Some(c) if !c.suffixes.is_empty() => &c.suffixes,
            _ => &self.suffixes,
        };
        if stems.is_empty() || suffixes.is_empty() {
            return None;
        }
        let one = |rng: &mut RNG, v: &[String]| -> String {
            v[(rng.rand_i32_range(0, v.len() as i32) as usize) % v.len()].clone()
        };
        for _ in 0..24 {
            let cand = format!("{} {}", one(rng, stems), one(rng, suffixes));
            if used.insert(cand.clone()) {
                return Some(cand);
            }
        }
        let base = format!("{} {}", stems[0], suffixes[0]);
        let mut i = 2;
        loop {
            let cand = format!("{base} {i}");
            if used.insert(cand.clone()) {
                return Some(cand);
            }
            i += 1;
        }
    }
}

/// Lowercase culture key used to look up per-culture word overrides in the YAML.
fn culture_key(culture: crate::generator::buildings_v2::Culture) -> &'static str {
    use crate::generator::buildings_v2::Culture;
    match culture {
        Culture::Desert => "desert",
        Culture::Japanese => "japanese",
        Culture::Medieval => "medieval",
    }
}

/// Open-space naming vocabulary, from `data/open_space_names.yaml`. Both plazas
/// and parks have a schema PER type (keyed by `PlazaType::key` / `ParkType::key`),
/// so a space is named for what it actually is — `Fountain Square`, `Corn Market`,
/// `Willow Cemetery`, `Heron Pond`. De-duplicate names with a shared `used` set.
#[derive(Debug, Deserialize)]
pub struct OpenSpaceNames {
    plazas: HashMap<String, PlaceSchema>,
    parks: HashMap<String, PlaceSchema>,
}

impl OpenSpaceNames {
    pub fn load() -> Option<Self> {
        match load_yaml("open_space_names.yaml") {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("open_space_names.yaml failed to load ({e}); open spaces stay unnamed");
                None
            }
        }
    }

    /// Name a plaza by its built [`PlazaType`], in the settlement's culture.
    pub fn name_plaza(
        &self,
        plaza_type: PlazaType,
        culture: crate::generator::buildings_v2::Culture,
        rng: &mut RNG,
        used: &mut HashSet<String>,
    ) -> Option<String> {
        self.plazas.get(plaza_type.key())?.pick(culture_key(culture), rng, used)
    }

    /// Name a park by its built [`ParkType`], in the settlement's culture.
    pub fn name_park(
        &self,
        park_type: ParkType,
        culture: crate::generator::buildings_v2::Culture,
        rng: &mut RNG,
        used: &mut HashSet<String>,
    ) -> Option<String> {
        self.parks.get(park_type.key())?.pick(culture_key(culture), rng, used)
    }
}

/// A green cell is an urban cell that nothing has claimed (or only `Nature`).
fn is_green(world: &World, c: Point2D) -> bool {
    matches!(
        world.get_claim(c),
        None | Some(BuildClaim::None) | Some(BuildClaim::Nature)
    )
}

/// Minimum width (cells) a strip must have to survive the thin-strip cull. A
/// cell sitting in a contiguous run of green cells narrower than this — in
/// either the X or Z direction — is dropped (e.g. `XPPX` → both `P`s culled).
/// Thin corridors/tendrils are too narrow to host any real open-space feature.
const MIN_STRIP_WIDTH: usize = 3;

/// Length of the contiguous run of green cells through `c` along `step`
/// (counting `c` itself), e.g. step `(1, 0)` measures the horizontal run.
fn run_len(green: &HashSet<Point2D>, c: Point2D, step: Point2D) -> usize {
    let mut len = 1;
    let mut p = c + step;
    while green.contains(&p) {
        len += 1;
        p = p + step;
    }
    p = c - step;
    while green.contains(&p) {
        len += 1;
        p = p - step;
    }
    len
}

/// Drop every cell that lies in a run narrower than [`MIN_STRIP_WIDTH`] along
/// either axis, leaving only the "fat" parts of the open space. Repeats until
/// stable, since culling a thin edge can expose a fresh thin sliver on what
/// remains.
fn cull_thin(green: &HashSet<Point2D>) -> HashSet<Point2D> {
    let mut set = green.clone();
    loop {
        let next: HashSet<Point2D> = set
            .iter()
            .copied()
            .filter(|&c| {
                run_len(&set, c, Point2D::new(1, 0)) >= MIN_STRIP_WIDTH
                    && run_len(&set, c, Point2D::new(0, 1)) >= MIN_STRIP_WIDTH
            })
            .collect();
        if next.len() == set.len() {
            return next;
        }
        set = next;
    }
}

/// Minimum distance-from-boundary (cells) for a cell to seed a region "core".
/// A cell this deep sits in something at least `2*CORE_MIN_DIST + 1` wide, so
/// fat lobes seed cores while necks ≤ `2*CORE_MIN_DIST` wide do not — those
/// become the watershed cuts that split sprawling regions apart.
const CORE_MIN_DIST: i32 = 2;

/// Flood the cells reachable from `start` within `allowed` that aren't already
/// labelled, tagging them `id` in `label`.
fn flood_label(
    start: Point2D,
    id: usize,
    allowed: &HashSet<Point2D>,
    label: &mut HashMap<Point2D, usize>,
) {
    let mut queue = VecDeque::new();
    queue.push_back(start);
    label.insert(start, id);
    while let Some(c) = queue.pop_front() {
        for d in CARDINALS_2D {
            let n = c + d;
            if allowed.contains(&n) && !label.contains_key(&n) {
                label.insert(n, id);
                queue.push_back(n);
            }
        }
    }
}

/// Split the green cells into regions by morphological opening + watershed:
/// erode to fat "cores", label each core blob, then grow every cell to its
/// nearest core. Sprawling shapes split at their necks; coreless components
/// (uniformly thin blobs) stay whole.
fn partition_cells(green: &HashSet<Point2D>) -> Vec<Vec<Point2D>> {
    // Multi-source BFS distance of every green cell from the nearest non-green
    // boundary (boundary-adjacent cells are 0) — the canonical impl in `props`.
    let dist = props::edge_depth(green);

    // Cores: cells deep enough to be a lobe centre.
    let core: HashSet<Point2D> = green
        .iter()
        .copied()
        .filter(|c| dist.get(c).copied().unwrap_or(0) >= CORE_MIN_DIST)
        .collect();

    // Label each connected core blob.
    let mut label: HashMap<Point2D, usize> = HashMap::new();
    let mut next_id = 0;
    for &c in &core {
        if !label.contains_key(&c) {
            flood_label(c, next_id, &core, &mut label);
            next_id += 1;
        }
    }

    // Watershed: grow cores outward over all green cells, nearest core wins.
    // Seed in sorted order so ties at neck midlines resolve deterministically.
    let mut seeds: Vec<Point2D> = label.keys().copied().collect();
    seeds.sort_by_key(|p| (p.x, p.y));
    let mut queue: VecDeque<Point2D> = seeds.into_iter().collect();
    while let Some(c) = queue.pop_front() {
        let id = label[&c];
        for d in CARDINALS_2D {
            let n = c + d;
            if green.contains(&n) && !label.contains_key(&n) {
                label.insert(n, id);
                queue.push_back(n);
            }
        }
    }

    // Coreless components (no fat centre anywhere) stay whole.
    for &c in green {
        if !label.contains_key(&c) {
            flood_label(c, next_id, green, &mut label);
            next_id += 1;
        }
    }

    let mut groups: Vec<Vec<Point2D>> = vec![Vec::new(); next_id];
    for (&c, &id) in &label {
        groups[id].push(c);
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// Detect the open-space regions: erode thin strips, split sprawling shapes at
/// their necks, and classify each resulting region.
pub fn detect_regions(world: &World, urban: &HashSet<Point2D>) -> Vec<Region> {
    let green: HashSet<Point2D> = urban
        .iter()
        .copied()
        .filter(|&c| is_green(world, c))
        .collect();
    let green = cull_thin(&green);

    let mut regions = Vec::new();
    for cells in partition_cells(&green) {
        // Edge if any cell abuts the city's outer extent — a wall/gate cell or a
        // cell outside the urban area. Neighbours in other regions don't count.
        let touches_edge = cells.iter().any(|&c| {
            CARDINALS_2D.iter().any(|d| {
                let n = c + *d;
                !urban.contains(&n)
                    || matches!(world.get_claim(n), Some(BuildClaim::Wall | BuildClaim::Gate))
            })
        });
        let area = cells.len();
        let kind = if touches_edge {
            RegionKind::Edge
        } else {
            RegionKind::Interior
        };
        let large = area >= LARGE_MIN_AREA;
        regions.push(Region { cells, area, kind, large });
    }

    regions
}

