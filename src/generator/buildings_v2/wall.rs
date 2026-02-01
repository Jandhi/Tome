use crate::geometry::Point2D;

/// Specific door variants with their dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorType {
    /// Standard single door (1 wide, 2 tall)
    Single,
    /// Double door for grand entrances (2 wide, 2 tall)
    Double,
    /// Open archway passage, no door block (2-3 wide, 3 tall)
    Archway,
}

impl DoorType {
    /// Get the width of this door type in blocks.
    pub fn width(&self) -> i32 {
        match self {
            DoorType::Single => 1,
            DoorType::Double => 2,
            DoorType::Archway => 2, // Default archway width, can be 2-3
        }
    }

    /// Get the height of this door type in blocks.
    pub fn height(&self) -> i32 {
        match self {
            DoorType::Single => 2,
            DoorType::Double => 2,
            DoorType::Archway => 3,
        }
    }

    /// Whether this door type uses actual door blocks (vs being an open passage).
    pub fn has_door_block(&self) -> bool {
        match self {
            DoorType::Single => true,
            DoorType::Double => true,
            DoorType::Archway => false,
        }
    }

    /// Minimum distance from corner required for this door type.
    pub fn min_corner_distance(&self) -> i32 {
        2 // All doors need at least 2 blocks from corners
    }
}

/// Specific window variants with their dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Basic 1x1 window
    Small,
    /// Tall 1x2 window (vertical emphasis)
    Tall,
    /// Wide 2x1 window (horizontal emphasis)
    Wide,
    /// Large 2x2 statement window
    Large,
}

impl WindowType {
    /// Get the width of this window type in blocks.
    pub fn width(&self) -> i32 {
        match self {
            WindowType::Small => 1,
            WindowType::Tall => 1,
            WindowType::Wide => 2,
            WindowType::Large => 2,
        }
    }

    /// Get the height of this window type in blocks.
    pub fn height(&self) -> i32 {
        match self {
            WindowType::Small => 1,
            WindowType::Tall => 2,
            WindowType::Wide => 1,
            WindowType::Large => 2,
        }
    }

    /// Get the vertical offset from floor level for this window type.
    pub fn y_offset(&self) -> i32 {
        1 // All windows start 1 block above the floor
    }

    /// Minimum distance from corner required for this window type.
    pub fn min_corner_distance(&self) -> i32 {
        1 // Windows need at least 1 block from corners
    }

    /// Minimum spacing between this window and other openings.
    pub fn min_spacing(&self) -> i32 {
        1 // At least 1 block between windows
    }
}

/// The type of opening in a wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningKind {
    Door(DoorType),
    Window(WindowType),
}

/// An opening (door, window, etc.) in a wall segment.
#[derive(Debug, Clone)]
pub struct Opening {
    /// The type of opening.
    pub kind: OpeningKind,
    /// Distance along the wall from the start point (in blocks).
    pub position: i32,
    /// Width of the opening (in blocks).
    pub width: i32,
    /// Height of the opening (in blocks).
    pub height: i32,
    /// Vertical offset from the floor (in blocks).
    pub y_offset: i32,
}

impl Opening {
    /// Create a new opening.
    pub fn new(kind: OpeningKind, position: i32, width: i32, height: i32, y_offset: i32) -> Self {
        Self {
            kind,
            position,
            width,
            height,
            y_offset,
        }
    }

    /// Create a standard single door (1 wide, 2 tall, at floor level).
    pub fn door(position: i32) -> Self {
        Self::single_door(position)
    }

    /// Create a single door (1 wide, 2 tall, at floor level).
    pub fn single_door(position: i32) -> Self {
        let door_type = DoorType::Single;
        Self::new(
            OpeningKind::Door(door_type),
            position,
            door_type.width(),
            door_type.height(),
            0,
        )
    }

    /// Create a double door (2 wide, 2 tall, at floor level).
    pub fn double_door(position: i32) -> Self {
        let door_type = DoorType::Double;
        Self::new(
            OpeningKind::Door(door_type),
            position,
            door_type.width(),
            door_type.height(),
            0,
        )
    }

    /// Create an archway (2 wide, 3 tall, at floor level).
    pub fn archway(position: i32) -> Self {
        let door_type = DoorType::Archway;
        Self::new(
            OpeningKind::Door(door_type),
            position,
            door_type.width(),
            door_type.height(),
            0,
        )
    }

    /// Create an archway with custom width (2-3 wide, 3 tall, at floor level).
    pub fn archway_wide(position: i32, width: i32) -> Self {
        Self::new(
            OpeningKind::Door(DoorType::Archway),
            position,
            width.clamp(2, 3),
            DoorType::Archway.height(),
            0,
        )
    }

    /// Create a standard window (1 wide, 1 tall, 1 block above floor).
    pub fn window(position: i32) -> Self {
        Self::small_window(position)
    }

    /// Create a small window (1x1, 1 block above floor).
    pub fn small_window(position: i32) -> Self {
        let window_type = WindowType::Small;
        Self::new(
            OpeningKind::Window(window_type),
            position,
            window_type.width(),
            window_type.height(),
            window_type.y_offset(),
        )
    }

    /// Create a tall window (1x2, 1 block above floor).
    pub fn tall_window(position: i32) -> Self {
        let window_type = WindowType::Tall;
        Self::new(
            OpeningKind::Window(window_type),
            position,
            window_type.width(),
            window_type.height(),
            window_type.y_offset(),
        )
    }

    /// Create a wide window (2x1, 1 block above floor).
    pub fn wide_window(position: i32) -> Self {
        let window_type = WindowType::Wide;
        Self::new(
            OpeningKind::Window(window_type),
            position,
            window_type.width(),
            window_type.height(),
            window_type.y_offset(),
        )
    }

    /// Create a large window (2x2, 1 block above floor).
    pub fn large_window(position: i32) -> Self {
        let window_type = WindowType::Large;
        Self::new(
            OpeningKind::Window(window_type),
            position,
            window_type.width(),
            window_type.height(),
            window_type.y_offset(),
        )
    }

    /// Check if this opening is a door.
    pub fn is_door(&self) -> bool {
        matches!(self.kind, OpeningKind::Door(_))
    }

    /// Check if this opening is a window.
    pub fn is_window(&self) -> bool {
        matches!(self.kind, OpeningKind::Window(_))
    }

    /// Get the door type if this is a door opening.
    pub fn door_type(&self) -> Option<DoorType> {
        match self.kind {
            OpeningKind::Door(dt) => Some(dt),
            _ => None,
        }
    }

    /// Get the window type if this is a window opening.
    pub fn window_type(&self) -> Option<WindowType> {
        match self.kind {
            OpeningKind::Window(wt) => Some(wt),
            _ => None,
        }
    }

    /// Get the end position of this opening along the wall.
    pub fn end_position(&self) -> i32 {
        self.position + self.width
    }

    /// Check if this opening overlaps with another.
    pub fn overlaps(&self, other: &Opening) -> bool {
        self.position < other.end_position() && other.position < self.end_position()
    }
}

/// A single wall segment between two corners.
#[derive(Debug, Clone)]
pub struct WallSegment {
    /// Starting corner (2D position in XZ plane).
    pub start: Point2D,
    /// Ending corner (2D position in XZ plane).
    pub end: Point2D,
    /// Openings in this wall (doors, windows, etc.).
    pub openings: Vec<Opening>,
}

impl WallSegment {
    /// Create a new wall segment between two points.
    pub fn new(start: Point2D, end: Point2D) -> Self {
        Self {
            start,
            end,
            openings: Vec::new(),
        }
    }

    /// Get the length of this wall segment in blocks.
    pub fn length(&self) -> i32 {
        let dx = (self.end.x - self.start.x).abs();
        let dz = (self.end.y - self.start.y).abs();
        // For axis-aligned walls, one of these will be 0
        dx.max(dz)
    }

    /// Get the direction vector from start to end.
    /// For axis-aligned walls, this will be a unit vector.
    pub fn direction(&self) -> Point2D {
        let dx = self.end.x - self.start.x;
        let dz = self.end.y - self.start.y;
        let len = self.length();

        if len == 0 {
            return Point2D::ZERO;
        }

        // Normalize to unit steps (for axis-aligned, this gives -1, 0, or 1)
        Point2D::new(dx.signum(), dz.signum())
    }

    /// Check if this wall is axis-aligned (parallel to X or Z axis).
    pub fn is_axis_aligned(&self) -> bool {
        self.start.x == self.end.x || self.start.y == self.end.y
    }

    /// Check if this wall runs along the X axis (constant Z).
    pub fn is_x_aligned(&self) -> bool {
        self.start.y == self.end.y
    }

    /// Check if this wall runs along the Z axis (constant X).
    pub fn is_z_aligned(&self) -> bool {
        self.start.x == self.end.x
    }

    /// Add an opening to this wall segment.
    /// Returns an error if the opening doesn't fit or overlaps with existing openings.
    pub fn add_opening(&mut self, opening: Opening) -> Result<(), WallError> {
        // Check if opening fits within wall bounds
        if opening.position < 0 || opening.end_position() > self.length() {
            return Err(WallError::OpeningOutOfBounds);
        }

        // Check for overlaps with existing openings
        for existing in &self.openings {
            if opening.overlaps(existing) {
                return Err(WallError::OpeningOverlap);
            }
        }

        self.openings.push(opening);
        Ok(())
    }

    /// Get all positions along the wall as 2D points.
    /// Useful for iterating over each block position in the wall.
    pub fn positions(&self) -> Vec<Point2D> {
        let len = self.length();
        if len == 0 {
            return vec![self.start];
        }

        let dir = self.direction();
        (0..=len)
            .map(|i| Point2D::new(self.start.x + dir.x * i, self.start.y + dir.y * i))
            .collect()
    }

    /// Check if a position along the wall (0 to length) is blocked by an opening.
    /// y_offset is the height above the floor being checked.
    pub fn is_opening_at(&self, position: i32, y_offset: i32) -> bool {
        for opening in &self.openings {
            if position >= opening.position
                && position < opening.end_position()
                && y_offset >= opening.y_offset
                && y_offset < opening.y_offset + opening.height
            {
                return true;
            }
        }
        false
    }

    /// Get the opening at a specific position, if any.
    pub fn opening_at(&self, position: i32, y_offset: i32) -> Option<&Opening> {
        self.openings.iter().find(|o| {
            position >= o.position
                && position < o.end_position()
                && y_offset >= o.y_offset
                && y_offset < o.y_offset + o.height
        })
    }
}

/// Errors that can occur when working with wall segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WallError {
    /// The opening extends beyond the wall bounds.
    OpeningOutOfBounds,
    /// The opening overlaps with an existing opening.
    OpeningOverlap,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wall_length() {
        // X-aligned wall
        let wall = WallSegment::new(Point2D::new(0, 5), Point2D::new(10, 5));
        assert_eq!(wall.length(), 10);

        // Z-aligned wall
        let wall = WallSegment::new(Point2D::new(5, 0), Point2D::new(5, 8));
        assert_eq!(wall.length(), 8);
    }

    #[test]
    fn test_wall_direction() {
        // East-facing wall
        let wall = WallSegment::new(Point2D::new(0, 0), Point2D::new(10, 0));
        assert_eq!(wall.direction(), Point2D::new(1, 0));

        // North-facing wall
        let wall = WallSegment::new(Point2D::new(0, 10), Point2D::new(0, 0));
        assert_eq!(wall.direction(), Point2D::new(0, -1));
    }

    #[test]
    fn test_is_axis_aligned() {
        let aligned = WallSegment::new(Point2D::new(0, 0), Point2D::new(10, 0));
        assert!(aligned.is_axis_aligned());
        assert!(aligned.is_x_aligned());
        assert!(!aligned.is_z_aligned());

        let diagonal = WallSegment::new(Point2D::new(0, 0), Point2D::new(5, 5));
        assert!(!diagonal.is_axis_aligned());
    }

    #[test]
    fn test_add_opening() {
        let mut wall = WallSegment::new(Point2D::new(0, 0), Point2D::new(10, 0));

        // Valid opening
        assert!(wall.add_opening(Opening::door(3)).is_ok());

        // Overlapping opening
        assert_eq!(
            wall.add_opening(Opening::door(3)),
            Err(WallError::OpeningOverlap)
        );

        // Out of bounds
        assert_eq!(
            wall.add_opening(Opening::door(10)),
            Err(WallError::OpeningOutOfBounds)
        );
    }

    #[test]
    fn test_positions() {
        let wall = WallSegment::new(Point2D::new(0, 5), Point2D::new(3, 5));
        let positions = wall.positions();

        assert_eq!(positions.len(), 4);
        assert_eq!(positions[0], Point2D::new(0, 5));
        assert_eq!(positions[1], Point2D::new(1, 5));
        assert_eq!(positions[2], Point2D::new(2, 5));
        assert_eq!(positions[3], Point2D::new(3, 5));
    }

    #[test]
    fn test_is_opening_at() {
        let mut wall = WallSegment::new(Point2D::new(0, 0), Point2D::new(10, 0));
        wall.add_opening(Opening::large_window(5)).unwrap();

        // Inside the window
        assert!(wall.is_opening_at(5, 1));
        assert!(wall.is_opening_at(6, 2));

        // Outside the window
        assert!(!wall.is_opening_at(4, 1));
        assert!(!wall.is_opening_at(5, 0));
        assert!(!wall.is_opening_at(5, 3));
    }
}
