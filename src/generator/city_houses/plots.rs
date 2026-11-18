//! House plots: how much ground a town house owns, graded by how far it stands
//! from the town centre.
//!
//! The core packs terraced rows on small lots; the further out a block lies, the
//! larger its lots, the wider the gap between neighbours and the deeper the
//! back garden each house is given ([`Ring`]). After the houses are placed,
//! [`plan_gardens`] hands every house the strip of its lot behind it (plus
//! its side gap) as a garden.

use crate::util::{HashMap, HashSet};

use crate::editor::World;
use crate::generator::buildings::BuildingID;
use crate::generator::buildings_v2::HouseOutput;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D, CARDINALS_2D};

/// How far a block lies from the town centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    Core,
    Middle,
    Outer,
}

impl Ring {
    /// Largest side of an interior lot before it is cut again by an alley.
    pub fn max_lot_dim(self) -> i32 {
        match self {
            Ring::Core => 24,
            Ring::Middle => 32,
            Ring::Outer => 40,
        }
    }

    /// Depth of the lot strip kept along a main road.
    pub fn ribbon_depth(self) -> i32 {
        match self {
            Ring::Core => 14,
            Ring::Middle => 18,
            Ring::Outer => 22,
        }
    }

    /// Cells left open between neighbouring houses on one frontage.
    pub fn side_gap(self) -> i32 {
        match self {
            Ring::Core => 0,
            Ring::Middle => 1,
            Ring::Outer => 2,
        }
    }

    /// Deepest back garden a house is given behind its back wall.
    pub fn garden_depth(self) -> i32 {
        match self {
            Ring::Core => 3,
            Ring::Middle => 7,
            Ring::Outer => 10,
        }
    }
}

/// Grades ground into [`Ring`]s by distance from the urban centroid: the
/// nearest 40% of the town's area is the core, the next 35% the middle ring,
/// the rest the outer ring. A set of cells is graded by its centroid.
pub struct RingMap {
    centre: Point2D,
    core: f32,
    middle: f32,
}

impl RingMap {
    pub fn new(urban: &HashSet<Point2D>) -> Self {
        let centre = centroid(urban);
        let mut d: Vec<f32> = urban.iter().map(|c| dist(*c, centre)).collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let at = |q: usize| d.get(d.len() * q / 100).copied().unwrap_or(0.0);
        Self { centre, core: at(40), middle: at(75) }
    }

    pub fn ring_of(&self, cells: &HashSet<Point2D>) -> Ring {
        let r = dist(centroid(cells), self.centre);
        if r < self.core {
            Ring::Core
        } else if r < self.middle {
            Ring::Middle
        } else {
            Ring::Outer
        }
    }
}

fn centroid(cells: &HashSet<Point2D>) -> Point2D {
    let n = cells.len().max(1) as i64;
    let (sx, sz) = cells.iter().fold((0i64, 0i64), |(ax, az), c| (ax + c.x as i64, az + c.y as i64));
    Point2D::new((sx / n) as i32, (sz / n) as i32)
}

fn dist(a: Point2D, b: Point2D) -> f32 {
    (((a.x - b.x) as f32).powi(2) + ((a.y - b.y) as f32).powi(2)).sqrt()
}

/// A town house as placed, enough to find the ground behind it.
#[derive(Debug, Clone)]
pub struct PlacedHouse {
    /// Frontage cells the house stands on, then its side-gap cells.
    pub front: Vec<Point2D>,
    /// How many of `front` the house itself covers (the rest is its side gap).
    pub width: usize,
    /// Depth of the house's core rect, away from the road.
    pub depth: i32,
    /// Side of the house that faces the road.
    pub outward: Cardinal,
    /// The id the house's cells are claimed under.
    pub building: BuildingID,
    pub ring: Ring,
    pub lot: usize,
    /// Cells the house's door steps stand on (see [`door_cells`]): no garden
    /// takes them, so its soil never buries a step.
    pub ramp_cells: Vec<Point2D>,
    /// Where each ground-floor door lets out onto the ground: a garden path
    /// starts from the one that opens into the garden.
    pub entrances: Vec<Point2D>,
}

/// A built house's door cells: `(ramp_cells, entrances)`. Ramp cells are every
/// cell a door's landing, platform or steps stand on where the ground did not
/// meet the sill; entrances are where each door lets out at ground level.
pub fn door_cells(out: &HouseOutput) -> (Vec<Point2D>, Vec<Point2D>) {
    let mut ramp = Vec::new();
    for r in &out.door_ramps {
        let pair = Point2D::from(r.wide_dir);
        let mut add = |c: Point2D| {
            ramp.push(c);
            if r.width == 2 {
                ramp.push(c + pair);
            }
        };
        add(r.landing_cell);
        if let Some(p) = &r.platform_extension {
            add(p.cell);
        }
        for s in &r.steps {
            add(s.cell);
        }
    }
    (ramp, out.door_entrances.clone())
}

/// One house's garden: the cells it owns behind and beside the house.
#[derive(Debug, Clone)]
pub struct Garden {
    pub house: usize,
    pub building: BuildingID,
    pub ring: Ring,
    pub cells: Vec<Point2D>,
    /// Unit step away from the road, into the lot.
    pub into: Point2D,
    /// Side-gap cells that touch the road: where the garden gate goes.
    pub gates: Vec<Point2D>,
    /// The house's door entrances (see [`PlacedHouse::entrances`]).
    pub doors: Vec<Point2D>,
}

/// Smallest garden worth keeping; smaller scraps stay unclaimed.
const MIN_GARDEN: usize = 3;

/// Give each house the free cells of its lot behind it, column by column from
/// its frontage, up to its ring's garden depth. Where another house stands at
/// the far end of a column, the run is split so each gets its half.
pub fn plan_gardens(world: &World, houses: &[PlacedHouse], lots: &[HashSet<Point2D>]) -> Vec<Garden> {
    let steps: HashSet<Point2D> = houses.iter().flat_map(|h| h.ramp_cells.iter().copied()).collect();
    let free = |c: Point2D| !steps.contains(&c) && matches!(world.get_claim(c), Some(BuildClaim::None | BuildClaim::Nature));
    let mut taken: HashSet<Point2D> = HashSet::default();
    let mut gardens = Vec::new();
    for (hi, h) in houses.iter().enumerate() {
        let lot = &lots[h.lot];
        let road = Point2D::from(h.outward);
        let into = Point2D::new(-road.x, -road.y);
        let own = |c: Point2D| world.get_claim(c) == Some(BuildClaim::Building(h.building));
        let cap = h.ring.garden_depth();
        let mut cells: Vec<Point2D> = Vec::new();
        let mut gates: Vec<Point2D> = Vec::new();
        for (i, &f) in h.front.iter().enumerate() {
            let gap_column = i >= h.width;
            let mut beside: Vec<Point2D> = Vec::new();
            let mut behind: Vec<Point2D> = Vec::new();
            let mut hit_building = false;
            // A house column first crosses any verge in front of the house
            // (diagonal roads seat houses back from the frontage), then the
            // house; only what lies past it is garden.
            let mut met_house = gap_column;
            let mut s = 0;
            loop {
                let c = f + into * s;
                s += 1;
                if !lot.contains(&c) || taken.contains(&c) {
                    break;
                }
                if own(c) {
                    met_house = true;
                    continue;
                }
                if steps.contains(&c) {
                    // Door steps behind the house: step over them.
                    continue;
                }
                if !met_house {
                    if s > 16 {
                        break;
                    }
                    continue;
                }
                if !free(c) {
                    hit_building = matches!(world.get_claim(c), Some(BuildClaim::Building(_)));
                    break;
                }
                if gap_column && s - 1 < h.depth {
                    // Level with the house: the side gap.
                    beside.push(c);
                } else {
                    behind.push(c);
                }
            }
            // Facing a house across the lot, split the gap with it; the outer
            // ring takes its whole half, so no strip is left between yards.
            let keep = match (hit_building, h.ring) {
                (true, Ring::Outer) => (behind.len() / 2).min(2 * cap as usize),
                (true, _) => (behind.len() / 2).min(cap as usize),
                (false, _) => behind.len().min(cap as usize),
            };
            behind.truncate(keep);
            if gap_column && beside.first() == Some(&f) {
                gates.push(f);
            }
            cells.extend(beside);
            cells.extend(behind);
        }
        if cells.len() < MIN_GARDEN {
            continue;
        }
        taken.extend(cells.iter().copied());
        gardens.push(Garden { house: hi, building: h.building, ring: h.ring, cells, into, gates, doors: h.entrances.clone() });
    }

    // In the outer ring, yards grow into the lot's leftover ground (a
    // breadth-first flood from every yard at once, so each takes what is
    // nearest): out there the open middle of a lot is yards, not a plaza.
    // Nearer the centre what is left stays for squares and nooks.
    let mut queue: std::collections::VecDeque<(Point2D, usize, i32)> = Default::default();
    for (gi, g) in gardens.iter().enumerate() {
        if g.ring != Ring::Outer {
            continue;
        }
        for &c in &g.cells {
            queue.push_back((c, gi, 0));
        }
    }
    let mut grown: Vec<Vec<Point2D>> = vec![Vec::new(); gardens.len()];
    while let Some((c, gi, d)) = queue.pop_front() {
        let g = &gardens[gi];
        if d >= g.ring.garden_depth() {
            continue;
        }
        let lot = &lots[houses[g.house].lot];
        for dir in CARDINALS_2D {
            let n = c + dir;
            if taken.contains(&n) || !lot.contains(&n) || !free(n) {
                continue;
            }
            taken.insert(n);
            grown[gi].push(n);
            queue.push_back((n, gi, d + 1));
        }
    }
    for (g, extra) in gardens.iter_mut().zip(grown) {
        g.cells.extend(extra);
    }

    // A one-cell strip of free ground left between two gardens (or a notch
    // in one) goes to the lower-numbered of them: otherwise each fences its
    // own side and two fence lines run a block apart.
    let mut owner: HashMap<Point2D, usize> = HashMap::default();
    for (gi, g) in gardens.iter().enumerate() {
        for &c in &g.cells {
            owner.insert(c, gi);
        }
    }
    let mut strip: Vec<Point2D> = gardens
        .iter()
        .flat_map(|g| g.cells.iter().flat_map(|&c| CARDINALS_2D.iter().map(move |d| c + *d)))
        .filter(|n| !owner.contains_key(n) && !taken.contains(n) && free(*n))
        .collect();
    strip.sort_by_key(|c| (c.x, c.y));
    strip.dedup();
    let mut extra: Vec<(Point2D, usize)> = Vec::new();
    for c in strip {
        let across = [Point2D::new(1, 0), Point2D::new(0, 1)].iter().find_map(|&d| {
            match (owner.get(&(c + d)), owner.get(&(c - d))) {
                (Some(&a), Some(&b)) => Some(a.min(b)),
                _ => None,
            }
        });
        if let Some(gi) = across {
            extra.push((c, gi));
        }
    }
    for (c, gi) in extra {
        gardens[gi].cells.push(c);
    }
    gardens
}
