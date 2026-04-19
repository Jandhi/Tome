use std::fmt::Write;

use crate::geometry::{Point2D, Rect2D};
use super::floors::{FloorPlan, StairKind};
use super::footprint::find_boundaries;
use super::frame::Frame;
use super::rooms::{ConstraintMap, CellState, RoomPlan, RoomRole};
use super::walls::{segment_cells, WallSegments, OpeningKind};
use super::RoomType;

// ---------------------------------------------------------------------------
// Blueprint data model
// ---------------------------------------------------------------------------

pub struct Blueprint {
    pub floors: Vec<BlueprintFloor>,
}

pub struct BlueprintFloor {
    pub floor_index: u32,
    pub is_attic: bool,
    pub rooms: Vec<BlueprintRoom>,
    pub exterior_walls: Vec<Vec<Point2D>>,   // wall segment cell lists
    pub interior_walls: Vec<Vec<Point2D>>,   // boundary wall cell lists
    pub doors: Vec<BlueprintOpening>,
    pub windows: Vec<BlueprintOpening>,
    pub interior_doors: Vec<Point2D>,
    pub stairs: Vec<BlueprintStair>,
    pub outline: Vec<Point2D>,
}

pub struct BlueprintRoom {
    pub rect: Rect2D,
    pub interior: Rect2D,
    pub room_type: RoomType,
    pub role: RoomRole,
    pub furniture: Vec<BlueprintFurniture>,
    /// Clone of the room's constraint map. Kept so the ASCII renderer can
    /// surface BlockedReachable cells and other constraint state.
    pub constraints: ConstraintMap,
}

pub struct BlueprintFurniture {
    pub name: String,
    pub cells: Vec<(i32, i32)>,
}

pub struct BlueprintOpening {
    pub cell: Point2D,
    pub kind: String,
}

pub struct BlueprintStair {
    pub positions: Vec<Point2D>,
    pub kind: StairKind,
}

// ---------------------------------------------------------------------------
// Build blueprint from pipeline outputs
// ---------------------------------------------------------------------------

pub fn build_blueprint(
    frame: &Frame,
    wall_segs: &WallSegments,
    floor_plan: &FloorPlan,
    room_plan: &RoomPlan,
    has_attic: bool,
) -> Blueprint {
    let rects = frame.footprint().rects();
    let boundaries = find_boundaries(rects);

    let max_floor = if has_attic {
        frame.max_floors()
    } else {
        frame.max_floors() - 1
    };

    let mut floors = Vec::new();

    for floor in 0..=max_floor {
        let is_attic = has_attic && floor == max_floor;

        // Rooms on this floor
        let rooms: Vec<BlueprintRoom> = room_plan.rooms.iter()
            .filter(|r| r.floor == floor)
            .map(|r| BlueprintRoom {
                rect: r.rect,
                interior: r.interior,
                room_type: r.room_type,
                role: r.role,
                constraints: r.constraints.clone(),
                furniture: r.furniture.iter().map(|f| BlueprintFurniture {
                    name: f.name.clone(),
                    cells: f.cells.clone(),
                }).collect(),
            })
            .collect();

        // Exterior wall segments on this floor
        let exterior_walls: Vec<Vec<Point2D>> = wall_segs.segments_on_floor(floor)
            .map(|seg| segment_cells(seg))
            .collect();

        // Interior walls between active rects on this floor
        let active = frame.active_rects(floor);
        let interior_walls: Vec<Vec<Point2D>> = boundaries.iter()
            .filter(|b| active.contains(&b.rect_a) && active.contains(&b.rect_b))
            .map(|b| b.wall_cells.clone())
            .collect();

        // Exterior doors/windows on this floor
        let mut doors = Vec::new();
        let mut windows = Vec::new();
        for seg in wall_segs.segments_on_floor(floor) {
            let cells = segment_cells(seg);
            for opening in &seg.openings {
                let idx = opening.offset as usize;
                if idx >= cells.len() { continue; }
                let cell = cells[idx];
                match &opening.kind {
                    OpeningKind::Door(_) => doors.push(BlueprintOpening {
                        cell,
                        kind: "door".into(),
                    }),
                    OpeningKind::Window(_) => windows.push(BlueprintOpening {
                        cell,
                        kind: "window".into(),
                    }),
                }
            }
        }

        // Interior doors on this floor
        let interior_doors: Vec<Point2D> = room_plan.interior_doors.iter()
            .filter(|(f, _, _, _)| *f == floor)
            .map(|(_, _, _, cell)| *cell)
            .collect();

        // Stairs starting on this floor
        let stairs: Vec<BlueprintStair> = floor_plan.stairwells_on_floor(floor).iter()
            .map(|sw| BlueprintStair {
                positions: sw.positions.clone(),
                kind: sw.kind,
            })
            .collect();

        // Outline
        let outline = frame.outline_at_floor(floor);

        floors.push(BlueprintFloor {
            floor_index: floor,
            is_attic,
            rooms,
            exterior_walls,
            interior_walls,
            doors,
            windows,
            interior_doors,
            stairs,
            outline,
        });
    }

    Blueprint { floors }
}

// ---------------------------------------------------------------------------
// SVG renderer
// ---------------------------------------------------------------------------

const CELL_SIZE: f32 = 16.0;
const FLOOR_GAP: f32 = 40.0;
const PADDING: f32 = 20.0;

fn room_color(room_type: RoomType) -> &'static str {
    match room_type {
        RoomType::Common => "#f5e6c8",
        RoomType::Hearth | RoomType::GreatRoom => "#f5d0a0",
        RoomType::Bedroom | RoomType::MultiBedroom | RoomType::MasterBedroom => "#c8daf5",
        RoomType::Kitchen => "#f5c8c8",
        RoomType::Storage | RoomType::Pantry => "#d9cbb8",
        RoomType::Study | RoomType::Library => "#c8f5d0",
        RoomType::Dining => "#f5f0c8",
        RoomType::Studio | RoomType::Armory => "#d8c8f5",
    }
}

fn furniture_color(name: &str) -> &'static str {
    match name {
        "bed" => "#6688cc",
        "crafting_table" => "#aa8844",
        "furnace" | "smoker" => "#cc6644",
        "chest" | "barrel" => "#ccaa44",
        "bookshelf" => "#886633",
        "lantern" => "#ffcc44",
        "anvil" => "#888888",
        "cauldron" => "#666666",
        "loom" => "#aa88aa",
        _ => "#999999",
    }
}

pub fn render_svg(blueprint: &Blueprint) -> String {
    if blueprint.floors.is_empty() {
        return String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
    }

    // Compute global bounding box across all floors
    let mut global_min_x = i32::MAX;
    let mut global_min_z = i32::MAX;
    let mut global_max_x = i32::MIN;
    let mut global_max_z = i32::MIN;

    for floor in &blueprint.floors {
        for p in &floor.outline {
            global_min_x = global_min_x.min(p.x);
            global_min_z = global_min_z.min(p.y);
            global_max_x = global_max_x.max(p.x);
            global_max_z = global_max_z.max(p.y);
        }
    }

    let building_w = (global_max_x - global_min_x + 1) as f32 * CELL_SIZE;
    let building_h = (global_max_z - global_min_z + 1) as f32 * CELL_SIZE;

    let num_floors = blueprint.floors.len() as f32;
    let total_w = building_w * num_floors + FLOOR_GAP * (num_floors - 1.0) + PADDING * 2.0;
    let total_h = building_h + PADDING * 2.0 + 30.0; // 30 for floor labels

    let mut svg = String::new();
    let _ = write!(svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.0} {:.0}\" \
         width=\"{:.0}\" height=\"{:.0}\" style=\"background:#ffffff\">\n",
        total_w, total_h, total_w, total_h
    );

    // Font style
    let _ = write!(svg, "<style>\n\
        text {{ font-family: 'Segoe UI', Arial, sans-serif; }}\n\
        .room-label {{ font-size: 10px; fill: #333; text-anchor: middle; dominant-baseline: central; }}\n\
        .floor-label {{ font-size: 13px; fill: #000; font-weight: bold; text-anchor: middle; }}\n\
        .furn-label {{ font-size: 7px; fill: #fff; text-anchor: middle; dominant-baseline: central; }}\n\
    </style>\n");

    for (fi, floor) in blueprint.floors.iter().enumerate() {
        let offset_x = PADDING + fi as f32 * (building_w + FLOOR_GAP);
        let offset_z = PADDING + 20.0; // room for floor label

        let _ = write!(svg, "<g transform=\"translate({:.1},{:.1})\">\n", offset_x, offset_z);

        // Floor label
        let label = if floor.is_attic {
            "Attic".to_string()
        } else {
            format!("Floor {}", floor.floor_index)
        };
        let _ = write!(svg, "  <text x=\"{:.1}\" y=\"-8\" class=\"floor-label\">{}</text>\n",
            building_w / 2.0, label);

        // Room fills
        for room in &floor.rooms {
            let rx = (room.rect.min().x - global_min_x) as f32 * CELL_SIZE;
            let rz = (room.rect.min().y - global_min_z) as f32 * CELL_SIZE;
            let rw = room.rect.size.x as f32 * CELL_SIZE;
            let rh = room.rect.size.y as f32 * CELL_SIZE;
            let color = room_color(room.room_type);

            let _ = write!(svg,
                "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                 fill=\"{}\" stroke=\"#ccc\" stroke-width=\"0.5\"/>\n",
                rx, rz, rw, rh, color);

            // Room type label
            let cx = rx + rw / 2.0;
            let cz = rz + rh / 2.0;
            let _ = write!(svg,
                "  <text x=\"{:.1}\" y=\"{:.1}\" class=\"room-label\">{}</text>\n",
                cx, cz, room.room_type.name());
        }

        // Exterior walls
        for wall_cells in &floor.exterior_walls {
            for cell in wall_cells {
                let cx = (cell.x - global_min_x) as f32 * CELL_SIZE;
                let cz = (cell.y - global_min_z) as f32 * CELL_SIZE;
                let _ = write!(svg,
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                     fill=\"#333\"/>\n",
                    cx, cz, CELL_SIZE, CELL_SIZE);
            }
        }

        // Interior walls
        for wall_cells in &floor.interior_walls {
            for cell in wall_cells {
                let cx = (cell.x - global_min_x) as f32 * CELL_SIZE;
                let cz = (cell.y - global_min_z) as f32 * CELL_SIZE;
                let _ = write!(svg,
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                     fill=\"#555\"/>\n",
                    cx, cz, CELL_SIZE, CELL_SIZE);
            }
        }

        // Wall slots: cells the interior-edge logic treats as "against a wall".
        // Drawn as orange ticks on the implied wall side of each cell. If a tick
        // has no actual wall/#555/#333 block on the outside, the slot is a
        // phantom wall — furniture placed there will float.
        const TICK: f32 = 2.0;
        const SLOT_COLOR: &str = "#ff8800";
        for room in &floor.rooms {
            let interior = room.interior;
            if interior.size.x <= 0 || interior.size.y <= 0 { continue; }
            let imin = interior.min();
            let imax = interior.max();
            for cell in interior.iter() {
                let cx = (cell.x - global_min_x) as f32 * CELL_SIZE;
                let cz = (cell.y - global_min_z) as f32 * CELL_SIZE;
                if cell.x == imin.x {
                    let _ = write!(svg,
                        "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"{}\"/>\n",
                        cx, cz, TICK, CELL_SIZE, SLOT_COLOR);
                }
                if cell.x == imax.x {
                    let _ = write!(svg,
                        "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"{}\"/>\n",
                        cx + CELL_SIZE - TICK, cz, TICK, CELL_SIZE, SLOT_COLOR);
                }
                if cell.y == imin.y {
                    let _ = write!(svg,
                        "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"{}\"/>\n",
                        cx, cz, CELL_SIZE, TICK, SLOT_COLOR);
                }
                if cell.y == imax.y {
                    let _ = write!(svg,
                        "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"{}\"/>\n",
                        cx, cz + CELL_SIZE - TICK, CELL_SIZE, TICK, SLOT_COLOR);
                }
            }
        }

        // Windows
        for win in &floor.windows {
            let wx = (win.cell.x - global_min_x) as f32 * CELL_SIZE + CELL_SIZE * 0.2;
            let wz = (win.cell.y - global_min_z) as f32 * CELL_SIZE + CELL_SIZE * 0.2;
            let _ = write!(svg,
                "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                 fill=\"#88bbee\" stroke=\"#4488aa\" stroke-width=\"0.5\"/>\n",
                wx, wz, CELL_SIZE * 0.6, CELL_SIZE * 0.6);
        }

        // Exterior doors
        for door in &floor.doors {
            let dx = (door.cell.x - global_min_x) as f32 * CELL_SIZE + CELL_SIZE * 0.1;
            let dz = (door.cell.y - global_min_z) as f32 * CELL_SIZE + CELL_SIZE * 0.1;
            let _ = write!(svg,
                "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                 fill=\"#aa6633\" stroke=\"#774422\" stroke-width=\"1\"/>\n",
                dx, dz, CELL_SIZE * 0.8, CELL_SIZE * 0.8);
        }

        // Interior doors
        for cell in &floor.interior_doors {
            let dx = (cell.x - global_min_x) as f32 * CELL_SIZE + CELL_SIZE * 0.15;
            let dz = (cell.y - global_min_z) as f32 * CELL_SIZE + CELL_SIZE * 0.15;
            let _ = write!(svg,
                "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                 fill=\"#cc9966\" stroke=\"#996633\" stroke-width=\"0.5\" rx=\"2\"/>\n",
                dx, dz, CELL_SIZE * 0.7, CELL_SIZE * 0.7);
        }

        // Stairs
        for stair in &floor.stairs {
            if stair.positions.is_empty() { continue; }
            let color = match stair.kind {
                StairKind::Straight => "#aaa",
                StairKind::Spiral => "#999",
                StairKind::LShaped => "#bbb",
            };
            for pos in &stair.positions {
                let sx = (pos.x - global_min_x) as f32 * CELL_SIZE + 1.0;
                let sz = (pos.y - global_min_z) as f32 * CELL_SIZE + 1.0;
                let _ = write!(svg,
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                     fill=\"{}\" stroke=\"#666\" stroke-width=\"0.5\"/>\n",
                    sx, sz, CELL_SIZE - 2.0, CELL_SIZE - 2.0, color);
            }
            // Arrow showing direction (first to last position)
            let first = &stair.positions[0];
            let last = stair.positions.last().unwrap();
            let ax1 = (first.x - global_min_x) as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            let az1 = (first.y - global_min_z) as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            let ax2 = (last.x - global_min_x) as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            let az2 = (last.y - global_min_z) as f32 * CELL_SIZE + CELL_SIZE / 2.0;
            let _ = write!(svg,
                "  <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke=\"#333\" stroke-width=\"1.5\" marker-end=\"url(#arrow)\"/>\n",
                ax1, az1, ax2, az2);
        }

        // Furniture
        for room in &floor.rooms {
            for furn in &room.furniture {
                let color = furniture_color(&furn.name);
                for &(fx, fz) in &furn.cells {
                    let px = (fx - global_min_x) as f32 * CELL_SIZE + 2.0;
                    let pz = (fz - global_min_z) as f32 * CELL_SIZE + 2.0;
                    let _ = write!(svg,
                        "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
                         fill=\"{}\" rx=\"2\" opacity=\"0.85\"/>\n",
                        px, pz, CELL_SIZE - 4.0, CELL_SIZE - 4.0, color);
                    // Label (first 3 chars of name)
                    let short: String = furn.name.chars().take(3).collect();
                    let _ = write!(svg,
                        "  <text x=\"{:.1}\" y=\"{:.1}\" class=\"furn-label\">{}</text>\n",
                        px + (CELL_SIZE - 4.0) / 2.0, pz + (CELL_SIZE - 4.0) / 2.0, short);
                }
            }
        }

        let _ = write!(svg, "</g>\n");
    }

    // Arrow marker definition
    let _ = write!(svg, "<defs>\n\
        <marker id=\"arrow\" markerWidth=\"6\" markerHeight=\"6\" refX=\"5\" refY=\"3\" orient=\"auto\">\n\
        <path d=\"M0,0 L6,3 L0,6 Z\" fill=\"#333\"/>\n\
        </marker>\n\
    </defs>\n");

    // Legend
    let legend_y = total_h - 15.0;
    let _ = write!(svg, "<text x=\"{:.1}\" y=\"{:.1}\" style=\"font-size:9px;fill:#666\">\
        Legend: ", PADDING, legend_y);
    let legend_items = [
        ("#aa6633", "Door"), ("#88bbee", "Window"), ("#aaa", "Stairs"),
        ("#ff8800", "Wall slot"),
    ];
    let mut lx = PADDING + 50.0;
    for (color, label) in &legend_items {
        let _ = write!(svg,
            "</text><rect x=\"{:.1}\" y=\"{:.1}\" width=\"10\" height=\"10\" fill=\"{}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" style=\"font-size:9px;fill:#666\">{} ",
            lx, legend_y - 9.0, color, lx + 13.0, legend_y, label);
        lx += 60.0;
    }
    let _ = write!(svg, "</text>\n");

    svg.push_str("</svg>\n");
    svg
}

// ---------------------------------------------------------------------------
// ASCII renderer
// ---------------------------------------------------------------------------

/// Single-character code for a furniture item, for ASCII rendering.
fn furniture_char(name: &str) -> char {
    match name {
        "bed" => 'B',
        "chest" => 'C',
        "crafting_table" => 'T',
        "furnace" => 'F',
        "lantern" => 'l',
        "bookshelf" => 'K',
        "barrel" => 'R',
        "anvil" => 'A',
        "cauldron" => 'U',
        "smoker" => 'S',
        "loom" => 'M',
        "table" => 'X',
        "flower_pot" => 'P',
        "carpet" | "carpet_runner" | "rug" => '~',
        "nightstand" => 'N',
        "chair" => 'H',
        "desk" => 'E',
        "shelf" => 'k',
        "vase" => 'V',
        "candle" => 'c',
        "banner" => 'b',
        "crate" => 'r',
        _ => name.chars().next().unwrap_or('?'),
    }
}

/// Stair direction arrow based on the first two positions of a stairwell.
fn stair_arrow(first: Point2D, second: Point2D) -> char {
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    match (dx.signum(), dy.signum()) {
        (1, 0) => '>',
        (-1, 0) => '<',
        (0, 1) => 'v',
        (0, -1) => '^',
        _ => '/',
    }
}

/// Render a Blueprint as ASCII art. Floors stack vertically with a header.
/// Use for terminal inspection / test output where SVGs are awkward.
///
/// Legend:
///   `#`  exterior wall      `W`  window
///   `%`  interior wall      `D`  exterior door
///   `.`  empty interior     `d`  interior door
///   `*`  BlockedReachable (door/stair/furniture approach)
///   ` `  outside building   `^v<>/`  stair cells (first step arrow)
///
/// Furniture characters (see `furniture_char`): `B`ed, `C`hest, `T`able,
/// `F`urnace, `l`antern, `K` bookshelf, `R` barrel, `X` table, `E` desk, etc.
pub fn render_ascii(blueprint: &Blueprint) -> String {
    if blueprint.floors.is_empty() {
        return String::new();
    }

    // Global bounding box across all floors
    let mut min_x = i32::MAX;
    let mut min_z = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_z = i32::MIN;
    for floor in &blueprint.floors {
        for p in &floor.outline {
            min_x = min_x.min(p.x);
            min_z = min_z.min(p.y);
            max_x = max_x.max(p.x);
            max_z = max_z.max(p.y);
        }
    }
    if min_x == i32::MAX {
        return String::new();
    }
    let width = (max_x - min_x + 1) as usize;
    let height = (max_z - min_z + 1) as usize;

    let mut out = String::new();

    for floor in &blueprint.floors {
        let mut grid: Vec<Vec<char>> = vec![vec![' '; width]; height];
        let put = |grid: &mut Vec<Vec<char>>, x: i32, z: i32, ch: char| {
            let lx = (x - min_x) as isize;
            let lz = (z - min_z) as isize;
            if lx >= 0 && lz >= 0 && (lx as usize) < width && (lz as usize) < height {
                grid[lz as usize][lx as usize] = ch;
            }
        };

        // Floor base fill: every cell inside any room rect becomes '.'
        for room in &floor.rooms {
            for cell in room.rect.iter() {
                put(&mut grid, cell.x, cell.y, '.');
            }
        }
        // Reserved cells (BlockedReachable, door approaches, stair landings,
        // furniture fronts) marked with `*`. Drawn after base fill so they
        // show through, but before walls/furniture which override them.
        for room in &floor.rooms {
            for ((cx, cz), state) in room.constraints.iter_ground() {
                if state == CellState::BlockedReachable {
                    put(&mut grid, cx, cz, '*');
                }
            }
        }
        // Walls (exterior + interior)
        for wall in &floor.exterior_walls {
            for cell in wall {
                put(&mut grid, cell.x, cell.y, '#');
            }
        }
        for wall in &floor.interior_walls {
            for cell in wall {
                put(&mut grid, cell.x, cell.y, '%');
            }
        }
        // Openings override walls
        for win in &floor.windows {
            put(&mut grid, win.cell.x, win.cell.y, 'W');
        }
        for door in &floor.doors {
            put(&mut grid, door.cell.x, door.cell.y, 'D');
        }
        for cell in &floor.interior_doors {
            put(&mut grid, cell.x, cell.y, 'd');
        }
        // Stairs: mark every cell, with the first showing an arrow toward the ascent
        for stair in &floor.stairs {
            if stair.positions.is_empty() { continue; }
            let arrow = if stair.positions.len() >= 2 {
                stair_arrow(stair.positions[0], stair.positions[1])
            } else { '/' };
            put(&mut grid, stair.positions[0].x, stair.positions[0].y, arrow);
            for pos in stair.positions.iter().skip(1) {
                put(&mut grid, pos.x, pos.y, '/');
            }
        }
        // Furniture overrides everything below it
        for room in &floor.rooms {
            for furn in &room.furniture {
                let ch = furniture_char(&furn.name);
                for &(fx, fz) in &furn.cells {
                    put(&mut grid, fx, fz, ch);
                }
            }
        }

        // Header
        let label = if floor.is_attic {
            "Attic".to_string()
        } else {
            format!("Floor {}", floor.floor_index)
        };
        out.push_str(&format!("{}:\n", label));

        // Column numbers (two-digit, second digit row)
        out.push_str("     ");
        for x in min_x..=max_x {
            out.push(char::from_digit((x.rem_euclid(10)) as u32, 10).unwrap_or('?'));
        }
        out.push('\n');

        // Room annotations: print abbreviated room type at the top-left corner of each room
        // (as a side note, not inside the grid since it would collide with walls).

        for (row_idx, row) in grid.iter().enumerate() {
            let world_z = min_z + row_idx as i32;
            out.push_str(&format!(" {:3} ", world_z));
            for &c in row { out.push(c); }
            out.push('\n');
        }

        // Room list footer
        if !floor.rooms.is_empty() {
            out.push_str("  Rooms: ");
            let names: Vec<String> = floor.rooms.iter()
                .map(|r| format!("{}@({},{})", r.room_type.name(), r.rect.min().x, r.rect.min().y))
                .collect();
            out.push_str(&names.join(", "));
            out.push('\n');
        }
        out.push('\n');
    }

    out
}
