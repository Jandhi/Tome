//! House plots: how much ground a town house owns, graded by how far it stands
//! from the town centre.
//!
//! The urban area is split by distance from its centroid ([`RingMap`]): the
//! nearest 40% of its area is the core, the next 35% the middle ring, the rest
//! the outer ring. The core packs terraced rows on small lots; the further out,
//! the larger the lots, the wider the gap between neighbours and the deeper
//! the ground each house is given ([`Ring`]: 24/32/40-cell lots, 0/1/2-cell
//! gaps, 3/7/10 deep). Lots are graded per piece as blocks are cut
//! (`districts::subdivide::subdivide_block_graded`).
//!
//! Settlement placement reserves a house's ground behind it outside the core,
//! and asks for a back door where there is ground behind
//! (`BuildingContext::back_door`; in the core only with two free cells behind).
//! After the houses are placed, [`plan_gardens`] hands every house the strip
//! of its lot behind it (plus its side gap), stepping over door ramps
//! ([`door_cells`]); outer-ring yards also take the lot's leftover ground, and
//! a one-cell strip of free ground between two gardens goes to the
//! lower-numbered one. The settlement claims each garden under the house's own
//! building id (`World::new_building_id`); what is left goes to the open-space
//! pass, so the core and middle ring keep their plazas and nooks.
//! `TOME_DEBUG_GARDENS=1` dumps the gardens and density data to `output/`.

use crate::util::{HashMap, HashSet};

use crate::editor::{Editor, World};
use crate::generator::buildings::BuildingID;
use crate::generator::buildings_v2::HouseOutput;
use crate::generator::BuildClaim;
use crate::geometry::{Cardinal, Point2D, Point3D, CARDINALS_2D};

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

/// Cell -> index into `gardens`, for telling neighbouring gardens apart.
pub fn garden_owners(gardens: &[Garden]) -> HashMap<Point2D, usize> {
    let mut m = HashMap::default();
    for (i, g) in gardens.iter().enumerate() {
        for &c in &g.cells {
            m.insert(c, i);
        }
    }
    m
}

/// Gardens smaller than this get no fence.
pub const FENCE_MIN_AREA: usize = 8;
/// A fenced garden keeps at least this many cells inside its fence...
const FENCE_MIN_INSIDE: usize = 6;
/// ...spread over at least this many rows back from its house.
const FENCE_MIN_DEPTH: usize = 2;

/// Every garden's fence posts, planned together so they join up. Only gardens
/// with room left inside a fence get one (slivers stay open). A garden fences
/// each of its cells on an edge that isn't its own house wall; where two
/// fenced gardens meet there is one line: the lower-numbered one fences,
/// unless the other's cell there is already a post. Fences join only north,
/// south, east and west, so a diagonal step (within a garden or where two
/// gardens' lines meet) gets a post on a garden cell between. Then posts in
/// front of a door (any of `houses`' steps or entrances, or any placed door
/// block beside the cell), corners of 2x2 squares and posts boxing in a pocket
/// of one or two cells are removed, and any post left standing alone, or
/// sticking out one block from a line, is dropped.
pub fn plan_fences(editor: &Editor, gardens: &[Garden], owners: &HashMap<Point2D, usize>, houses: &[PlacedHouse]) -> HashSet<Point2D> {
    let world = editor.world();
    let own_wall = |gi: usize, c: Point2D| {
        !owners.contains_key(&c) && world.get_claim(c) == Some(BuildClaim::Building(gardens[gi].building))
    };
    let inside = |gi: usize, c: Point2D| owners.get(&c) == Some(&gi) || own_wall(gi, c);
    // Which gardens get a fence: enough cells that no fence could stand on
    // (every neighbour its own), over at least two rows.
    let eligible: Vec<bool> = gardens
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            if g.cells.len() < FENCE_MIN_AREA {
                return false;
            }
            let core: Vec<Point2D> =
                g.cells.iter().copied().filter(|&c| CARDINALS_2D.iter().all(|d| inside(gi, c + *d))).collect();
            let rows: HashSet<i32> = core.iter().map(|c| c.x * g.into.x + c.y * g.into.y).collect();
            core.len() >= FENCE_MIN_INSIDE && rows.len() >= FENCE_MIN_DEPTH
        })
        .collect();
    let fenced_garden = |c: &Point2D| owners.get(c).is_some_and(|&g| eligible[g]);
    let mut fences: HashSet<Point2D> = HashSet::default();
    // Edges onto open ground (or an unfenced garden) first, then edges between
    // fenced gardens, highest-numbered garden first: by the time a garden
    // fences a shared edge, the far side's posts are final, so it can skip
    // the cells already fenced from there.
    let open = |c: Point2D| match owners.get(&c) {
        Some(&o) => !eligible[o],
        None => !matches!(world.get_claim(c), Some(BuildClaim::Building(_) | BuildClaim::Structure(_))),
    };
    // Against an unfenced garden the line goes on that garden's edge cell,
    // so a sliver between two fenced gardens becomes their one shared line.
    for (gi, g) in gardens.iter().enumerate().filter(|(gi, _)| eligible[*gi]) {
        for &c in &g.cells {
            for d in CARDINALS_2D {
                let n = c + d;
                if inside(gi, n) || !open(n) {
                    continue;
                }
                fences.insert(if owners.contains_key(&n) { n } else { c });
            }
        }
    }
    for (gi, g) in gardens.iter().enumerate().rev().filter(|(gi, _)| eligible[*gi]) {
        for &c in &g.cells {
            let shared = CARDINALS_2D.iter().any(|d| {
                let n = c + *d;
                owners.get(&n).is_some_and(|&o| o != gi && eligible[o] && gi < o) && !fences.contains(&n)
            });
            if shared {
                fences.insert(c);
            }
        }
    }
    // A wall a fence can end against: a building or structure, not a garden.
    let wall = |c: Point2D| {
        !owners.contains_key(&c) && matches!(world.get_claim(c), Some(BuildClaim::Building(_) | BuildClaim::Structure(_)))
    };
    let fenced = fenced_garden;
    // Close diagonal steps.
    let mut sorted: Vec<Point2D> = fences.iter().copied().collect();
    sorted.sort_by_key(|c| (c.x, c.y));
    for c in sorted {
        for (dx, dz) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            if !fences.contains(&(c + Point2D::new(dx, dz))) {
                continue;
            }
            let (a, b) = (c + Point2D::new(dx, 0), c + Point2D::new(0, dz));
            if fences.contains(&a) || fences.contains(&b) || wall(a) || wall(b) {
                continue;
            }
            if let Some(p) = [a, b].into_iter().find(|p| fenced(p)) {
                fences.insert(p);
            }
        }
    }
    // A line that stops just short of a wall or another fence, across another
    // garden's cells that aren't fenced there, carries on to meet it; one that
    // ends beside a house corner diagonally ahead steps sideways onto it.
    let mut sorted: Vec<Point2D> = fences.iter().copied().collect();
    sorted.sort_by_key(|c| (c.x, c.y));
    for c in sorted {
        let joined: Vec<Point2D> = CARDINALS_2D.iter().copied().filter(|d| fences.contains(&(c + *d)) || wall(c + *d)).collect();
        if joined.len() != 1 {
            continue;
        }
        let ahead = Point2D::new(-joined[0].x, -joined[0].y);
        // A house corner diagonally ahead: one post beside the end joins it.
        let side = [Point2D::new(ahead.y, ahead.x), Point2D::new(-ahead.y, -ahead.x)]
            .into_iter()
            .find(|s| wall(c + ahead + *s) && fenced(&(c + *s)) && !fences.contains(&(c + *s)));
        if let Some(s) = side {
            fences.insert(c + s);
            continue;
        }
        let mut run: Vec<Point2D> = Vec::new();
        let mut p = c + ahead;
        for _ in 0..3 {
            if fences.contains(&p) || wall(p) {
                fences.extend(run.iter().copied());
                break;
            }
            if !fenced(&p) {
                break;
            }
            run.push(p);
            p = p + ahead;
        }
    }
    let links = |f: &HashSet<Point2D>, c: Point2D| {
        CARDINALS_2D.iter().filter(|d| f.contains(&(c + **d)) || wall(c + **d)).count()
    };
    // Keep doors clear: no post on a door's steps or entrance, or beside any
    // placed door block (towers and structures too).
    let steps: HashSet<Point2D> = houses.iter().flat_map(|h| h.ramp_cells.iter().chain(&h.entrances).copied()).collect();
    let by_door = |c: Point2D| {
        steps.contains(&c)
            || world.get_ocean_floor_height_at(c).is_some_and(|h| {
                CARDINALS_2D.iter().any(|d| {
                    let n = c + *d;
                    (h - 2..=h + 3).any(|y| {
                        editor.get_cached_block(Point3D::new(n.x, y, n.y)).is_some_and(|b| {
                            let id = b.id.as_str();
                            id.ends_with("_door") && !id.ends_with("trapdoor")
                        })
                    })
                })
            })
    };
    let mut sorted: Vec<Point2D> = fences.iter().copied().collect();
    sorted.sort_by_key(|c| (c.x, c.y));
    for c in sorted {
        if by_door(c) {
            fences.remove(&c);
        }
    }
    // No two lines one cell apart: a strip of garden one cell wide between
    // posts (or a wall), two or more cells long, becomes a single line down
    // its middle; side posts that only ran along it go.
    let blocked = |f: &HashSet<Point2D>, c: Point2D| f.contains(&c) || wall(c);
    let neck = |f: &HashSet<Point2D>, c: Point2D, a: Point2D| {
        !blocked(f, c) && fenced(&c) && blocked(f, c + a) && blocked(f, c - a)
    };
    // Converting a strip can leave another beside it: repeat a few times.
    for _ in 0..3 {
        let mut necks: Vec<(Point2D, Point2D)> = Vec::new();
        for (&c, _) in owners.iter() {
            for a in [Point2D::new(1, 0), Point2D::new(0, 1)] {
                let along = Point2D::new(a.y, a.x);
                if neck(&fences, c, a) && (neck(&fences, c + along, a) || neck(&fences, c - along, a)) && !by_door(c) {
                    necks.push((c, a));
                }
            }
        }
        necks.sort_by_key(|(c, a)| (c.x, c.y, a.x));
        if necks.is_empty() {
            break;
        }
        let before = fences.clone();
        for &(c, a) in &necks {
            fences.insert(c);
            for side in [c + a, c - a] {
                if before.contains(&side) && !blocked(&before, side + (side - c)) {
                    fences.remove(&side);
                }
            }
        }
    }
    // No 2x2 squares of posts: drop the corner with the fewest links outside
    // the square, then the one leaving the fewest new line ends.
    let mut sorted: Vec<Point2D> = fences.iter().copied().collect();
    sorted.sort_by_key(|c| (c.x, c.y));
    for c in sorted {
        let square = [c, c + Point2D::new(1, 0), c + Point2D::new(0, 1), c + Point2D::new(1, 1)];
        if !square.iter().all(|p| fences.contains(p)) {
            continue;
        }
        let cost = |p: Point2D| {
            let new_ends = CARDINALS_2D.iter().filter(|d| fences.contains(&(p + **d)) && links(&fences, p + **d) <= 2).count();
            (links(&fences, p) - 2) * 10 + new_ends
        };
        if let Some(&p) = square.iter().min_by_key(|p| cost(**p)) {
            fences.remove(&p);
        }
    }
    // No post boxed in on all four sides (a plain crossing of two lines
    // stays), and no pocket of one or two cells shut in by posts and walls:
    // open each through the post with the fewest links.
    for _ in 0..8 {
        let mut sorted: Vec<Point2D> = fences.iter().copied().collect();
        sorted.sort_by_key(|c| (c.x, c.y));
        let mut changed = false;
        for &c in &sorted {
            if !fences.contains(&c) || links(&fences, c) < 4 {
                continue;
            }
            let cross = CARDINALS_2D.iter().all(|d| fences.contains(&(c + *d)))
                && [(1, 1), (1, -1), (-1, 1), (-1, -1)].iter().all(|&(dx, dz)| !blocked(&fences, c + Point2D::new(dx, dz)));
            if !cross {
                fences.remove(&c);
                changed = true;
            }
        }
        for &c in &sorted {
            if !fences.contains(&c) {
                continue;
            }
            for d in CARDINALS_2D {
                let start = c + d;
                if blocked(&fences, start) {
                    continue;
                }
                let mut pocket = vec![start];
                let mut i = 0;
                while i < pocket.len() && pocket.len() <= 2 {
                    for e in CARDINALS_2D {
                        let n = pocket[i] + e;
                        if !blocked(&fences, n) && !pocket.contains(&n) {
                            pocket.push(n);
                        }
                    }
                    i += 1;
                }
                if pocket.len() > 2 {
                    continue;
                }
                // Posts beside the pocket that open onto ground outside it.
                let mut doors: Vec<Point2D> = pocket
                    .iter()
                    .flat_map(|&p| CARDINALS_2D.iter().map(move |e| p + *e))
                    .filter(|p| fences.contains(p))
                    .filter(|p| CARDINALS_2D.iter().any(|e| !blocked(&fences, *p + *e) && !pocket.contains(&(*p + *e))))
                    .collect();
                doors.sort_by_key(|p| (links(&fences, *p), p.x, p.y));
                if let Some(&p) = doors.first() {
                    fences.remove(&p);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Drop lone posts and one-block spurs off a line (a line's own end, where
    // its only neighbour carries on, stays).
    for _ in 0..2 {
        let mut drop: Vec<Point2D> = Vec::new();
        for &c in &fences {
            match links(&fences, c) {
                0 => drop.push(c),
                1 => {
                    let n = CARDINALS_2D.iter().map(|d| c + *d).find(|n| fences.contains(n));
                    if n.is_some_and(|n| links(&fences, n) >= 3) {
                        drop.push(c);
                    }
                }
                _ => {}
            }
        }
        if drop.is_empty() {
            break;
        }
        for c in drop {
            fences.remove(&c);
        }
    }
    fences
}
