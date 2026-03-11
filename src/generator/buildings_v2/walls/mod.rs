#[cfg(test)]
mod test;

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use super::footprint::merge::walk_edge_cells;
use super::frame::Frame;

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
    Small, // 1 wide, 1 tall
    Tall,  // 1 wide, 2 tall
    Wide,  // 2 wide, 2 tall
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
pub fn place_doors(wall_segs: &mut WallSegments, plot_bounds: &Rect2D, footprint_area: i32, _rng: &mut RNG) {
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

    // Score ground-floor segments by distance to nearest plot edge (lower = better)
    let mut scored: Vec<(usize, i32)> = wall_segs
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.floor == 0 && s.length >= min_segment_len)
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

/// Place windows on all wall segments. Even spacing, denser on upper floors.
/// Skips positions that overlap with existing doors.
pub fn place_windows(wall_segs: &mut WallSegments, _rng: &mut RNG) {
    let corner_margin: u32 = 1;

    for seg_idx in 0..wall_segs.segments.len() {
        let seg = &wall_segs.segments[seg_idx];
        let seg_len = seg.length as u32;

        if seg_len < corner_margin * 2 + 3 {
            continue; // too short for any window
        }

        let available = seg_len - corner_margin * 2;

        // Window style based on segment length
        let (style, win_width, win_height) = if available < 5 {
            (WindowStyle::Small, 1u32, 1u32)
        } else {
            (WindowStyle::Tall, 1, 2)
        };

        // Spacing: ground floor is sparser
        let spacing = if seg.floor == 0 { 5u32 } else { 4u32 };
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
            if overlaps {
                continue;
            }

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

/// Place wall infill blocks for all segments. Should be called BEFORE
/// place_frame and openings so the frame and openings can overwrite.
pub async fn place_wall_infill(
    editor: &Editor,
    wall_segs: &WallSegments,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let material_id = palette
        .get_material(MaterialRole::PrimaryWall)
        .expect("No primary wall material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);
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
}

/// Place opening blocks (doors and windows) for all wall segments.
/// Doors use PrimaryWood with Door form and correct facing.
/// Windows use PrimaryWood with Fence form for now.
pub async fn place_openings(
    editor: &Editor,
    wall_segs: &WallSegments,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let material_id = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);

        for opening in &seg.openings {
            match &opening.kind {
                OpeningKind::Door(style) => {
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
                OpeningKind::Window(_style) => {
                    for dx in 0..opening.width {
                        let idx = (opening.offset + dx) as usize;
                        if idx >= cells.len() { continue; }
                        let cell = cells[idx];
                        for dy in 0..opening.height {
                            let y = seg.base_y + opening.y_offset as i32 + dy as i32;
                            placer.place_block(
                                editor,
                                Point3D::new(cell.x, y, cell.y),
                                BlockForm::Fence,
                                None,
                                None,
                            ).await;
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
/// and axis state to orient logs.
pub async fn place_frame(
    editor: &Editor,
    frame: &Frame,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let material_id = palette
        .get_material(MaterialRole::WoodPillar)
        .or_else(|| palette.get_material(MaterialRole::PrimaryWood))
        .expect("No wood pillar or primary wood material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    let wall_segs = build_segments(frame);

    // Track the highest floor each corner vertex appears on (for post height)
    let mut corner_max_floor: HashMap<(i32, i32), u32> = HashMap::new();

    for seg in &wall_segs.segments {
        let cells = segment_cells(seg);
        let beam_axis = axis_state(seg.facing);
        let floor_y = seg.base_y;
        let ceiling_y = seg.base_y + seg.height as i32;

        // Track corner vertex (first cell of each segment)
        if let Some(first) = cells.first() {
            let entry = corner_max_floor.entry((first.x, first.y)).or_insert(0);
            *entry = (*entry).max(seg.floor);
        }

        // Crossbeams at floor and ceiling
        for cell in &cells {
            placer.place_block(
                editor,
                Point3D::new(cell.x, floor_y - 1, cell.y),
                BlockForm::Block,
                Some(&beam_axis),
                None,
            ).await;
            placer.place_block(
                editor,
                Point3D::new(cell.x, ceiling_y, cell.y),
                BlockForm::Block,
                Some(&beam_axis),
                None,
            ).await;
        }
    }

    // Vertical corner posts (placed last to override crossbeams at intersections)
    let y_axis: HashMap<String, String> =
        HashMap::from([("axis".to_string(), "y".to_string())]);

    for (&(vx, vz), &max_floor) in &corner_max_floor {
        let top_y = frame.floor_y(max_floor) + frame.wall_height() as i32;
        for y in frame.base_y()..=top_y {
            placer.place_block_forced(
                editor,
                Point3D::new(vx, y, vz),
                BlockForm::Block,
                Some(&y_axis),
                None,
            ).await;
        }
    }
}
