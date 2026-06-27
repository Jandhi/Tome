//! The town's civic banner: its two colours flown on the wall towers and gates.
//!
//! One heraldic banner is minted from the settlement's two town colours (reusing
//! the manor heraldry generator, [`pick_family_banner`]) and hung facing OUTWARD
//! on the outer face of every wall tower and to either side of every gate, so the
//! town's colour identity reads from the approach. Returns the English blazon so
//! the chronicle can name the arms.
//!
//! Placement uses geometry recorded at build time ([`World::tower_bases`],
//! [`World::gate_locations`]) rather than reading blocks back — the editor block
//! cache is keyed in a different space than the pipeline's local coords, so a
//! freshly-placed wall reads as terrain (see `Editor::get_cached_block`).

use crate::editor::Editor;
use crate::generator::heraldry::pick_family_banner;
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::{string_to_block, Color};
use crate::noise::RNG;

/// Mint the civic banner and fly it on the towers + gates. `town` is the two town
/// colours (field, then charge); `centre` is the urban centroid (build-area
/// local), used to face each tower banner outward. Returns the blazon (e.g. "a red
/// cross on a white background") for the chronicle, or `None` if no banner design
/// loaded.
pub async fn place_civic_banners(
    editor: &mut Editor,
    centre: Point2D,
    town: [Color; 2],
    rng: &mut RNG,
) -> Option<String> {
    let banner = pick_family_banner(town[0], town[1], rng)?;
    let base: String = town[0].into();

    // Towers: one banner on the outward (away-from-centre) face of each tower.
    for (tower, support_y) in editor.world().tower_bases.clone() {
        let out = cardinal_away(tower, centre);
        let off: Point2D = out.into();
        // 5×5 base: the outer wall block sits two cells out, the banner one beyond.
        let air = tower + scale(off, 3);
        hang_banner(editor, Point3D::new(air.x, support_y, air.y), out, &base, &banner.data).await;
    }

    // Gates: a banner on the jamb either side of the opening, on BOTH faces of the
    // gate, so it reads from the approach and from inside the town. The stored gate
    // `dir` left the banners a quarter-turn off (mounted facing across the
    // passage), so the fixture is rotated one step CLOCKWISE about the gate centre:
    // `face` is `dir` turned clockwise. Each of the two faces (`face` and its
    // opposite) gets its own outward offset + facing.
    for (gate_point, dir) in editor.world().gate_locations.clone() {
        let opening = gate_point.drop_y();
        let y = gate_point.y + 3;
        let face = dir.rotate_right();
        for facing in [face, face.opposite()] {
            let off: Point2D = facing.into();
            let side: Point2D = facing.rotate_right().into();
            for k in [2, -2] {
                // One cell out from the jamb onto this face, facing out. The
                // no-update placement (see `hang_banner`) keeps that facing instead
                // of letting a block update re-orient it onto the passage side wall.
                let jamb = opening + scale(side, k);
                let air = jamb + off;
                hang_banner(editor, Point3D::new(air.x, y, air.y), facing, &base, &banner.data).await;
            }
        }
    }

    Some(banner.blazon)
}

/// Place one `<base>_wall_banner` carrying the civic pattern `data` at `air`,
/// facing `out` (the front of the banner points away from the wall behind it).
/// Placed with block updates OFF: a deep gate passage has no solid block directly
/// behind the banner in the outward direction, so a normal placement triggers an
/// update that re-orients the banner onto the passage side wall (facing sideways).
/// Skipping the update preserves the outward `facing` we set.
async fn hang_banner(
    editor: &Editor,
    air: Point3D,
    out: Cardinal,
    base_color: &str,
    data: &str,
) {
    let banner = format!("minecraft:{base_color}_wall_banner[facing={}]", out.to_string());
    let Some(mut block) = string_to_block(&banner) else { return };
    block.data = Some(data.to_string());
    editor.place_block_no_update(&block, air).await;
}

/// The cardinal pointing from the town `centre` out to `p` — the dominant axis, so
/// a tower on the east wall flies its banner facing east.
fn cardinal_away(p: Point2D, centre: Point2D) -> Cardinal {
    let dx = p.x - centre.x;
    let dz = p.y - centre.y;
    if dx.abs() >= dz.abs() {
        if dx >= 0 { Cardinal::East } else { Cardinal::West }
    } else if dz >= 0 {
        Cardinal::South
    } else {
        Cardinal::North
    }
}

fn scale(p: Point2D, k: i32) -> Point2D {
    Point2D::new(p.x * k, p.y * k)
}
