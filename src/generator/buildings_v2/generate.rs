use crate::noise::RNG;

use super::{DoorType, Frame, Opening, WallSegment, WindowType};

/// Configuration for door generation.
#[derive(Debug, Clone)]
pub struct DoorRules {
    /// Minimum number of doors (at least 1).
    pub min_count: u32,
    /// Maximum number of doors per building.
    pub max_count: u32,
    /// Whether to prefer centering doors on walls.
    pub prefer_symmetry: bool,
    /// Default door type to use.
    pub default_type: DoorType,
}

impl Default for DoorRules {
    fn default() -> Self {
        Self {
            min_count: 1,
            max_count: 2,
            prefer_symmetry: true,
            default_type: DoorType::Single,
        }
    }
}

/// Result of door generation for a building.
#[derive(Debug, Clone)]
pub struct DoorPlacements {
    /// Index of the wall segment and the opening to add.
    pub placements: Vec<(usize, Opening)>,
}

/// Check if a wall segment can fit a door of the given type.
fn can_fit_door(segment: &WallSegment, door_type: DoorType) -> bool {
    let length = segment.length();
    let min_distance = door_type.min_corner_distance();
    let door_width = door_type.width();

    // Need: min_distance on each side + door width
    length >= min_distance * 2 + door_width
}

/// Find the best position for a door on a wall segment.
/// Returns the position along the wall, or None if no valid position exists.
fn find_door_position(
    segment: &WallSegment,
    door_type: DoorType,
    prefer_symmetry: bool,
    rng: &mut RNG,
) -> Option<i32> {
    let length = segment.length();
    let min_distance = door_type.min_corner_distance();
    let door_width = door_type.width();

    // Calculate valid range for door position
    let min_pos = min_distance;
    let max_pos = length - min_distance - door_width;

    if min_pos > max_pos {
        return None; // Wall is too short
    }

    // Check for overlaps with existing openings
    let find_valid_position = |pos: i32| -> bool {
        let candidate = Opening::new(
            super::OpeningKind::Door(door_type),
            pos,
            door_width,
            door_type.height(),
            0,
        );
        !segment.openings.iter().any(|o| candidate.overlaps(o))
    };

    if prefer_symmetry {
        // Try to center the door
        let center = (min_pos + max_pos) / 2;
        if find_valid_position(center) {
            return Some(center);
        }
    }

    // Try random positions
    for _ in 0..10 {
        let pos = rng.rand_i32_range(min_pos, max_pos + 1);
        if find_valid_position(pos) {
            return Some(pos);
        }
    }

    // Fallback: try all positions
    for pos in min_pos..=max_pos {
        if find_valid_position(pos) {
            return Some(pos);
        }
    }

    None
}

/// Generate door placements for a building frame.
/// Returns wall segment indices and openings to add.
pub fn generate_doors(
    frame: &Frame,
    rules: &DoorRules,
    rng: &mut RNG,
) -> DoorPlacements {
    let segments = frame.wall_segments();
    let mut placements = Vec::new();

    // Find walls that can fit doors
    let mut eligible_walls: Vec<(usize, &WallSegment)> = segments
        .iter()
        .enumerate()
        .filter(|(_, seg)| can_fit_door(seg, rules.default_type))
        .collect();

    if eligible_walls.is_empty() {
        return DoorPlacements { placements };
    }

    // Determine how many doors to place
    let door_count = rng.rand_i32_range(rules.min_count as i32, rules.max_count as i32 + 1) as u32;
    let door_count = door_count.min(eligible_walls.len() as u32);

    // Shuffle eligible walls for random selection
    for i in (1..eligible_walls.len()).rev() {
        let j = rng.rand_i32_range(0, i as i32 + 1) as usize;
        eligible_walls.swap(i, j);
    }

    // Place doors
    for i in 0..door_count as usize {
        let (wall_idx, segment) = eligible_walls[i];

        if let Some(pos) = find_door_position(segment, rules.default_type, rules.prefer_symmetry, rng) {
            let opening = match rules.default_type {
                DoorType::Single => Opening::single_door(pos),
                DoorType::Double => Opening::double_door(pos),
                DoorType::Archway => Opening::archway(pos),
            };
            placements.push((wall_idx, opening));
        }
    }

    DoorPlacements { placements }
}

/// Apply door placements to wall segments.
/// This modifies the segments in place by adding the door openings.
pub fn apply_door_placements(
    segments: &mut [WallSegment],
    placements: &DoorPlacements,
) {
    for (wall_idx, opening) in &placements.placements {
        if *wall_idx < segments.len() {
            // Ignore errors if opening doesn't fit (shouldn't happen if generated correctly)
            let _ = segments[*wall_idx].add_opening(opening.clone());
        }
    }
}

/// Generate doors and apply them directly to a frame.
/// This is a convenience function that combines generate_doors and apply_door_placements.
pub fn add_doors_to_frame(
    frame: &mut Frame,
    rules: &DoorRules,
    rng: &mut RNG,
) {
    let placements = generate_doors(frame, rules, rng);
    apply_door_placements(frame.wall_segments_mut(), &placements);
}

/// Configuration for window generation.
#[derive(Debug, Clone)]
pub struct WindowRules {
    /// Density of windows (0.0-1.0), representing the proportion of available space to fill.
    pub density: f32,
    /// Whether to prefer symmetric/centered placements.
    pub prefer_symmetry: bool,
    /// Whether to use the same window type for an entire floor.
    pub consistent_type: bool,
    /// Default window type to use.
    pub default_type: WindowType,
}

impl Default for WindowRules {
    fn default() -> Self {
        Self {
            density: 0.5,
            prefer_symmetry: true,
            consistent_type: true,
            default_type: WindowType::Small,
        }
    }
}

/// Result of window generation for a building.
#[derive(Debug, Clone)]
pub struct WindowPlacements {
    /// (wall_index, floor, opening) tuples.
    pub placements: Vec<(usize, u32, Opening)>,
}

/// Check if a wall segment can fit a window of the given type at a specific position.
fn can_fit_window_at(
    segment: &WallSegment,
    window_type: WindowType,
    position: i32,
) -> bool {
    let length = segment.length();
    let min_distance = window_type.min_corner_distance();
    let window_width = window_type.width();

    // Check position is valid
    if position < min_distance || position + window_width > length - min_distance {
        return false;
    }

    // Check for overlaps with existing openings (including spacing)
    let candidate = Opening::new(
        super::OpeningKind::Window(window_type),
        position,
        window_width,
        window_type.height(),
        window_type.y_offset(),
    );

    for existing in &segment.openings {
        // Calculate required spacing
        let spacing = window_type.min_spacing();
        
        // Check if candidate is too close to existing opening
        if candidate.position < existing.end_position() + spacing 
            && existing.position < candidate.end_position() + spacing {
            return false;
        }
    }

    true
}

/// Find valid positions for windows on a wall segment.
/// Returns a list of valid positions where windows can be placed.
fn find_window_positions(
    segment: &WallSegment,
    window_type: WindowType,
    count: usize,
    prefer_symmetry: bool,
    rng: &mut RNG,
) -> Vec<i32> {
    let length = segment.length();
    let min_distance = window_type.min_corner_distance();
    let window_width = window_type.width();
    let spacing = window_type.min_spacing();

    let mut positions = Vec::new();

    // Calculate valid range
    let min_pos = min_distance;
    let max_pos = length - min_distance - window_width;

    if min_pos > max_pos {
        return positions; // Wall too short
    }

    if prefer_symmetry && count > 0 {
        // Try to distribute windows evenly
        let available_space = max_pos - min_pos;
        let space_per_window = (available_space as f32) / (count as f32 + 1.0);

        for i in 0..count {
            let pos = min_pos + ((i + 1) as f32 * space_per_window) as i32;
            
            // Verify position is valid
            if can_fit_window_at(segment, window_type, pos) {
                positions.push(pos);
                
                // Temporarily add this window to check future overlaps
                // Note: This is a simplified check; a proper implementation would
                // clone the segment and add the opening
            }
        }
    } else {
        // Random placement
        let mut attempts = 0;
        while positions.len() < count && attempts < count * 10 {
            let pos = rng.rand_i32_range(min_pos, max_pos + 1);
            
            // Check if this position works
            if can_fit_window_at(segment, window_type, pos) {
                // Also check it doesn't overlap with already-placed windows in this generation
                let mut overlaps = false;
                for &existing_pos in &positions {
                    if (pos - existing_pos).abs() < window_width + spacing {
                        overlaps = true;
                        break;
                    }
                }
                
                if !overlaps {
                    positions.push(pos);
                }
            }
            
            attempts += 1;
        }
    }

    positions
}

/// Generate window placements for a building frame.
/// Windows are placed on all floors (unlike doors which are ground floor only).
pub fn generate_windows(
    frame: &Frame,
    rules: &WindowRules,
    rng: &mut RNG,
) -> WindowPlacements {
    let segments = frame.wall_segments();
    let mut placements = Vec::new();

    // Determine window type per floor if consistent_type is enabled
    let mut floor_window_types = Vec::new();
    if rules.consistent_type {
        for _ in 0..frame.floors {
            floor_window_types.push(rules.default_type);
        }
    }

    // Generate windows for each floor
    for floor in 0..frame.floors {
        let window_type = if rules.consistent_type {
            floor_window_types[floor as usize]
        } else {
            rules.default_type
        };

        // Process each wall segment
        for (wall_idx, segment) in segments.iter().enumerate() {
            let length = segment.length();
            let min_distance = window_type.min_corner_distance();
            let window_width = window_type.width();

            // Calculate available space (accounting for corners and existing openings)
            let mut available_space = length - 2 * min_distance;
            
            // Subtract space taken by existing openings (mainly doors on ground floor)
            for opening in &segment.openings {
                available_space -= opening.width + window_type.min_spacing();
            }

            if available_space < window_width {
                continue; // Not enough space
            }

            // Calculate how many windows to place based on density
            let max_windows = available_space / (window_width + window_type.min_spacing());
            let window_count = ((max_windows as f32) * rules.density).ceil() as usize;

            if window_count == 0 {
                continue;
            }

            // Find positions for windows
            let positions = find_window_positions(
                segment,
                window_type,
                window_count,
                rules.prefer_symmetry,
                rng,
            );

            // Create openings for each position
            for pos in positions {
                let opening = match window_type {
                    WindowType::Small => Opening::small_window(pos),
                    WindowType::Tall => Opening::tall_window(pos),
                    WindowType::Wide => Opening::wide_window(pos),
                    WindowType::Large => Opening::large_window(pos),
                };
                placements.push((wall_idx, floor, opening));
            }
        }
    }

    WindowPlacements { placements }
}

/// Apply window placements to wall segments.
/// This modifies the segments in place by adding the window openings.
pub fn apply_window_placements(
    segments: &mut [WallSegment],
    placements: &WindowPlacements,
) {
    for (wall_idx, _floor, opening) in &placements.placements {
        if *wall_idx < segments.len() {
            let _ = segments[*wall_idx].add_opening(opening.clone());
        }
    }
}

/// Generate windows and apply them directly to a frame.
pub fn add_windows_to_frame(
    frame: &mut Frame,
    rules: &WindowRules,
    rng: &mut RNG,
) {
    let placements = generate_windows(frame, rules, rng);
    apply_window_placements(frame.wall_segments_mut(), &placements);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::buildings_v2::Footprint;

    #[test]
    fn test_generate_doors_basic() {
        let footprint = Footprint::rectangle(crate::geometry::Point2D::new(0, 0), 10, 10);
        let frame = Frame::new(footprint, 0, 4, 1);
        let rules = DoorRules::default();
        let mut rng = RNG::new(42);

        let placements = generate_doors(&frame, &rules, &mut rng);

        // Should have at least one door
        assert!(!placements.placements.is_empty());
        assert!(placements.placements.len() >= rules.min_count as usize);
    }

    #[test]
    fn test_door_respects_corner_distance() {
        let footprint = Footprint::rectangle(crate::geometry::Point2D::new(0, 0), 10, 10);
        let frame = Frame::new(footprint, 0, 4, 1);
        let rules = DoorRules::default();
        let mut rng = RNG::new(42);

        let placements = generate_doors(&frame, &rules, &mut rng);

        for (_, opening) in &placements.placements {
            // Door should be at least 2 blocks from start
            assert!(opening.position >= 2);
        }
    }

    #[test]
    fn test_small_wall_no_door() {
        // Wall too small for a door (needs 2 + 1 + 2 = 5 blocks minimum for single door)
        let footprint = Footprint::rectangle(crate::geometry::Point2D::new(0, 0), 4, 4);
        let frame = Frame::new(footprint, 0, 4, 1);
        let rules = DoorRules::default();
        let mut rng = RNG::new(42);

        let _placements = generate_doors(&frame, &rules, &mut rng);

        // May or may not have doors depending on wall lengths
        // A 4x4 building has walls of length 4, which is just barely not enough
        // (needs min_distance=2 on each side + width=1 = 5)
    }
}
