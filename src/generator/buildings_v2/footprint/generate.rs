use std::collections::HashMap;
use crate::geometry::{Point2D, Rect2D};
use crate::noise::RNG;
use super::{SizeClass, maximal_rect::find_largest_rect, Plot};

/// The four sides of a rectangle that a wing can attach to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    North, // -z edge
    South, // +z edge
    West,  // -x edge
    East,  // +x edge
}

const ALL_SIDES: [Side; 4] = [Side::North, Side::South, Side::West, Side::East];

/// Minimum side length for any wing (smaller than core min_side).
const MIN_WING_SIDE: i32 = 5;

/// Snaps a value to the nearest odd number, preferring to round up.
fn snap_odd(value: i32) -> i32 {
    if value % 2 == 0 { value + 1 } else { value }
}

/// Finds the candidate area (largest usable rectangle) in the plot,
/// converted to world coordinates. Returns None if too small for the size class.
fn find_candidate(plot: &Plot, size_class: &SizeClass) -> Option<Rect2D> {
    let rect = find_largest_rect(&plot.usable)?;
    let min = plot.bounds.min();
    let candidate = Rect2D::new(
        Point2D::new(min.x + rect.origin.x, min.y + rect.origin.y),
        rect.size,
    );
    if candidate.length() < size_class.min_side || candidate.width() < size_class.min_side {
        return None;
    }
    Some(candidate)
}

/// Generates a single core rectangle candidate within the candidate area.
fn generate_core(
    rng: &mut RNG,
    candidate: &Rect2D,
    size_class: &SizeClass,
    target_area: i32,
) -> Option<Rect2D> {
    // Core takes less of the target area when more wings are expected
    // 0 wings: 100%, 1 wing: 50-65%, 2 wings: 40-55%, 3 wings: 35-50%
    let core_fraction = if size_class.max_wings == 0 {
        100
    } else {
        let max_frac = 65 - (size_class.max_wings - 1) * 10;
        let min_frac = max_frac - 15;
        rng.rand_i32_range(min_frac, max_frac + 1)
    };
    let core_area = target_area * core_fraction / 100;

    let ratio = rng.rand_i32_range(100, 201) as f32 / 100.0;

    let width_f = (core_area as f32 * ratio).sqrt();
    let depth_f = (core_area as f32 / ratio).sqrt();

    let mut width = snap_odd(width_f as i32).max(size_class.min_side);
    let mut depth = snap_odd(depth_f as i32).max(size_class.min_side);

    if rng.percent(50) {
        std::mem::swap(&mut width, &mut depth);
    }

    width = width.min(candidate.length());
    depth = depth.min(candidate.width());

    if width < size_class.min_side || depth < size_class.min_side {
        return None;
    }

    let max_x = candidate.min().x + candidate.length() - width;
    let max_z = candidate.min().y + candidate.width() - depth;

    if max_x < candidate.min().x || max_z < candidate.min().y {
        return None;
    }

    let x = if max_x == candidate.min().x {
        candidate.min().x
    } else {
        rng.rand_i32_range(candidate.min().x, max_x + 1)
    };
    let z = if max_z == candidate.min().y {
        candidate.min().y
    } else {
        rng.rand_i32_range(candidate.min().y, max_z + 1)
    };

    Some(Rect2D::new(Point2D::new(x, z), Point2D::new(width, depth)))
}

/// An occupied span along a core edge (start..end inclusive, relative to edge start).
#[derive(Debug, Clone, Copy)]
struct Span {
    start: i32,
    end: i32,
}

/// Finds gaps along an edge of length `edge_len` given existing occupied spans.
/// Returns a list of (gap_start, gap_len) pairs where a wing could fit.
fn find_gaps(edge_len: i32, spans: &[Span], min_gap: i32) -> Vec<(i32, i32)> {
    let mut gaps = Vec::new();
    let mut pos = 0;

    // Sort spans by start
    let mut sorted: Vec<Span> = spans.to_vec();
    sorted.sort_by_key(|s| s.start);

    for span in &sorted {
        let gap = span.start - pos;
        if gap >= min_gap {
            gaps.push((pos, gap));
        }
        pos = span.end + 1;
    }

    // Trailing gap
    let gap = edge_len - pos;
    if gap >= min_gap {
        gaps.push((pos, gap));
    }

    gaps
}

/// Attempts to attach a wing to a specific side of the core.
/// `occupied` contains spans already taken on this side.
/// Returns the wing rect and the span it occupies, or None.
fn attach_wing(
    rng: &mut RNG,
    core: &Rect2D,
    side: Side,
    candidate: &Rect2D,
    remaining_area: i32,
    min_side: i32,
    core_area: i32,
    wings_left: i32,
    occupied: &[Span],
) -> Option<(Rect2D, Span)> {
    // Wing area: 30-70% of core area, but at least min_side^2 if budget allows.
    let max_budget = if wings_left > 1 {
        remaining_area * 2 / 3
    } else {
        remaining_area
    };
    let min_wing_area = min_side * min_side;
    let target_wing_area = core_area * rng.rand_i32_range(30, 71) / 100;
    let wing_area = target_wing_area.max(min_wing_area).min(max_budget);
    if wing_area < min_wing_area {
        return None;
    }

    // The edge length of the core along this side
    let edge_len = match side {
        Side::North | Side::South => core.length(),
        Side::West | Side::East => core.width(),
    };

    // Find available gaps on this edge
    let gaps = find_gaps(edge_len, occupied, min_side);
    if gaps.is_empty() {
        return None;
    }

    // Pick a random gap
    let &(gap_start, gap_len) = rng.choose(&gaps);

    // Wing length: fit within the gap, capped at 80% of full edge to keep a notch
    let max_along = gap_len.min(edge_len * 4 / 5);
    if max_along < min_side {
        return None;
    }
    let wing_along = rng.rand_i32_range(min_side, max_along + 1);
    let wing_along = snap_odd(wing_along).min(gap_len);
    if wing_along < min_side {
        return None;
    }

    // Wing depth perpendicular to the edge
    let wing_perp = snap_odd(wing_area / wing_along).max(min_side);
    if wing_perp < min_side {
        return None;
    }

    // Position within the gap: corner-flush or centered
    let offset_in_gap = if rng.percent(70) {
        if rng.percent(50) {
            0
        } else {
            gap_len - wing_along
        }
    } else {
        (gap_len - wing_along) / 2
    };
    let offset = gap_start + offset_in_gap;

    // Compute available depth on this side
    let max_perp = match side {
        Side::North => core.min().y - candidate.min().y,
        Side::South => candidate.max().y - core.max().y,
        Side::West => core.min().x - candidate.min().x,
        Side::East => candidate.max().x - core.max().x,
    };

    let wing_perp = wing_perp.min(max_perp);
    if wing_perp < min_side {
        return None;
    }

    // Compute wing rectangle in world coordinates
    let (wing_x, wing_z, wing_w, wing_h) = match side {
        Side::North => (
            core.min().x + offset,
            core.min().y - wing_perp,
            wing_along,
            wing_perp,
        ),
        Side::South => (
            core.min().x + offset,
            core.max().y + 1,
            wing_along,
            wing_perp,
        ),
        Side::West => (
            core.min().x - wing_perp,
            core.min().y + offset,
            wing_perp,
            wing_along,
        ),
        Side::East => (
            core.max().x + 1,
            core.min().y + offset,
            wing_perp,
            wing_along,
        ),
    };

    let wing = Rect2D::new(Point2D::new(wing_x, wing_z), Point2D::new(wing_w, wing_h));

    if !candidate.contains_rect(&wing) {
        return None;
    }

    let span = Span { start: offset, end: offset + wing_along - 1 };
    Some((wing, span))
}

/// A complete layout: core + wings.
#[derive(Debug, Clone)]
pub struct Layout {
    pub core: Rect2D,
    pub wings: Vec<Rect2D>,
}

const MAX_ASPECT_RATIO: f32 = 2.5;

impl Layout {
    pub fn total_area(&self) -> i32 {
        self.core.area() + self.wings.iter().map(|w| w.area()).sum::<i32>()
    }

    /// Aspect ratio of the overall bounding box (always >= 1.0).
    pub fn aspect_ratio(&self) -> f32 {
        let rects = self.rects();
        let min_x = rects.iter().map(|r| r.min().x).min().unwrap();
        let min_z = rects.iter().map(|r| r.min().y).min().unwrap();
        let max_x = rects.iter().map(|r| r.max().x).max().unwrap();
        let max_z = rects.iter().map(|r| r.max().y).max().unwrap();
        let w = (max_x - min_x + 1) as f32;
        let h = (max_z - min_z + 1) as f32;
        w.max(h) / w.min(h)
    }

    /// All rects in order: core first, then wings.
    pub fn rects(&self) -> Vec<Rect2D> {
        let mut rects = vec![self.core];
        rects.extend_from_slice(&self.wings);
        rects
    }
}

const WEIGHT_AREA_MATCH: f32 = 1.0;
const WEIGHT_PROPORTION: f32 = 0.6;
const WEIGHT_BALANCE: f32 = 0.2;
const WEIGHT_COMPLEXITY: f32 = 0.8;

/// Scores a layout from 0.0 to 1.0 on multiple criteria.
pub fn score_layout(layout: &Layout, target_area: i32, candidate: &Rect2D) -> f32 {
    let total_weight = WEIGHT_AREA_MATCH + WEIGHT_PROPORTION + WEIGHT_BALANCE + WEIGHT_COMPLEXITY;

    // Area match: how close total area is to target. 1.0 = exact, drops off linearly.
    let area_ratio = layout.total_area() as f32 / target_area as f32;
    let area_score = 1.0 - (area_ratio - 1.0).abs().min(1.0);

    // Proportion: wings should be smaller than core. Penalize wings approaching core size.
    let proportion_score = if layout.wings.is_empty() {
        1.0
    } else {
        let core_area = layout.core.area() as f32;
        let worst_ratio = layout.wings.iter()
            .map(|w| w.area() as f32 / core_area)
            .fold(0.0f32, f32::max);
        // 0.0 ratio = perfect (tiny wing), 1.0 = same size as core = bad
        1.0 - worst_ratio.min(1.0)
    };

    // Balance: prefer closer to center of candidate area.
    let layout_bounds = {
        let rects = layout.rects();
        let min_x = rects.iter().map(|r| r.min().x).min().unwrap();
        let min_z = rects.iter().map(|r| r.min().y).min().unwrap();
        let max_x = rects.iter().map(|r| r.max().x).max().unwrap();
        let max_z = rects.iter().map(|r| r.max().y).max().unwrap();
        (min_x, min_z, max_x, max_z)
    };
    let candidate_center = candidate.midpoint();
    let layout_center_x = (layout_bounds.0 + layout_bounds.2) as f32 / 2.0;
    let layout_center_z = (layout_bounds.1 + layout_bounds.3) as f32 / 2.0;
    let max_dist = (candidate.length() as f32 / 2.0).hypot(candidate.width() as f32 / 2.0);
    let dist = (layout_center_x - candidate_center.x as f32).hypot(layout_center_z - candidate_center.y as f32);
    let balance_score = if max_dist > 0.0 { 1.0 - (dist / max_dist).min(1.0) } else { 1.0 };

    // Complexity: reward having wings. 0 wings = 0.0, 1 = 0.6, 2 = 0.8, 3+ = 1.0.
    let complexity_score = match layout.wings.len() {
        0 => 0.0,
        1 => 0.6,
        2 => 0.8,
        _ => 1.0,
    };

    (WEIGHT_AREA_MATCH * area_score
        + WEIGHT_PROPORTION * proportion_score
        + WEIGHT_BALANCE * balance_score
        + WEIGHT_COMPLEXITY * complexity_score)
        / total_weight
}

/// Scores all layouts and selects one via weighted random.
/// Filters out layouts whose total area is below `min_area` before scoring.
pub fn select_layout(rng: &mut RNG, layouts: &[Layout], target_area: i32, candidate: &Rect2D, min_area: i32) -> Option<Layout> {
    let viable: Vec<&Layout> = layouts.iter()
        .filter(|l| l.total_area() >= min_area && l.aspect_ratio() <= MAX_ASPECT_RATIO)
        .collect();

    if viable.is_empty() {
        return None;
    }

    let scored: Vec<(&Layout, f32)> = viable.into_iter()
        .map(|l| {
            let score = score_layout(l, target_area, candidate);
            // Square the score to bias more heavily toward better layouts.
            (l, score * score)
        })
        .collect();

    Some((*rng.choose_weighted_vec(&scored)).clone())
}

/// Generates a wing configuration for a given core.
fn generate_wings(
    rng: &mut RNG,
    core: &Rect2D,
    candidate: &Rect2D,
    size_class: &SizeClass,
    target_area: i32,
) -> Vec<Rect2D> {
    if size_class.max_wings == 0 {
        return vec![];
    }

    let mut wings = Vec::new();
    let mut occupied: HashMap<Side, Vec<Span>> = HashMap::new();
    let mut current_area = core.area();

    let num_wings = rng.rand_i32_range(size_class.min_wings.max(1), size_class.max_wings + 1);

    for attempt in 0..num_wings {
        let remaining = target_area - current_area;
        if remaining < MIN_WING_SIDE * MIN_WING_SIDE && wings.len() as i32 >= size_class.min_wings {
            break;
        }

        // Pick a random side (allow repeats — gaps will be checked)
        let &side = rng.choose(&ALL_SIDES);
        let spans = occupied.get(&side).map(|v| v.as_slice()).unwrap_or(&[]);

        let wings_left = num_wings - attempt;
        if let Some((wing, span)) = attach_wing(rng, core, side, candidate, remaining, MIN_WING_SIDE, core.area(), wings_left, spans) {
            current_area += wing.area();
            occupied.entry(side).or_default().push(span);
            wings.push(wing);
        }
    }

    wings
}

/// Result of layout generation, including context needed for scoring.
pub struct GeneratedLayouts {
    pub layouts: Vec<Layout>,
    pub target_area: i32,
    pub candidate: Rect2D,
}

/// Generates multiple complete layouts (core + wings) for a plot and size class.
/// Returns K * W layouts total (K core variants, W wing configs each).
pub fn generate_layouts(
    rng: &mut RNG,
    plot: &Plot,
    size_class: &SizeClass,
    core_count: i32,
    wings_per_core: i32,
) -> Option<GeneratedLayouts> {
    let candidate = find_candidate(plot, size_class)?;

    let target_area = rng.rand_i32_range(size_class.target_area_min, size_class.target_area_max + 1)
        .min(candidate.area());

    let mut layouts = Vec::new();

    for _ in 0..core_count {
        let core = match generate_core(rng, &candidate, size_class, target_area) {
            Some(c) => c,
            None => continue,
        };

        if size_class.max_wings == 0 {
            layouts.push(Layout { core, wings: vec![] });
        } else {
            for _ in 0..wings_per_core {
                let wings = generate_wings(rng, &core, &candidate, size_class, target_area);
                layouts.push(Layout { core, wings });
            }
        }
    }

    if layouts.is_empty() {
        return None;
    }

    Some(GeneratedLayouts { layouts, target_area, candidate })
}

// Keep the old public API working
pub fn generate_cores(
    rng: &mut RNG,
    plot: &Plot,
    size_class: &SizeClass,
    count: i32,
) -> Vec<Rect2D> {
    let candidate = match find_candidate(plot, size_class) {
        Some(c) => c,
        None => return vec![],
    };

    let target_area = rng.rand_i32_range(size_class.target_area_min, size_class.target_area_max + 1)
        .min(candidate.area());

    let mut cores = Vec::new();
    for _ in 0..count {
        if let Some(core) = generate_core(rng, &candidate, size_class, target_area) {
            cores.push(core);
        }
    }

    cores
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_layout(plot: &Plot, layout: &Layout) -> String {
        let min = plot.bounds.min();
        let w = plot.bounds.length();
        let h = plot.bounds.width();
        let mut lines = Vec::new();

        for z in 0..h {
            let mut row = String::new();
            for x in 0..w {
                let world_point = Point2D::new(min.x + x, min.y + z);
                let usable = plot.usable[x as usize][z as usize];

                let in_core = layout.core.contains(world_point);
                let wing_idx = layout.wings.iter().position(|w| w.contains(world_point));

                if in_core {
                    row.push('#');
                } else if let Some(idx) = wing_idx {
                    let ch = (b'1' + idx as u8) as char;
                    row.push(ch);
                } else if usable {
                    row.push('.');
                } else {
                    row.push('x');
                }
            }
            lines.push(row);
        }

        lines.join("\n")
    }

    fn render_cores(plot: &Plot, cores: &[Rect2D]) -> String {
        let min = plot.bounds.min();
        let w = plot.bounds.length();
        let h = plot.bounds.width();
        let mut lines = Vec::new();

        for z in 0..h {
            let mut row = String::new();
            for x in 0..w {
                let world_point = Point2D::new(min.x + x, min.y + z);
                let usable = plot.usable[x as usize][z as usize];

                let core_index = cores.iter().position(|c| c.contains(world_point));

                if let Some(idx) = core_index {
                    let ch = if idx < 10 {
                        (b'0' + idx as u8) as char
                    } else {
                        (b'A' + (idx - 10) as u8) as char
                    };
                    row.push(ch);
                } else if usable {
                    row.push('.');
                } else {
                    row.push('x');
                }
            }
            lines.push(row);
        }

        lines.join("\n")
    }

    #[test]
    fn generate_cores_fully_usable() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(42);

        let cores = generate_cores(&mut rng, &plot, &SizeClass::HOUSE, 6);

        println!("Generated {} cores for HOUSE:", cores.len());
        for (i, core) in cores.iter().enumerate() {
            println!("  Core {}: origin={:?} size={:?} area={}",
                i, core.origin, core.size, core.area());
        }
        println!("{}", render_cores(&plot, &cores));

        assert!(!cores.is_empty());
        for core in &cores {
            assert!(core.area() >= SizeClass::HOUSE.min_side * SizeClass::HOUSE.min_side);
            assert!(core.length() >= SizeClass::HOUSE.min_side);
            assert!(core.width() >= SizeClass::HOUSE.min_side);
        }
    }

    #[test]
    fn generate_cores_all_size_classes() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);

        for (name, class) in [
            ("COTTAGE", SizeClass::COTTAGE),
            ("HOUSE", SizeClass::HOUSE),
            ("HALL", SizeClass::HALL),
            ("MANOR", SizeClass::MANOR),
        ] {
            let mut rng = RNG::new(42);
            let cores = generate_cores(&mut rng, &plot, &class, 5);

            println!("\n=== {} ===", name);
            for (i, core) in cores.iter().enumerate() {
                println!("  Core {}: {}x{} area={}",
                    i, core.length(), core.width(), core.area());
            }
            println!("{}", render_cores(&plot, &cores));

            assert!(!cores.is_empty());
        }
    }

    #[test]
    fn generate_cores_small_plot() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(7, 7));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(42);

        let cores = generate_cores(&mut rng, &plot, &SizeClass::COTTAGE, 5);

        println!("Small plot cores:");
        for (i, core) in cores.iter().enumerate() {
            println!("  Core {}: {}x{} area={}",
                i, core.length(), core.width(), core.area());
        }
        println!("{}", render_cores(&plot, &cores));

        assert!(!cores.is_empty());
    }

    #[test]
    fn generate_cores_with_obstacles() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(24, 24));
        let mut plot = Plot::fully_usable(bounds);

        for x in 0..8 {
            for z in 0..8 {
                plot.usable[x][z] = false;
            }
        }

        let mut rng = RNG::new(99);
        let cores = generate_cores(&mut rng, &plot, &SizeClass::HALL, 5);

        println!("Cores with obstacles:");
        for (i, core) in cores.iter().enumerate() {
            println!("  Core {}: origin={:?} size={:?} area={}",
                i, core.origin, core.size, core.area());
        }
        println!("{}", render_cores(&plot, &cores));

        assert!(!cores.is_empty());
        for core in &cores {
            for point in core.iter() {
                assert!(plot.is_usable(point),
                    "Core contains unusable cell at {:?}", point);
            }
        }
    }

    // --- Wing tests ---

    #[test]
    fn generate_layouts_house() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(42);

        let result = generate_layouts(&mut rng, &plot, &SizeClass::HOUSE, 4, 3).unwrap();
        let layouts = &result.layouts;

        println!("Generated {} HOUSE layouts (target_area={}):", layouts.len(), result.target_area);
        for (i, layout) in layouts.iter().enumerate() {
            let score = score_layout(layout, result.target_area, &result.candidate);
            println!("\n--- Layout {} --- core: {}x{} wings: {} total_area: {} score: {:.3}",
                i, layout.core.length(), layout.core.width(),
                layout.wings.len(), layout.total_area(), score);
            for (j, wing) in layout.wings.iter().enumerate() {
                println!("  Wing {}: {}x{} area={}", j, wing.length(), wing.width(), wing.area());
            }
            println!("{}", render_layout(&plot, layout));
        }

        assert!(!layouts.is_empty());
        let with_wings = layouts.iter().filter(|l| !l.wings.is_empty()).count();
        println!("\n{} of {} layouts have wings", with_wings, layouts.len());
    }

    #[test]
    fn generate_layouts_hall() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(77);

        let result = generate_layouts(&mut rng, &plot, &SizeClass::HALL, 4, 3).unwrap();
        let layouts = &result.layouts;

        println!("Generated {} TOWNHOUSE layouts (target_area={}):", layouts.len(), result.target_area);
        for (i, layout) in layouts.iter().enumerate() {
            let score = score_layout(layout, result.target_area, &result.candidate);
            println!("\n--- Layout {} --- core: {}x{} wings: {} total_area: {} score: {:.3}",
                i, layout.core.length(), layout.core.width(),
                layout.wings.len(), layout.total_area(), score);
            println!("{}", render_layout(&plot, layout));
        }

        assert!(!layouts.is_empty());
    }

    #[test]
    fn generate_layouts_manor() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(123);

        let result = generate_layouts(&mut rng, &plot, &SizeClass::MANOR, 3, 4).unwrap();
        let layouts = &result.layouts;

        println!("Generated {} MANOR layouts (target_area={}):", layouts.len(), result.target_area);
        for (i, layout) in layouts.iter().enumerate() {
            let score = score_layout(layout, result.target_area, &result.candidate);
            println!("\n--- Layout {} --- core: {}x{} wings: {} total_area: {} score: {:.3}",
                i, layout.core.length(), layout.core.width(),
                layout.wings.len(), layout.total_area(), score);
            println!("{}", render_layout(&plot, layout));
        }

        assert!(!layouts.is_empty());
    }

    #[test]
    fn generate_layouts_cottage() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(14, 14));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(42);

        let result = generate_layouts(&mut rng, &plot, &SizeClass::COTTAGE, 5, 3).unwrap();
        let layouts = &result.layouts;

        println!("Generated {} COTTAGE layouts:", layouts.len());
        for (i, layout) in layouts.iter().enumerate() {
            println!("  Layout {}: {}x{} area={} wings={}",
                i, layout.core.length(), layout.core.width(),
                layout.total_area(), layout.wings.len());
        }

        assert!(!layouts.is_empty());
        for layout in layouts {
            assert!(layout.wings.len() <= 1, "Cottages should have at most 1 wing");
        }
    }

    #[test]
    fn generate_layouts_with_obstacles() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let mut plot = Plot::fully_usable(bounds);

        // Lake in top-right
        for x in 20..30 {
            for z in 0..8 {
                plot.usable[x][z] = false;
            }
        }

        let mut rng = RNG::new(55);
        let result = generate_layouts(&mut rng, &plot, &SizeClass::HALL, 4, 3).unwrap();
        let layouts = &result.layouts;

        println!("Layouts with obstacles:");
        for (i, layout) in layouts.iter().enumerate() {
            println!("\n--- Layout {} --- wings: {} total_area: {}",
                i, layout.wings.len(), layout.total_area());
            println!("{}", render_layout(&plot, layout));
        }

        // Verify nothing overlaps obstacles
        for layout in layouts {
            for rect in layout.rects() {
                for point in rect.iter() {
                    assert!(plot.is_usable(point),
                        "Layout rect contains unusable cell at {:?}", point);
                }
            }
        }
    }

    #[test]
    fn select_layout_prefers_better_scores() {
        let bounds = Rect2D::from_points(Point2D::new(0, 0), Point2D::new(29, 29));
        let plot = Plot::fully_usable(bounds);
        let mut rng = RNG::new(42);

        let result = generate_layouts(&mut rng, &plot, &SizeClass::HALL, 6, 4).unwrap();

        // Print all scores
        println!("All layout scores (target_area={}):", result.target_area);
        for (i, layout) in result.layouts.iter().enumerate() {
            let score = score_layout(layout, result.target_area, &result.candidate);
            println!("  Layout {}: area={} score={:.3}", i, layout.total_area(), score);
        }

        // Select 20 times, verify we get results and they tend toward higher scores
        let mut selected_scores = Vec::new();
        for seed in 0..20 {
            let mut select_rng = RNG::new(seed);
            let selected = select_layout(&mut select_rng, &result.layouts, result.target_area, &result.candidate, SizeClass::HALL.min_side * SizeClass::HALL.min_side).unwrap();
            let score = score_layout(&selected, result.target_area, &result.candidate);
            selected_scores.push(score);
        }

        let avg_selected: f32 = selected_scores.iter().sum::<f32>() / selected_scores.len() as f32;
        let all_scores: Vec<f32> = result.layouts.iter()
            .map(|l| score_layout(l, result.target_area, &result.candidate))
            .collect();
        let avg_all: f32 = all_scores.iter().sum::<f32>() / all_scores.len() as f32;

        println!("\nAvg score of all layouts: {:.3}", avg_all);
        println!("Avg score of selected:   {:.3}", avg_selected);

        // Selected average should be at least as good as overall average
        assert!(avg_selected >= avg_all * 0.9,
            "Selection should bias toward better layouts");
    }
}
