#[cfg(test)]
mod test;

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use super::footprint::merge::walk_edge_cells;
use super::footprint::SizeClass;
use super::frame::Frame;
use super::pipeline::BuildCtx;

/// What fills a window opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFill {
    Glass,
    Trapdoor,
    Open,
}

/// Per-panel decorative motif laid inside a stud-bounded panel. `Empty` is a
/// blank wattle-and-daub panel; the others place stair-block braces at the
/// panel corners. `Pillar` is a full-height extra stud, useful as a divider
/// inside composite sequences like `/`-`|`-`\`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelShape {
    Empty,
    Vee,      // V — braces at top corners, point at bottom center
    Chevron,  // ^ — braces at bottom corners, point at top center
    Cross,    // X — all four corners
    Forward,  // / — bottom-left + top-right
    Back,     // \ — top-left + bottom-right
    Pillar,   // | — extra vertical stud column
    KRight,   // K — left pillar + Forward in the right half
    KLeft,    // ⊣ — right pillar + Back in the left half
}

/// Extra timber detail laid over the baseline corner posts + floor/ceiling beams.
/// `Plain` is the original look (just the skeleton). `Studded` adds vertical
/// studs; `Braced` adds corner knee braces on top of the studs. `Decorated`
/// fills each stud-bounded panel with a single uniform `PanelShape` motif
/// (one design per wall — never a mix). Braces are placed as contrasting
/// plank stairs, not logs, so the frame reads as posts + lighter carpentry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimberPattern {
    Plain,
    Studded { spacing: u32 },
    Braced { spacing: u32 },
    Decorated { spacing: u32 },
}

impl TimberPattern {
    /// Roll a pattern biased by size class. Cottages stay simple; bigger
    /// buildings get denser timber so a settlement reads as a mix. Patterns
    /// whose studs wouldn't actually appear given the longest wall segment
    /// are filtered out before sampling, so we never silently downgrade to
    /// Plain after the fact. `max_seg_length` is the longest segment in the
    /// frame (across all floors); if no studded variant fits, returns Plain.
    pub fn pick(size_class: SizeClass, max_seg_length: u32, rng: &mut RNG) -> Self {
        let table: &[(Self, u32)] = match size_class {
            SizeClass::Cottage => &[
                (Self::Plain, 2),
                (Self::Studded { spacing: 3 }, 2),
            ],
            SizeClass::House => &[
                (Self::Plain, 1),
                (Self::Studded { spacing: 3 }, 2),
                (Self::Braced { spacing: 4 }, 1),
                (Self::Decorated { spacing: 3 }, 1),
            ],
            SizeClass::Hall => &[
                (Self::Studded { spacing: 3 }, 1),
                (Self::Braced { spacing: 4 }, 3),
                (Self::Decorated { spacing: 3 }, 2),
            ],
            SizeClass::Manor => &[
                (Self::Braced { spacing: 4 }, 2),
                (Self::Decorated { spacing: 3 }, 2),
                (Self::Decorated { spacing: 4 }, 1),
            ],
        };

        let eligible: Vec<(Self, u32)> = table.iter()
            .copied()
            .filter(|(p, _)| p.fits(max_seg_length))
            .collect();
        if eligible.is_empty() {
            return Self::Plain;
        }
        let total: u32 = eligible.iter().map(|(_, w)| w).sum();
        let mut roll = rng.rand_i32_range(0, total as i32) as u32;
        for (p, w) in &eligible {
            if roll < *w { return *p; }
            roll -= w;
        }
        eligible[0].0
    }

    /// True if this pattern's timber would actually appear on a wall whose
    /// longest segment is `max_seg_length` cells. `Plain` is always true;
    /// studded variants require `stud_indices` to produce at least one column.
    pub fn fits(&self, max_seg_length: u32) -> bool {
        match self {
            Self::Plain => true,
            Self::Studded { spacing }
            | Self::Braced { spacing }
            | Self::Decorated { spacing } => !stud_indices(max_seg_length, *spacing).is_empty(),
        }
    }

    fn has_studs(&self) -> bool {
        matches!(self,
            Self::Studded { .. } | Self::Braced { .. } | Self::Decorated { .. })
    }

    fn has_corner_braces(&self) -> bool {
        matches!(self, Self::Braced { .. })
    }

    fn has_panel_decorations(&self) -> bool {
        matches!(self, Self::Decorated { .. })
    }

    fn spacing(&self) -> u32 {
        match self {
            Self::Plain => 0,
            Self::Studded { spacing }
            | Self::Braced { spacing }
            | Self::Decorated { spacing } => *spacing,
        }
    }
}

/// A single straight run of wall between two outline vertices, at one floor level.
#[derive(Debug, Clone)]
pub struct WallSegment {
    /// Start vertex (dual-grid coords)
    pub start: Point2D,
    /// End vertex (dual-grid coords)
    pub end: Point2D,
    /// Extra cell prepended at a concave start corner
    pub extra_start: Option<Point2D>,
    /// Extra cell appended at a concave end corner
    pub extra_end: Option<Point2D>,
    /// Which direction the wall faces outward
    pub facing: Cardinal,
    /// Floor index (0 = ground)
    pub floor: u32,
    /// Y position of the floor surface in world coords
    pub base_y: i32,
    /// Wall height in blocks of air for this floor
    pub height: u32,
    /// Length of this segment in blocks
    pub length: i32,
    /// Openings cut into this segment
    pub openings: Vec<Opening>,
}

/// An opening (door or window) in a wall segment.
#[derive(Debug, Clone)]
pub struct Opening {
    pub kind: OpeningKind,
    /// Offset along the segment (in blocks from segment start)
    pub offset: u32,
    /// Width in blocks
    pub width: u32,
    /// Height in blocks
    pub height: u32,
    /// Vertical offset from floor base (usually 0 for doors, 1 for windows)
    pub y_offset: u32,
}

#[derive(Debug, Clone)]
pub enum OpeningKind {
    Door(DoorStyle),
    Window(WindowStyle),
}

#[derive(Debug, Clone, Copy)]
pub enum DoorStyle {
    Single,  // 1 wide, 2 tall
    Double,  // 2 wide, 2 tall
    Archway, // 2-3 wide, 3 tall
}

#[derive(Debug, Clone, Copy)]
pub enum WindowStyle {
    Small,     // 1 wide, 1 tall
    Tall,      // 1 wide, 2 tall
    Wide,      // 2 wide, 2 tall
    Decorated, // 1 wide, 2 tall — upside-down stair on top, air/fill below
    Arched,    // 2 wide, 2 tall — inward-facing upside-down stairs on top, air/fill below
}

/// Collection of all wall segments for a building.
pub struct WallSegments {
    pub segments: Vec<WallSegment>,
}

impl WallSegments {
    pub fn segments_on_floor(&self, floor: u32) -> impl Iterator<Item = &WallSegment> {
        self.segments.iter().filter(move |s| s.floor == floor)
    }

    pub fn doors(&self) -> impl Iterator<Item = (&WallSegment, &Opening)> {
        self.segments.iter().flat_map(|s| {
            s.openings.iter()
                .filter(|o| matches!(o.kind, OpeningKind::Door(_)))
                .map(move |o| (s, o))
        })
    }

    pub fn windows(&self) -> impl Iterator<Item = (&WallSegment, &Opening)> {
        self.segments.iter().flat_map(|s| {
            s.openings.iter()
                .filter(|o| matches!(o.kind, OpeningKind::Window(_)))
                .map(move |o| (s, o))
        })
    }
}

/// Compute the outward-facing direction for a clockwise polygon edge.
/// The outward normal is 90 degrees clockwise from the walk direction.
fn facing_from_edge(start: Point2D, end: Point2D) -> Cardinal {
    let dx = (end.x - start.x).signum();
    let dz = (end.y - start.y).signum();
    match (dx, dz) {
        (1, 0) => Cardinal::South,  // walking +x, outward is +z
        (-1, 0) => Cardinal::North, // walking -x, outward is -z
        (0, 1) => Cardinal::West,   // walking +z, outward is -x
        (0, -1) => Cardinal::East,  // walking -z, outward is +x
        _ => unreachable!("Non-axis-aligned edge: {:?} -> {:?}", start, end),
    }
}

/// Dual-grid vertex to cell offset for a given walk direction.
fn edge_offset(dx: i32, dz: i32) -> (i32, i32) {
    match (dx, dz) {
        (1, 0) => (0, 0),
        (0, 1) => (-1, 0),
        (-1, 0) => (-1, -1),
        (0, -1) => (0, -1),
        _ => (0, 0),
    }
}

/// Returns true if vertex `curr` is a concave (inner) corner in a CW polygon.
fn is_concave_corner(prev: Point2D, curr: Point2D, next: Point2D) -> bool {
    let dx1 = (curr.x - prev.x).signum();
    let dz1 = (curr.y - prev.y).signum();
    let dx2 = (next.x - curr.x).signum();
    let dz2 = (next.y - curr.y).signum();
    dx1 * dz2 - dz1 * dx2 < 0
}

/// Build wall segments from the frame's per-floor outlines.
/// Each edge of each floor's outline becomes a WallSegment.
/// At convex corners, adjacent segments naturally share a cell.
/// At concave corners, both meeting segments extend by one cell into the
/// corner, creating an overlap.
pub fn build_segments(frame: &Frame) -> WallSegments {
    let mut segments = Vec::new();

    for floor in frame.floors() {
        let outline = frame.outline_at_floor(floor);
        let n = outline.len();

        for i in 0..n {
            let prev = outline[(i + n - 1) % n];
            let start = outline[i];
            let end = outline[(i + 1) % n];
            let next = outline[(i + 2) % n];
            let facing = facing_from_edge(start, end);

            let cur_dx = (end.x - start.x).signum();
            let cur_dz = (end.y - start.y).signum();
            let (cur_ox, cur_oz) = edge_offset(cur_dx, cur_dz);

            // At concave start: prepend a cell using the previous edge's offset
            let extra_start = if is_concave_corner(prev, start, end) {
                let prev_dx = (start.x - prev.x).signum();
                let prev_dz = (start.y - prev.y).signum();
                let (ox, oz) = edge_offset(prev_dx, prev_dz);
                Some(Point2D::new(start.x + ox, start.y + oz))
            } else {
                None
            };

            // At concave end: append a cell using this edge's offset
            let extra_end = if is_concave_corner(start, end, next) {
                Some(Point2D::new(end.x + cur_ox, end.y + cur_oz))
            } else {
                None
            };

            let walk_len = (end.x - start.x).abs() + (end.y - start.y).abs();
            let length = walk_len
                + extra_start.is_some() as i32
                + extra_end.is_some() as i32;

            segments.push(WallSegment {
                start,
                end,
                extra_start,
                extra_end,
                facing,
                floor,
                base_y: frame.floor_y(floor),
                height: frame.wall_height(),
                length,
                openings: Vec::new(),
            });
        }
    }

    WallSegments { segments }
}

/// Walk cells for a wall segment.
/// At convex corners, adjacent segments naturally share cells.
/// At concave corners, extra cells are prepended/appended so both
/// meeting segments cover the inner corner block.
pub fn segment_cells(seg: &WallSegment) -> Vec<Point2D> {
    let mut cells = Vec::with_capacity(seg.length as usize);
    if let Some(cell) = seg.extra_start {
        cells.push(cell);
    }
    cells.extend(walk_edge_cells(seg.start, seg.end));
    if let Some(cell) = seg.extra_end {
        cells.push(cell);
    }
    cells
}

/// Distance from a point to the nearest edge of a rectangle.
fn distance_to_plot_edge(point: Point2D, plot_bounds: &Rect2D) -> i32 {
    let min = plot_bounds.min();
    let max = plot_bounds.max();
    let dx_min = (point.x - min.x).abs();
    let dx_max = (point.x - max.x).abs();
    let dz_min = (point.y - min.y).abs();
    let dz_max = (point.y - max.y).abs();
    dx_min.min(dx_max).min(dz_min).min(dz_max)
}

/// Midpoint of a wall segment in world coords.
fn segment_midpoint(seg: &WallSegment) -> Point2D {
    Point2D::new(
        (seg.start.x + seg.end.x) / 2,
        (seg.start.y + seg.end.y) / 2,
    )
}

/// Place doors on ground-floor segments. Picks the segment closest to the plot
/// edge (likely road-facing). For large buildings, adds a second door on the
/// opposite side.
pub fn place_doors(wall_segs: &mut WallSegments, plot_bounds: &Rect2D, footprint_area: i32, boundary_cells: &HashSet<Point2D>, _rng: &mut RNG) {
    let door_style = if footprint_area > 150 {
        DoorStyle::Double
    } else {
        DoorStyle::Single
    };
    let door_width = match door_style {
        DoorStyle::Single => 1,
        DoorStyle::Double => 2,
        DoorStyle::Archway => 2,
    };
    let min_segment_len = door_width as i32 + 4; // 2 blocks margin each side

    // Helper: true if a segment overlaps interior boundary cells (where archways go)
    let is_on_boundary = |i: usize| {
        segment_cells(&wall_segs.segments[i]).iter().any(|c| boundary_cells.contains(c))
    };

    // Score ground-floor segments by distance to nearest plot edge (lower = better)
    // Skip segments that overlap with interior boundary cells (where dividing walls go).
    let mut scored: Vec<(usize, i32)> = wall_segs
        .segments
        .iter()
        .enumerate()
        .filter(|(i, s)| s.floor == 0 && s.length >= min_segment_len && !is_on_boundary(*i))
        .map(|(i, s)| {
            let mid = segment_midpoint(s);
            let dist = distance_to_plot_edge(mid, plot_bounds);
            (i, dist)
        })
        .collect();

    scored.sort_by_key(|&(_, dist)| dist);

    if scored.is_empty() {
        return;
    }

    // Place primary door on the closest segment
    let primary_idx = scored[0].0;
    let primary_offset = (wall_segs.segments[primary_idx].length as u32 - door_width) / 2;
    wall_segs.segments[primary_idx].openings.push(Opening {
        kind: OpeningKind::Door(door_style),
        offset: primary_offset,
        width: door_width,
        height: 2,
        y_offset: 0,
    });

    // For larger buildings, place a second door on the opposite wall.
    // Falls back to any non-adjacent facing if opposite isn't available.
    if footprint_area > 100 {
        let primary_facing = wall_segs.segments[primary_idx].facing;
        let opposite = -primary_facing;
        if let Some(&(idx, _)) = scored.iter()
            .find(|&&(i, _)| wall_segs.segments[i].facing == opposite)
            .or_else(|| scored.iter().find(|&&(i, _)| {
                wall_segs.segments[i].facing != primary_facing
            }))
        {
            let offset = (wall_segs.segments[idx].length as u32 - door_width) / 2;
            wall_segs.segments[idx].openings.push(Opening {
                kind: OpeningKind::Door(DoorStyle::Single),
                offset,
                width: 1,
                height: 2,
                y_offset: 0,
            });
        }
    }
}

/// Collect all interior boundary cells between adjacent rects into a HashSet.
/// Used to prevent side doors from overlapping with interior archway positions.
pub fn boundary_cell_set(rects: &[Rect2D]) -> HashSet<Point2D> {
    use super::footprint::find_boundaries;
    find_boundaries(rects)
        .into_iter()
        .flat_map(|b| b.wall_cells)
        .collect()
}

/// Place doors from upper floors onto the flat roof of a shorter adjacent rect.
/// For each boundary between rects with different floor counts, the shorter
/// rect's roof aligns with a floor of the taller rect. A door is placed in the
/// taller rect's wall at that floor, giving access to the rooftop terrace.
///
/// Returns the 2D positions of the door cells on the roof surface (for parapet
/// exclusion and room/cell-state marking).
pub fn place_terrace_doors(
    wall_segs: &mut WallSegments,
    frame: &Frame,
) -> Vec<Point2D> {
    use super::footprint::find_boundaries;
    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);
    let mut terrace_door_cells = Vec::new();

    for boundary in &boundaries {
        let fc_a = frame.floor_counts()[boundary.rect_a];
        let fc_b = frame.floor_counts()[boundary.rect_b];
        if fc_a == fc_b { continue; }

        // The shorter rect's roof is accessed from the taller rect's floor
        // at index == shorter's floor count.
        let (short_fc, _tall_fc) = if fc_a < fc_b { (fc_a, fc_b) } else { (fc_b, fc_a) };
        let door_floor = short_fc; // floor index on the taller rect

        // Find wall segments on that floor whose cells overlap with boundary cells
        let boundary_set: HashSet<Point2D> = boundary.wall_cells.iter().copied().collect();

        for seg_idx in 0..wall_segs.segments.len() {
            let seg = &wall_segs.segments[seg_idx];
            if seg.floor != door_floor { continue; }

            let cells = segment_cells(seg);
            // Find which cell indices overlap with the boundary
            let overlapping: Vec<usize> = cells.iter().enumerate()
                .filter(|(_, c)| boundary_set.contains(c))
                .map(|(i, _)| i)
                .collect();

            if overlapping.is_empty() { continue; }

            // Place a single door centered on the overlap
            let mid_idx = overlapping[overlapping.len() / 2];
            let offset = mid_idx as u32;

            // Check segment is long enough and door doesn't overlap existing openings
            if offset == 0 || offset >= seg.length as u32 - 1 { continue; }

            let overlaps_existing = seg.openings.iter().any(|o| {
                let o_start = o.offset.saturating_sub(1);
                let o_end = o.offset + o.width + 1;
                offset + 1 > o_start && offset < o_end
            });
            if overlaps_existing { continue; }

            wall_segs.segments[seg_idx].openings.push(Opening {
                kind: OpeningKind::Door(DoorStyle::Single),
                offset,
                width: 1,
                height: 2,
                y_offset: 0,
            });

            // The door cell on the roof surface
            terrace_door_cells.push(cells[mid_idx]);
        }
    }

    terrace_door_cells
}

/// Place windows on all wall segments. Even spacing, denser on upper floors.
/// Skips positions that overlap with existing doors.
pub fn place_windows(
    wall_segs: &mut WallSegments,
    interior_walls: &HashSet<(i32, i32)>,
    _rng: &mut RNG,
) {
    let corner_margin: u32 = 1;

    for seg_idx in 0..wall_segs.segments.len() {
        let seg = &wall_segs.segments[seg_idx];
        let seg_len = seg.length as u32;

        if seg_len < corner_margin * 2 + 1 {
            continue; // too short for any window
        }

        let cells = segment_cells(seg);
        let available = seg_len - corner_margin * 2;

        // Window style based on segment length
        let (style, win_width, win_height) = if available >= 8 {
            (WindowStyle::Arched, 2u32, 2u32)
        } else if available >= 5 {
            (WindowStyle::Decorated, 1, 2)
        } else {
            (WindowStyle::Small, 1u32, 1u32)
        };

        let spacing = if matches!(style, WindowStyle::Arched) { 5u32 } else { 3u32 };
        let count = available / spacing;
        if count == 0 {
            continue;
        }

        // Evenly distribute
        let stride = available / (count + 1);
        let existing_openings = seg.openings.clone();

        for i in 1..=count {
            let offset = corner_margin + stride * i - win_width / 2;

            // Check overlap with existing doors
            let overlaps = existing_openings.iter().any(|o| {
                let o_start = o.offset.saturating_sub(1);
                let o_end = o.offset + o.width + 1;
                offset + win_width > o_start && offset < o_end
            });
            if overlaps { continue; }

            // Check overlap with interior walls
            let hits_interior = (0..win_width).any(|dx| {
                let idx = (offset + dx) as usize;
                idx < cells.len() && interior_walls.contains(&(cells[idx].x, cells[idx].y))
            });
            if hits_interior { continue; }

            wall_segs.segments[seg_idx].openings.push(Opening {
                kind: OpeningKind::Window(style),
                offset,
                width: win_width,
                height: win_height,
                y_offset: 1,
            });
        }
    }
}

/// Check if a position (cell index along segment, relative y) falls inside any opening.
fn is_inside_opening(openings: &[Opening], idx: u32, ry: u32) -> bool {
    openings.iter().any(|o| {
        idx >= o.offset
            && idx < o.offset + o.width
            && ry >= o.y_offset
            && ry < o.y_offset + o.height
    })
}

// ---------------------------------------------------------------------------
// Wall infill patterns
// ---------------------------------------------------------------------------

/// Context passed to wall infill for each non-opening cell.
pub struct InfillCell {
    /// World position of this cell.
    pub pos: Point3D,
    /// Index along the segment (0 = first cell).
    pub idx: u32,
    /// Row within the wall height (0 = floor level).
    pub ry: u32,
    /// Total wall height in blocks.
    pub height: u32,
    /// Length of the segment in blocks.
    pub seg_length: u32,
}

/// A wall infill pattern that controls what blocks are placed in wall panels.
pub enum WallInfill {
    /// Single material fills every cell.
    Solid,
    /// PrimaryStone for most of the wall, SecondaryStone on the bottom row.
    StoneBase,
}

impl WallInfill {
    async fn fill_segment(
        &self,
        editor: &Editor,
        seg: &WallSegment,
        cells: &[Point2D],
        data: &LoadedData,
        palette: &Palette,
        rng: &mut RNG,
    ) {
        match self {
            WallInfill::Solid => {
                let material_id = palette
                    .get_material(MaterialRole::PrimaryWall)
                    .expect("No primary wall material")
                    .clone();
                let mut placer_rng = rng.derive();
                let mut placer = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut placer_rng),
                    material_id,
                );

                for (idx, cell) in cells.iter().enumerate() {
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx as u32, ry) {
                            continue;
                        }
                        let y = seg.base_y + ry as i32;
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }
            }
            WallInfill::StoneBase => {
                let primary_id = palette
                    .get_material(MaterialRole::PrimaryStone)
                    .expect("No primary stone material")
                    .clone();
                let secondary_id = palette
                    .get_material(MaterialRole::SecondaryStone)
                    .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No stone material"))
                    .clone();
                let mut placer_rng = rng.derive();
                let mut secondary_rng = placer_rng.derive();
                let mut primary = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut placer_rng),
                    primary_id,
                );
                let mut secondary = MaterialPlacer::new(
                    Placer::new(&data.materials, &mut secondary_rng),
                    secondary_id,
                );

                for (idx, cell) in cells.iter().enumerate() {
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx as u32, ry) {
                            continue;
                        }
                        let y = seg.base_y + ry as i32;
                        let placer = if ry == 0 { &mut secondary } else { &mut primary };
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Block,
                            None,
                            None,
                        ).await;
                    }
                }
            }
        }
    }
}

/// Place wall infill blocks for all segments. Should be called BEFORE
/// place_frame and openings so the frame and openings can overwrite.
/// Accepts separate infill patterns for the ground floor and upper floors.
pub async fn place_wall_infill(
    ctx: &mut BuildCtx<'_>,
    wall_segs: &WallSegments,
    ground_infill: &WallInfill,
    upper_infill: &WallInfill,
) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);
        let infill = if seg.floor == 0 { ground_infill } else { upper_infill };
        infill.fill_segment(editor, seg, &cells, data, palette, rng).await;
    }
}

/// Place opening blocks (doors and windows) for all wall segments.
/// Doors use PrimaryWood with Door form and correct facing.
/// Windows use PrimaryWood with Fence form for now.
pub async fn place_openings(
    ctx: &mut BuildCtx<'_>,
    wall_segs: &WallSegments,
    window_fill: WindowFill,
) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let wood_material = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();
    let wall_material = palette
        .get_material(MaterialRole::PrimaryWall)
        .unwrap_or_else(|| palette.get_material(MaterialRole::PrimaryStone).expect("No wall or stone material"))
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        wood_material,
    );
    let mut wall_placer_rng = rng.derive();
    let mut wall_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut wall_placer_rng),
        wall_material,
    );

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);

        for opening in &seg.openings {
            match &opening.kind {
                OpeningKind::Door(_style) => {
                    let facing_str = seg.facing.to_string();
                    for dx in 0..opening.width {
                        let idx = (opening.offset + dx) as usize;
                        if idx >= cells.len() { continue; }
                        let cell = cells[idx];

                        let hinge = if dx == 0 { "right" } else { "left" };

                        // Lower half
                        let lower_state = HashMap::from([
                            ("facing".to_string(), facing_str.clone()),
                            ("half".to_string(), "lower".to_string()),
                            ("hinge".to_string(), hinge.to_string()),
                        ]);
                        let y = seg.base_y + opening.y_offset as i32;
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y, cell.y),
                            BlockForm::Door,
                            Some(&lower_state),
                            None,
                        ).await;

                        // Upper half
                        let upper_state = HashMap::from([
                            ("facing".to_string(), facing_str.clone()),
                            ("half".to_string(), "upper".to_string()),
                            ("hinge".to_string(), hinge.to_string()),
                        ]);
                        placer.place_block(
                            editor,
                            Point3D::new(cell.x, y + 1, cell.y),
                            BlockForm::Door,
                            Some(&upper_state),
                            None,
                        ).await;
                    }
                }
                OpeningKind::Window(style) => {
                    let is_arched = matches!(style, WindowStyle::Decorated | WindowStyle::Arched);
                    let top_dy = opening.height.saturating_sub(1);

                    for dx in 0..opening.width {
                        let idx = (opening.offset + dx) as usize;
                        if idx >= cells.len() { continue; }
                        let cell = cells[idx];
                        for dy in 0..opening.height {
                            let y = seg.base_y + opening.y_offset as i32 + dy as i32;
                            let pos = Point3D::new(cell.x, y, cell.y);

                            if is_arched && dy == top_dy {
                                let stair_facing = match style {
                                    WindowStyle::Arched => {
                                        let walk_dir = match seg.facing {
                                            Cardinal::South => Cardinal::East,
                                            Cardinal::North => Cardinal::West,
                                            Cardinal::East => Cardinal::North,
                                            Cardinal::West => Cardinal::South,
                                        };
                                        if dx == 0 { -walk_dir } else { walk_dir }
                                    }
                                    _ => seg.facing,
                                };
                                let state = HashMap::from([
                                    ("facing".to_string(), stair_facing.to_string()),
                                    ("half".to_string(), "top".to_string()),
                                ]);
                                wall_placer.place_block_forced(
                                    editor, pos, BlockForm::Stairs,
                                    Some(&state), None,
                                ).await;
                                continue;
                            }

                            match window_fill {
                                WindowFill::Glass => {
                                    editor.place_block_forced(
                                        &Block::from_id("minecraft:glass_pane".into()),
                                        pos,
                                    ).await;
                                }
                                WindowFill::Trapdoor => {
                                    let facing_str = (-seg.facing).to_string();
                                    let state = HashMap::from([
                                        ("facing".to_string(), facing_str),
                                        ("open".to_string(), "true".to_string()),
                                        ("half".to_string(), "bottom".to_string()),
                                    ]);
                                    placer.place_block_forced(
                                        editor, pos, BlockForm::Trapdoor,
                                        Some(&state), None,
                                    ).await;
                                }
                                WindowFill::Open => {
                                    editor.place_block_forced(
                                        &"air".into(),
                                        pos,
                                    ).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Returns the log axis state for a wall's facing direction.
/// The beam runs along the edge (perpendicular to facing).
fn axis_state(facing: Cardinal) -> HashMap<String, String> {
    let axis = match facing {
        Cardinal::East | Cardinal::West => "z",
        Cardinal::North | Cardinal::South => "x",
    };
    HashMap::from([("axis".to_string(), axis.to_string())])
}

/// Place the timber frame: vertical corner posts and horizontal crossbeams
/// at floor/ceiling levels along each edge. Uses WoodPillar role with Block form
/// and axis state to orient logs. `pattern` adds extra timber (vertical studs,
/// a mid-rail, corner knee braces) over the baseline skeleton — see
/// `TimberPattern` for the variants.
pub async fn place_frame(
    ctx: &mut BuildCtx<'_>,
    frame: &Frame,
    pattern: &TimberPattern,
) {
    let editor: &Editor = &*ctx.editor;
    let data = ctx.data;
    let palette = ctx.palette;
    let rng = &mut *ctx.rng;

    let pillar_id = palette
        .get_material(MaterialRole::WoodPillar)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("No wood pillar or primary wood material")
        .clone();

    // Non-pillar blocks (e.g. cut_sandstone) don't accept an `axis` blockstate,
    // and Minecraft will reject placements that specify one. Skip the axis state
    // unless the material is a pillar-style block.
    let supports_axis = material_supports_axis(pillar_id.as_str());

    let mut pillar_rng = rng.derive();
    let mut pillar_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut pillar_rng),
        pillar_id,
    );

    // Braces (panel diagonals + corner knee braces) use a contrasting plank
    // wood placed as stairs, so they read as lighter carpentry against the log
    // frame instead of thickening it. Falls back to the pillar wood if the
    // palette defines no plank role.
    let brace_id = palette
        .get_material(MaterialRole::PrimaryWood)
        .or_else(|| palette.get_material(MaterialRole::SecondaryWood))
        .or_else(|| palette.get_material(MaterialRole::WoodPillar))
        .expect("No wood material for braces")
        .clone();
    let mut brace_rng = rng.derive();
    let mut brace_placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut brace_rng),
        brace_id,
    );

    let wall_segs = build_segments(frame);

    // Track the lowest and highest floor each corner vertex appears on. The
    // lowest sets where the post starts (jettied upper corners hover above the
    // ground floor and must not drop logs into the overhang air below); the
    // highest sets where it ends.
    let mut corner_floor_range: HashMap<(i32, i32), (u32, u32)> = HashMap::new();

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);
        let beam_axis = axis_state(seg.facing);
        let beam_state = if supports_axis { Some(&beam_axis) } else { None };
        let y_axis_state: HashMap<String, String> =
            HashMap::from([("axis".to_string(), "y".to_string())]);
        let stud_state = if supports_axis { Some(&y_axis_state) } else { None };
        // Direction of increasing cell index along this segment. Brace stairs
        // face down-slope: a `/` (rising with idx) faces -walk_dir, a `\`
        // (falling with idx) faces +walk_dir — the gable-rake convention.
        let walk_dir = match seg.facing {
            Cardinal::South => Cardinal::East,
            Cardinal::North => Cardinal::West,
            Cardinal::East => Cardinal::North,
            Cardinal::West => Cardinal::South,
        };
        let floor_y = seg.base_y;
        let ceiling_y = seg.base_y + seg.height as i32;

        // Track corner vertex (first cell of each segment)
        if let Some(first) = cells.first() {
            let entry = corner_floor_range
                .entry((first.x, first.y))
                .or_insert((seg.floor, seg.floor));
            entry.0 = entry.0.min(seg.floor);
            entry.1 = entry.1.max(seg.floor);
        }

        // Crossbeams at floor and ceiling
        for cell in &cells {
            pillar_placer.place_block(
                editor,
                Point3D::new(cell.x, floor_y - 1, cell.y),
                BlockForm::Block,
                beam_state,
                None,
            ).await;
            pillar_placer.place_block(
                editor,
                Point3D::new(cell.x, ceiling_y, cell.y),
                BlockForm::Block,
                beam_state,
                None,
            ).await;
        }

        // Pattern extras (studs / mid-rail / corner braces) only apply to
        // upper floors — floor 0 uses the stone base infill and timber overlay
        // would clash with that. Baseline corner posts + crossbeams still go
        // on every floor.
        let apply_pattern = seg.floor > 0;

        // Vertical studs at regular spacing along the segment. Skip the
        // corner columns (they get a full post) and any rows inside an opening.
        if apply_pattern && pattern.has_studs() {
            for idx in stud_indices(seg.length as u32, pattern.spacing()) {
                if idx >= cells.len() as u32 { continue; }
                let cell = cells[idx as usize];
                for ry in 0..seg.height {
                    if is_inside_opening(&seg.openings, idx, ry) { continue; }
                    pillar_placer.place_block_forced(
                        editor,
                        Point3D::new(cell.x, floor_y + ry as i32, cell.y),
                        BlockForm::Block,
                        stud_state,
                        None,
                    ).await;
                }
            }
        }

        // Per-panel decorations: pick one uniform PanelShape for the whole
        // segment, then place stepped plank-stair braces (and optional extra
        // pillar columns) inside each panel. Diagonals step one column and one
        // row per cell — e.g. `\` from upper-left skips the top row then walks
        // (col+1, ry-1) until it hits the bottom or right edge of the panel.
        if apply_pattern && pattern.has_panel_decorations() && seg.length >= 4 && seg.height >= 2 {
            let studs: Vec<u32> = stud_indices(seg.length as u32, pattern.spacing());
            let spans = panel_spans(&studs, seg.length as u32);
            let mut seq_rng = rng.derive();
            let sequence = pick_panel_sequence(spans.len(), &mut seq_rng);
            let top_ry = seg.height - 1;

            for (span_idx, &(left, right)) in spans.iter().enumerate() {
                let shape = sequence.get(span_idx).copied().unwrap_or(PanelShape::Empty);
                if matches!(shape, PanelShape::Empty) { continue; }
                if right < left + 2 { continue; }
                let inner_left = left + 1;
                let inner_right = right - 1;
                let mid = (inner_left + inner_right) / 2;

                // (col, ry, rising): rising = the diagonal ascends as the cell
                // index increases (a `/`), so its stair faces down-slope toward
                // -walk_dir; a falling `\` faces +walk_dir.
                let mut diagonal_braces: Vec<(u32, u32, bool)> = Vec::new();
                let mut pillars: Vec<u32> = Vec::new();
                match shape {
                    PanelShape::Empty => {}
                    PanelShape::Back => {
                        // \ stepped diagonal from upper-left going down-right;
                        // skip the top row (sits under the ceiling beam).
                        let mut col = inner_left;
                        let mut ry = top_ry as i32 - 1;
                        while col <= inner_right && ry >= 0 {
                            diagonal_braces.push((col, ry as u32, false));
                            col += 1; ry -= 1;
                        }
                    }
                    PanelShape::Forward => {
                        // / stepped diagonal from lower-left going up-right;
                        // skip the bottom row (sits on the floor beam).
                        let mut col = inner_left;
                        let mut ry = 1u32;
                        while col <= inner_right && ry < seg.height {
                            diagonal_braces.push((col, ry, true));
                            col += 1; ry += 1;
                        }
                    }
                    PanelShape::Cross => {
                        // X — full \ + full /.
                        let mut col = inner_left;
                        let mut ry = top_ry as i32 - 1;
                        while col <= inner_right && ry >= 0 {
                            diagonal_braces.push((col, ry as u32, false));
                            col += 1; ry -= 1;
                        }
                        let mut col = inner_left;
                        let mut ry = 1u32;
                        while col <= inner_right && ry < seg.height {
                            diagonal_braces.push((col, ry, true));
                            col += 1; ry += 1;
                        }
                    }
                    PanelShape::Vee => {
                        // V — half-length \ on the left + half-length / on the
                        // right, both descending from the top corners toward the
                        // panel midpoint. Steps `half` cells in.
                        let panel_width = inner_right - inner_left + 1;
                        let half = panel_width.div_ceil(2);
                        let mut col = inner_left;
                        let mut ry = top_ry as i32 - 1;
                        for _ in 0..half {
                            if col > inner_right || ry < 0 { break; }
                            diagonal_braces.push((col, ry as u32, false));
                            col += 1; ry -= 1;
                        }
                        let mut col = inner_right as i32;
                        let mut ry = top_ry as i32 - 1;
                        for _ in 0..half {
                            if col < inner_left as i32 || ry < 0 { break; }
                            diagonal_braces.push((col as u32, ry as u32, true));
                            col -= 1; ry -= 1;
                        }
                    }
                    PanelShape::Chevron => {
                        // ^ — half-length / on the left + half-length \ on the
                        // right, both rising from the bottom corners toward the
                        // panel midpoint.
                        let panel_width = inner_right - inner_left + 1;
                        let half = panel_width.div_ceil(2);
                        let mut col = inner_left;
                        let mut ry = 1u32;
                        for _ in 0..half {
                            if col > inner_right || ry >= seg.height { break; }
                            diagonal_braces.push((col, ry, true));
                            col += 1; ry += 1;
                        }
                        let mut col = inner_right as i32;
                        let mut ry = 1u32;
                        for _ in 0..half {
                            if col < inner_left as i32 || ry >= seg.height { break; }
                            diagonal_braces.push((col as u32, ry, false));
                            col -= 1; ry += 1;
                        }
                    }
                    PanelShape::Pillar => {
                        pillars.push(mid);
                    }
                    PanelShape::KRight => {
                        // | + / on the right half.
                        pillars.push(mid);
                        if mid + 1 <= inner_right {
                            let mut col = mid + 1;
                            let mut ry = 1u32;
                            while col <= inner_right && ry < seg.height {
                                diagonal_braces.push((col, ry, true));
                                col += 1; ry += 1;
                            }
                        }
                    }
                    PanelShape::KLeft => {
                        // | + \ on the left half.
                        pillars.push(mid);
                        if mid > inner_left {
                            let left_inner_right = mid - 1;
                            let mut col = inner_left;
                            let mut ry = top_ry as i32 - 1;
                            while col <= left_inner_right && ry >= 0 {
                                diagonal_braces.push((col, ry as u32, false));
                                col += 1; ry -= 1;
                            }
                        }
                    }
                }

                for (idx, ry, rising) in diagonal_braces {
                    if (idx as usize) >= cells.len() { continue; }
                    if is_inside_opening(&seg.openings, idx, ry) { continue; }
                    let cell = cells[idx as usize];
                    let facing = if rising { -walk_dir } else { walk_dir };
                    let brace_state = HashMap::from([
                        ("facing".to_string(), facing.to_string()),
                        ("half".to_string(), "bottom".to_string()),
                    ]);
                    brace_placer.place_block_forced(
                        editor,
                        Point3D::new(cell.x, floor_y + ry as i32, cell.y),
                        BlockForm::Stairs,
                        Some(&brace_state),
                        None,
                    ).await;
                }
                for idx in pillars {
                    if (idx as usize) >= cells.len() { continue; }
                    let cell = cells[idx as usize];
                    for ry in 0..seg.height {
                        if is_inside_opening(&seg.openings, idx, ry) { continue; }
                        pillar_placer.place_block_forced(
                            editor,
                            Point3D::new(cell.x, floor_y + ry as i32, cell.y),
                            BlockForm::Block,
                            stud_state,
                            None,
                        ).await;
                    }
                }
            }
        }

        // Knee braces: stepped plank-stair diagonals near each segment corner,
        // just inside the corner post. Left corner gets a short `\` (falling
        // from idx=1, faces +walk_dir), right corner gets a mirrored `/`
        // (rising into idx=length-2, faces -walk_dir). Each brace is 2 cells
        // long, descending from one row below the ceiling.
        if apply_pattern && pattern.has_corner_braces() && seg.length >= 5 && seg.height >= 3 {
            let top_ry = seg.height - 1;
            // (col, ry, rising) — see the panel-decoration braces above.
            let knee_braces: Vec<(u32, u32, bool)> = vec![
                (1, top_ry - 1, false),
                (2, top_ry - 2, false),
                (seg.length as u32 - 2, top_ry - 1, true),
                (seg.length as u32 - 3, top_ry - 2, true),
            ];
            for (idx, ry, rising) in knee_braces {
                if (idx as usize) >= cells.len() { continue; }
                if is_inside_opening(&seg.openings, idx, ry) { continue; }
                let cell = cells[idx as usize];
                let facing = if rising { -walk_dir } else { walk_dir };
                let brace_state = HashMap::from([
                    ("facing".to_string(), facing.to_string()),
                    ("half".to_string(), "bottom".to_string()),
                ]);
                brace_placer.place_block_forced(
                    editor,
                    Point3D::new(cell.x, floor_y + ry as i32, cell.y),
                    BlockForm::Stairs,
                    Some(&brace_state),
                    None,
                ).await;
            }
        }
    }

    // Vertical corner posts (placed last to override crossbeams at intersections)
    let y_axis: HashMap<String, String> =
        HashMap::from([("axis".to_string(), "y".to_string())]);
    let post_state = if supports_axis { Some(&y_axis) } else { None };

    for (&(vx, vz), &(min_floor, max_floor)) in &corner_floor_range {
        // Floor-0 corners run from `base_y` so the post sits on the foundation
        // course. Upper-only corners (jetty overhang) start at the floor-level
        // crossbeam of their lowest floor — one below the floor surface — so
        // the post lines up flush with the wall above without dropping logs
        // into the air below the jetty.
        let bottom_y = if min_floor == 0 {
            frame.base_y()
        } else {
            frame.floor_y(min_floor) - 1
        };
        let top_y = frame.floor_y(max_floor) + frame.wall_height() as i32;
        for y in bottom_y..=top_y {
            pillar_placer.place_block_forced(
                editor,
                Point3D::new(vx, y, vz),
                BlockForm::Block,
                post_state,
                None,
            ).await;
        }
    }
}

/// Boundaries (left, right) of each panel between framing columns in a wall
/// segment of `length` cells with vertical studs at `stud_cols`. The corner
/// posts at column 0 and column length-1 cap the run. Panels with fewer than
/// one interior cell (right − left < 2) are dropped, so callers can safely
/// place decorations between left+1 and right-1 without bounds checks.
fn panel_spans(stud_cols: &[u32], length: u32) -> Vec<(u32, u32)> {
    if length < 4 { return Vec::new(); }
    let last = length - 1;
    let mut spans = Vec::new();
    let mut prev = 0u32;
    for &s in stud_cols {
        if s > prev + 1 { spans.push((prev, s)); }
        prev = s;
    }
    if last > prev + 1 { spans.push((prev, last)); }
    spans
}

/// Pick ONE motif and apply it uniformly across every panel of the wall, so a
/// long wall reads as a single coherent design instead of a chaotic mix of
/// shapes. The only within-wall variation is the alternating single brace
/// (`/ \ / \`), which flips lean panel-to-panel — still one design, just
/// rhythmic. Small walls get the plainer designs by virtue of the weighting;
/// the richer X / ^ / V motifs are the feature look for long façades.
fn pick_panel_sequence(panel_count: usize, rng: &mut RNG) -> Vec<PanelShape> {
    use PanelShape::*;
    if panel_count == 0 { return Vec::new(); }

    // Weighted design table: (design id, weight).
    //   0 empty (studs only)   1 alternating single brace   2 cross
    //   3 chevron              4 vee
    let table: &[(u8, u32)] = &[
        (0, 1),
        (1, 4),
        (2, 2),
        (3, 2),
        (4, 1),
    ];
    let total: u32 = table.iter().map(|(_, w)| w).sum();
    let mut roll = rng.rand_i32_range(0, total as i32) as u32;
    let mut design = 1u8;
    for (d, w) in table {
        if roll < *w { design = *d; break; }
        roll -= w;
    }

    (0..panel_count)
        .map(|i| match design {
            0 => Empty,
            1 => if i % 2 == 0 { Forward } else { Back },
            2 => Cross,
            3 => Chevron,
            _ => Vee,
        })
        .collect()
}

/// Symmetric stud column indices along a segment of `length` cells, with
/// adjacent studs `spacing` apart. Guarantees ≥2 infill cells between any
/// stud and either corner post (so no two vertical posts are adjacent and no
/// `C . S` either) and equal left/right margins. Picks the densest `n` that
/// fits; if no symmetric layout works for this length, returns empty
/// (segment stays Plain — minimum showable length is 7 at any spacing).
fn stud_indices(length: u32, spacing: u32) -> Vec<u32> {
    if length < 7 || spacing < 2 {
        return Vec::new();
    }
    // Max n where studs at p, p+s, … fit with p ≥ 3 and symmetric.
    // p = (length - 1 - (n-1)*s) / 2 ≥ 3  ⇒  (n-1)*s ≤ length - 7.
    let mut n_max = 1u32;
    while n_max * spacing <= length - 7 {
        n_max += 1;
    }
    for n in (1..=n_max).rev() {
        let span = (n - 1) * spacing;
        if (length - 1 - span) % 2 != 0 {
            continue;
        }
        let p = (length - 1 - span) / 2;
        if p < 3 {
            continue;
        }
        return (0..n).map(|i| p + i * spacing).collect();
    }
    Vec::new()
}

/// Returns true if a Minecraft block of the given material id accepts an
/// `axis` blockstate. This covers logs, stripped logs, pillars, stems,
/// hyphae, and a handful of axis-rotatable stone blocks.
fn material_supports_axis(id: &str) -> bool {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    if id.ends_with("_log")
        || id.ends_with("_wood")
        || id.ends_with("_stem")
        || id.ends_with("_hyphae")
        || id.ends_with("_pillar")
    {
        return true;
    }
    matches!(
        id,
        "basalt"
            | "polished_basalt"
            | "deepslate"
            | "bone_block"
            | "bamboo_block"
            | "stripped_bamboo_block"
            | "muddy_mangrove_roots"
            | "ochre_froglight"
            | "verdant_froglight"
            | "pearlescent_froglight"
    )
}
