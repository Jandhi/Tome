//! Wall segments: the per-floor, per-edge straight runs that every other wall
//! pass operates on. Defines the segment data types (segments, openings, door
//! and window styles) and the geometry that turns a frame's floor outlines into
//! segments with the right facing and concave-corner overlaps.

use crate::geometry::{Cardinal, Point2D};

use super::super::footprint::merge::walk_edge_cells;
use super::super::frame::Frame;

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

/// Check if a position (cell index along segment, relative y) falls inside any opening.
pub(super) fn is_inside_opening(openings: &[Opening], idx: u32, ry: u32) -> bool {
    openings.iter().any(|o| {
        idx >= o.offset
            && idx < o.offset + o.width
            && ry >= o.y_offset
            && ry < o.y_offset + o.height
    })
}
