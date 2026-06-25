//! Sparse exterior wall decoration.
//!
//! Occasionally sets a household prop (barrel, pot, planter, firewood, …) on
//! the ground against the *outside* of a building's walls, so houses read as
//! lived-in rather than dropped models. Runs per building once the shell is
//! built. Tasteful by design: most houses get one or two props, never blocking
//! a door, a road, or another building.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::generator::BuildClaim;
use crate::generator::buildings::BuildingID;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Palette};
use crate::geometry::{Cardinal, Point2D, Point3D, CARDINALS_2D};
use crate::minecraft::{string_to_block, Block, BlockForm, Color};
use crate::noise::RNG;

use super::footprint::Footprint;
use super::pipeline::BuildCtx;
use super::walls::{segment_cells, WallSegments};

/// Hang a manor's family colour as wall banners flanking its front door, so the
/// street reads the household before you step inside. Placed on the solid wall
/// to either side of the ground-floor doorway (not above — an archway doorway
/// has no wall block over it to support a banner), facing out toward the street.
/// A no-op if the building has no ground-floor door.
pub async fn place_family_banner(
    ctx: &mut BuildCtx<'_>,
    wall_segs: &WallSegments,
    color: Color,
) {
    let Some((seg, opening)) = wall_segs.doors().find(|(s, _)| s.floor == 0) else {
        return;
    };
    let cells = segment_cells(seg);
    // `seg.facing` is the INWARD normal; the street side is its negation.
    let out: Point2D = (-seg.facing).into();
    let facing = (-seg.facing).to_string();
    let color_str: String = color.into();
    // The family's heraldic pattern (if any), stamped on both door banners so
    // they match the interior banners. `None` leaves them solid `color`.
    let banner_data = ctx.palette.banner_data.clone();
    // Mid-wall row: solid infill beside the door on every door height.
    let y = seg.base_y + 1;
    // The wall cell just left and just right of the doorway.
    let flanks = [opening.offset as i32 - 1, (opening.offset + opening.width) as i32];
    for idx in flanks {
        if idx < 0 || idx as usize >= cells.len() {
            continue;
        }
        let wall_cell = cells[idx as usize];
        // The banner hangs in the exterior air cell, supported by the wall block
        // behind it. Forced so a verge lip or terrain can't block the placement.
        let pos = wall_cell + out;
        let banner = format!("minecraft:{color_str}_wall_banner[facing={facing}]");
        let mut block = string_to_block(&banner).expect("family banner block");
        block.data = banner_data.clone();
        ctx.editor
            .place_block_forced(&block, Point3D::new(pos.x, y, pos.y))
            .await;
    }
}

/// One manor's pending name sign. The geometry + wood are worked out while the
/// building is fresh (its front door known), but the board can't be lettered
/// until the household's surname is rolled in the population pass — so the site
/// is held and lettered later by [`place_manor_sign`]. `anchor_idx` is the
/// manor's index in the town house list, used to look up its household.
pub struct ManorSignSite {
    pub anchor_idx: usize,
    sign_pos: Point3D,
    rotation: u8,
    wood: String,
    designation: String,
}

impl ManorSignSite {
    /// The designation line (e.g. "Manor", "Estate") — for logging.
    pub fn designation(&self) -> &str {
        &self.designation
    }
}

/// Work out where a manor's name sign hangs over its front door, or `None` if it
/// has no ground-floor door. Reuses the door the family banner flanks. The sign
/// is a ceiling hanging sign in the air cell just outside the door top, hanging
/// from whatever structure is above (wall overhang / jettied upper floor). The
/// board faces along the wall so it hangs **perpendicular to the wall** — edge-on
/// to the doorway, broadside to anyone coming up the street.
pub fn plan_manor_sign(
    wall_segs: &WallSegments,
    palette: &Palette,
    materials: &HashMap<MaterialId, Material>,
    rng: &mut RNG,
    anchor_idx: usize,
    designation: String,
) -> Option<ManorSignSite> {
    let (seg, opening) = wall_segs.doors().find(|(s, _)| s.floor == 0)?;
    let cells = segment_cells(seg);
    // The wall cell over the door's middle, then one step out toward the street.
    let mid = (opening.offset + opening.width / 2) as usize;
    let door_cell = *cells.get(mid)?;
    // `seg.facing` is the INWARD normal; the street side is its negation.
    let out: Point2D = (-seg.facing).into();
    let out_cell = door_cell + out;
    // Hang the board level with the door top (just above the opening).
    let door_top = seg.base_y + opening.y_offset as i32 + opening.height as i32;
    let sign_y = door_top;
    // Text faces along the wall (perpendicular to the street normal), so the board
    // hangs edge-on to the doorway and broadside to the street.
    let rotation = sign_rotation(seg.facing.rotate_right());
    let wood = wood_prefix(palette, materials, rng);
    Some(ManorSignSite {
        anchor_idx,
        sign_pos: Point3D::new(out_cell.x, sign_y, out_cell.y),
        rotation,
        wood,
        designation,
    })
}

/// Letter and place a planned manor sign now its family `name` is known. The
/// board reads blank / name / designation / blank so the two lines sit centred.
pub async fn place_manor_sign(editor: &Editor, site: &ManorSignSite, name: &str) {
    let face = format!(
        "{{messages:[{},{},{},{}]}}",
        sign_text(""),
        sign_text(name),
        sign_text(&site.designation),
        sign_text(""),
    );
    let data = format!("{{front_text:{face},back_text:{face}}}");
    let mut state = HashMap::new();
    state.insert("rotation".to_string(), site.rotation.to_string());
    // Attached = hangs rigidly from the block above (the bracket) on two chains.
    state.insert("attached".to_string(), "true".to_string());
    let sign = Block::new(
        format!("minecraft:{}_hanging_sign", site.wood).as_str().into(),
        Some(state),
        Some(data),
    );
    editor.place_block_forced(&sign, site.sign_pos).await;
}

/// Sign rotation (0-15) for the cardinal the front text should face. Mirrors
/// vanilla: south=0, west=4, north=8, east=12 (each step is 22.5°).
fn sign_rotation(facing: Cardinal) -> u8 {
    match facing {
        Cardinal::South => 0,
        Cardinal::West => 4,
        Cardinal::North => 8,
        Cardinal::East => 12,
    }
}

/// Wood types that have `<wood>_hanging_sign` / `<wood>_planks` blocks.
const SIGN_WOODS: [&str; 12] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove",
    "cherry", "pale_oak", "bamboo", "crimson", "warped",
];

/// PrimaryWood as a hanging-sign / planks prefix ("spruce", "dark_oak", …),
/// defaulting to oak if the palette's wood has no hanging-sign variant.
fn wood_prefix(palette: &Palette, materials: &HashMap<MaterialId, Material>, rng: &mut RNG) -> String {
    palette
        .get_block(MaterialRole::PrimaryWood, &BlockForm::Block, materials, rng)
        .map(|id| {
            id.as_str()
                .trim_start_matches("minecraft:")
                .trim_end_matches("_planks")
                .to_string()
        })
        .filter(|w| SIGN_WOODS.contains(&w.as_str()))
        .unwrap_or_else(|| "oak".to_string())
}

/// Wrap a string as a single-quoted SNBT literal for sign text, escaping `\` and
/// `'` so a name with an apostrophe can't break the sign data.
fn sign_text(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

/// Prop blocks placed against exterior walls — a varied, mostly single-block
/// set of everyday household clutter. Keep this list 10+ deep so a street shows
/// real variety.
const PROPS: [&str; 12] = [
    "minecraft:barrel[facing=up]",
    "minecraft:decorated_pot",
    "minecraft:hay_block",
    "minecraft:composter",
    "minecraft:cauldron",
    "minecraft:water_cauldron[level=3]",
    "minecraft:potted_cactus",
    "minecraft:potted_azalea_bush",
    "minecraft:potted_dead_bush",
    "minecraft:oak_log[axis=x]",
    "minecraft:lantern",
    "minecraft:flower_pot",
];

/// Weighted target count of props per building — average ~1.3, capped at 3,
/// often zero, so decoration stays occasional.
const TARGET_COUNTS: [u32; 6] = [0, 0, 1, 1, 2, 3];

/// Decorate the outside of a building's walls with a few sparse props.
pub async fn decorate_exterior_walls(
    ctx: &mut BuildCtx<'_>,
    footprint: &Footprint,
    wall_segs: &WallSegments,
) {
    let mut rng = ctx.rng.derive();

    let target = *rng.choose(&TARGET_COUNTS);
    if target == 0 {
        return;
    }

    // Cells just outside each door (plus the approach cell) — keep entrances clear.
    let mut avoid: HashSet<Point2D> = HashSet::new();
    for (seg, opening) in wall_segs.doors() {
        let cells = segment_cells(seg);
        // `seg.facing` is the wall's INWARD normal, so the *outside* of the door
        // (where exterior props would block the entrance) is its negation.
        let out: Point2D = (-seg.facing).into();
        for dx in 0..opening.width {
            if let Some(&cell) = cells.get((opening.offset + dx) as usize) {
                avoid.insert(cell + out);
                avoid.insert(cell + out * 2);
            }
        }
    }

    // The exterior ring: cells one step out from the footprint.
    let filled: HashSet<Point2D> = footprint.filled_points().into_iter().collect();
    let mut ring: HashSet<Point2D> = HashSet::new();
    for &c in &filled {
        for d in CARDINALS_2D {
            let ext = c + d;
            if !filled.contains(&ext) {
                ring.insert(ext);
            }
        }
    }
    // Sort to a deterministic order (Point2D isn't Ord), then shuffle via RNG.
    let mut candidates: Vec<Point2D> = ring.into_iter().collect();
    candidates.sort_by_key(|p| (p.x, p.y));
    shuffle(&mut candidates, &mut rng);

    // Claim placed props as part of this building so a later building or road
    // never overwrites them (same id the footprint claim uses).
    let building_idx = ctx.editor.world().buildings.len();

    let mut placed: Vec<Point2D> = Vec::new();
    for cell in candidates {
        if placed.len() >= target as usize {
            break;
        }
        if avoid.contains(&cell) || !is_open_ground(ctx.editor, cell) {
            continue;
        }
        // Spread props out so two never sit side by side.
        if placed.iter().any(|p| p.distance_manhattan(&cell) < 3) {
            continue;
        }

        let prop = pick_prop(&mut rng);
        let Some(y) = ctx.editor.world().get_height_at(cell) else {
            continue;
        };
        ctx.editor.place_block(&prop, Point3D::new(cell.x, y, cell.y)).await;
        ctx.editor
            .world_mut()
            .claim(cell, BuildClaim::Building(BuildingID(building_idx)));
        placed.push(cell);
    }
}

/// A cell is a good prop spot if it's in bounds, unclaimed open ground (not a
/// road, wall, structure, or another building), and sits on solid, non-water
/// ground.
fn is_open_ground(editor: &Editor, cell: Point2D) -> bool {
    let world = editor.world();
    if !world.is_in_bounds_2d(cell) {
        return false;
    }
    if !matches!(world.get_claim(cell), Some(BuildClaim::None | BuildClaim::Nature)) {
        return false;
    }
    // The block the prop will stand on (one below the placement Y).
    let Some(y) = world.get_height_at(cell) else {
        return false;
    };
    match world.get_block(Point3D::new(cell.x, y - 1, cell.y)) {
        Some(b) => {
            let id = b.id.as_str();
            !b.id.is_water() && id != "minecraft:air" && id != "air"
        }
        None => false,
    }
}

fn pick_prop(rng: &mut RNG) -> Block {
    let s = *rng.choose(&PROPS);
    string_to_block(s).unwrap_or_else(|| Block::from_id(s.into()))
}

fn shuffle<T>(items: &mut [T], rng: &mut RNG) {
    for i in (1..items.len()).rev() {
        let j = rng.rand_i32_range(0, (i + 1) as i32) as usize;
        items.swap(i, j);
    }
}
