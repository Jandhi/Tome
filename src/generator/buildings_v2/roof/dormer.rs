//! Gable dormers on pitched gable roofs: a small gable standing on the eave
//! wall line, its ridge running out from the main slope, with a window in its
//! front face.
//!
//! A dormer is planned per eave side of each rect, in its own coordinates:
//! `d` along the eave (0 at the dormer's centre, cheeks at `±1`) and `s` in
//! from the eave (0 on the wall line, `-1` the overhang row). Its roof stands
//! at a fixed height above the eave (its `rise` at the ridge, one less on the
//! cheeks, two less on the side eaves a cell wider) and runs back up the slope
//! until the main roof reaches it. The face and the cells behind it are
//! *owned*: the main roof leaves them out (`DormerPlan::owned`) and
//! [`place_dormers`] builds them; the side eaves sit on top of the main roof.
//!
//! Heights are relative to the group's (pitch-adjusted) `roof_y`, as in
//! [`super::gable::gable_heightmap`]: rel `-1` is the top of the wall (the
//! ceiling row under a stair roof, the eave block under a double one), rel `0`
//! the eave stair.

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::{Block, BlockForm};
use crate::noise::RNG;
use crate::util::HashSet;

use super::gable::{GablePitch, RidgeAxis};

/// Dormer ridge height above the eave (rel to `roof_y`): a two-row window,
/// a row of wall above it, then the ridge slab. (A two-row dormer with a
/// one-row window was tried for low roofs and reads as a bump, not a dormer.)
const RISE: i32 = 3;
/// Cells kept clear between a dormer's cheek and the rect's gable wall.
const END_MARGIN: i32 = 2;
/// Cells kept between two dormers' faces (their roofs, a cell wider each
/// side, keep two cells of main roof between them).
const GAP: i32 = 4;

/// One dormer, in the coordinates of the rect it sits on.
#[derive(Debug, Clone)]
pub struct Dormer {
    pub rect: usize,
    axis: RidgeAxis,
    /// Position of the dormer's centre along the ridge axis.
    along: i32,
    /// Short-axis coordinate of the eave wall line it stands on.
    eave: i32,
    /// +1 when the eave is on the rect's max side, -1 on its min side
    /// (the direction from the wall line out of the house).
    out: i32,
    pitch: GablePitch,
    /// Ridge height above the eave (`RISE`).
    rise: i32,
}

impl Dormer {
    /// World cell at `d` along the eave and `s` in from it.
    fn cell(&self, d: i32, s: i32) -> Point2D {
        let short = self.eave - self.out * s;
        match self.axis {
            RidgeAxis::X => Point2D::new(self.along + d, short),
            RidgeAxis::Z => Point2D::new(short, self.along + d),
        }
    }

    /// The row the main roof's surface block takes `s` cells in from the
    /// eave (as `place_roof_blocks` lays it: a slab roof sits half a row low).
    fn main_h(&self, s: i32) -> i32 {
        let lower = if matches!(self.pitch, GablePitch::Slab) { 0.5 } else { 0.0 };
        (s as f32 * self.pitch.value() - lower).floor() as i32
    }

    /// The dormer roof's height at `d` (ridge 0, cheeks ±1, side eaves ±2).
    fn roof_h(&self, d: i32) -> i32 {
        self.rise - d.abs()
    }

    /// Last `s` (in from the eave) at which the dormer roof at `d` stands
    /// above the main roof.
    fn last_s(&self, d: i32) -> i32 {
        let mut s = 0;
        while self.main_h(s + 1) < self.roof_h(d) {
            s += 1;
        }
        s
    }

    /// Facing of the cheek stairs at `d` (they climb toward the ridge, d = 0).
    fn cheek_facing(&self, d: i32) -> Cardinal {
        match (self.axis, d < 0) {
            (RidgeAxis::X, true) => Cardinal::East,
            (RidgeAxis::X, false) => Cardinal::West,
            (RidgeAxis::Z, true) => Cardinal::South,
            (RidgeAxis::Z, false) => Cardinal::North,
        }
    }

    /// The direction the dormer's face looks out of the house.
    fn outward(&self) -> Cardinal {
        match (self.axis, self.out > 0) {
            (RidgeAxis::X, true) => Cardinal::South,
            (RidgeAxis::X, false) => Cardinal::North,
            (RidgeAxis::Z, true) => Cardinal::East,
            (RidgeAxis::Z, false) => Cardinal::West,
        }
    }

    /// Every cell this dormer takes from the main roof: the face and what
    /// stands behind it. The main eave overhang in front stays, so the eave
    /// line runs on under the dormer; the dormer's side eaves (`d = ±2`) sit
    /// on top of the main roof without taking it.
    fn owned(&self) -> Vec<Point2D> {
        (-1..=1)
            .flat_map(|d| (0..=self.last_s(d)).map(move |s| (d, s)))
            .map(|(d, s)| self.cell(d, s))
            .collect()
    }
}

/// The dormers of one house and the roof cells they own.
#[derive(Debug, Clone, Default)]
pub struct DormerPlan {
    pub dormers: Vec<Dormer>,
    pub owned: HashSet<Point2D>,
}

/// Plan the dormers of a gable roof. `rects` are the top-floor rects,
/// `roof_rects` the same extended for the roof (wings reaching the core's
/// ridge), `roof_ys` each rect's raw roof y. A dormer needs an eave side with
/// nothing built beyond it, a slope deep enough that the main roof closes
/// behind the dormer a row short of the ridge, a main ridge at least two rows
/// above the dormer's (so stair roofs need a slope of 5+, double roofs 3+,
/// and slab roofs are too shallow), and clear roof (no other rect, no gable
/// end) around it; the eave
/// run takes as many as fit evenly, up to three. The front side (`front`, the
/// side the front door opens out of) is planned first.
pub fn plan_dormers(
    rects: &[Rect2D],
    roof_rects: &[Rect2D],
    axes: &[RidgeAxis],
    roof_ys: &[i32],
    pitch: GablePitch,
    front: Option<Cardinal>,
) -> DormerPlan {
    let mut plan = DormerPlan::default();
    for (i, rect) in rects.iter().enumerate() {
        let axis = axes[i];
        let (along_min, along_max, short_min, short_max) = match axis {
            RidgeAxis::X => (rect.min().x, rect.max().x, rect.min().y, rect.max().y),
            RidgeAxis::Z => (rect.min().y, rect.max().y, rect.min().x, rect.max().x),
        };
        let half = (short_max - short_min) / 2;
        let mut sides = vec![(short_min, -1), (short_max, 1)];
        // Front side first.
        sides.sort_by_key(|&(_, out)| {
            let probe = Dormer { rect: i, axis, along: 0, eave: 0, out, pitch, rise: 0 };
            Some(probe.outward()) != front
        });
        for (eave, out) in sides {
            // The main roof must close behind the ridge cell with a row to
            // spare, and the dormer's ridge stay two rows under the main one
            // so it reads as a dormer, not a cross gable.
            let rise = RISE;
            let probe = Dormer { rect: i, axis, along: along_min, eave, out, pitch, rise };
            if probe.last_s(0) + 2 > half || probe.main_h(half) - rise < 2 {
                continue;
            }
            let run = along_max - along_min + 1;
            // Dormer is 3 wide; keep END_MARGIN to each gable and GAP between.
            let fits = |n: i32| n * 5 + (n - 1) * (GAP - 2) + 2 * END_MARGIN <= run;
            let mut n = (1..=3).rev().find(|&n| fits(n)).unwrap_or(0);
            if n == 0 {
                continue;
            }
            // Evenly spaced centres.
            let mut placed = Vec::new();
            while n > 0 && placed.is_empty() {
                for k in 0..n {
                    let along = along_min + ((k + 1) * (run - 1)) / (n + 1);
                    let dormer = Dormer { rect: i, axis, along, eave, out, pitch, rise };
                    if is_clear(&dormer, i, rects, roof_rects, roof_ys) {
                        placed.push(dormer);
                    }
                }
                if placed.is_empty() {
                    n -= 1;
                }
            }
            for dormer in placed {
                plan.owned.extend(dormer.owned());
                plan.dormers.push(dormer);
            }
        }
    }
    plan
}

/// Whether a dormer's surroundings are free: every cell a ring around it
/// (cheeks ±2, one row past where it meets the main roof, two rows out past
/// the eave) belongs to its own rect's roof alone, and its ends stay
/// `END_MARGIN` from the gable walls.
fn is_clear(dormer: &Dormer, i: usize, rects: &[Rect2D], roof_rects: &[Rect2D], roof_ys: &[i32]) -> bool {
    let rect = &rects[i];
    let depth = dormer.last_s(0) + 1;
    for d in -3..=3 {
        for s in -2..=depth {
            let p = dormer.cell(d, s);
            if s >= 0 && !rect.contains(p) {
                return false;
            }
            for (j, (r, rr)) in rects.iter().zip(roof_rects).enumerate() {
                if j == i {
                    continue;
                }
                // Another rect's walls or roof here (any height): a wing, a
                // taller block, or a lower roof the overhang would hang over.
                if r.contains(p) || (roof_ys[j] >= roof_ys[i] && rr.contains(p)) {
                    return false;
                }
            }
        }
    }
    let along_lo = match dormer.axis {
        RidgeAxis::X => rect.min().x,
        RidgeAxis::Z => rect.min().y,
    };
    let along_hi = match dormer.axis {
        RidgeAxis::X => rect.max().x,
        RidgeAxis::Z => rect.max().y,
    };
    dormer.along - 2 - END_MARGIN >= along_lo && dormer.along + 2 + END_MARGIN <= along_hi
}

/// Build the dormers planned for one roof height group. The main roof has
/// already been placed without the owned cells.
///
/// Front face: timber posts at the cheeks, a two-high window between them
/// with a row of wall above; cheek walls close the sides back to the main
/// slope; the roof is a ridge of slabs over cheek stairs, with brackets under
/// its overhang like the main eaves. Where the main roof had its top-of-wall
/// row under an owned cell, it is put back.
pub async fn place_dormers(
    editor: &Editor,
    dormers: &[&Dormer],
    roof_y: i32,
    trim: bool,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    if dormers.is_empty() {
        return;
    }
    let material = |role: MaterialRole, fallback: MaterialRole| {
        palette.get_material(role).or_else(|| palette.get_material(fallback)).cloned()
    };
    let roof_id = material(MaterialRole::PrimaryRoof, MaterialRole::PrimaryRoof).expect("No primary roof material");
    let trim_id = if trim { material(MaterialRole::SecondaryRoof, MaterialRole::PrimaryRoof) } else { None }
        .unwrap_or_else(|| roof_id.clone());
    let wall_id = material(MaterialRole::PrimaryWall, MaterialRole::PrimaryWall).expect("No primary wall material");

    let mut roof_rng = rng.derive();
    let mut roof = MaterialPlacer::new(Placer::new(&data.materials, &mut roof_rng), roof_id);
    let mut trim_rng = rng.derive();
    let mut edge = MaterialPlacer::new(Placer::new(&data.materials, &mut trim_rng), trim_id);
    let mut wall_rng = rng.derive();
    let mut wall = MaterialPlacer::new(Placer::new(&data.materials, &mut wall_rng), wall_id);

    let state = |pairs: &[(&str, String)]| -> std::collections::HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    };
    let bottom_slab = state(&[("type", "bottom".into())]);
    let glass = Block::from_id("minecraft:glass_pane".into());
    let at = |p: Point2D, rel: i32| Point3D::new(p.x, roof_y + rel, p.y);

    for dormer in dormers {
        let ridge = dormer.rise;
        let window_rows = dormer.rise - 1;
        // Stair and slab roofs lay a ceiling row under the whole roof; a
        // double roof only an eave block on the wall line.
        let has_ceiling_row = !matches!(dormer.pitch, GablePitch::Double);

        // Top-of-wall row under the owned cells inside the walls.
        for d in -1..=1 {
            let last = if has_ceiling_row { dormer.last_s(d) } else { 0 };
            for s in 0..=last {
                roof.place_block(editor, at(dormer.cell(d, s), -1), BlockForm::Block, None, None).await;
            }
        }

        // Front face on the wall line: plastered cheeks either side of the window.
        for d in [-1, 1] {
            for rel in 0..=dormer.roof_h(d) - 1 {
                wall.place_block(editor, at(dormer.cell(d, 0), rel), BlockForm::Block, None, None).await;
            }
        }
        for rel in 0..window_rows {
            editor.place_block_forced(&glass, at(dormer.cell(0, 0), rel)).await;
        }
        for rel in window_rows..ridge {
            wall.place_block(editor, at(dormer.cell(0, 0), rel), BlockForm::Block, None, None).await;
        }

        // Cheek walls, from the main slope up to under the cheek stairs.
        for d in [-1, 1] {
            for s in 1..=dormer.last_s(d) {
                for rel in dormer.main_h(s)..dormer.roof_h(d) {
                    wall.place_block(editor, at(dormer.cell(d, s), rel), BlockForm::Block, None, None).await;
                }
            }
        }

        // Roof: ridge slabs over the face, stairs on the cheeks and a side eave
        // a cell wider, all from the overhang row back.
        for s in -1..=dormer.last_s(0) {
            let placer = if s < 0 || trim { &mut edge } else { &mut roof };
            placer.place_block(editor, at(dormer.cell(0, s), ridge), BlockForm::Slab, Some(&bottom_slab), None).await;
        }
        for d in [-2i32, -1, 1, 2] {
            // A side eave must clear the main eave it stands over.
            if d.abs() == 2 && dormer.roof_h(d) <= dormer.main_h(0) {
                continue;
            }
            let facing = dormer.cheek_facing(d);
            let stair = state(&[("facing", facing.to_string())]);
            let bracket = state(&[("facing", facing.opposite().to_string()), ("half", "top".into())]);
            let rel = dormer.roof_h(d);
            let last = if d.abs() == 2 { (0..).take_while(|&s| dormer.main_h(s) < rel).last().unwrap_or(-1) } else { dormer.last_s(d) };
            for s in -1..=last {
                let placer = if s < 0 || d.abs() == 2 && trim { &mut edge } else { &mut roof };
                placer.place_block(editor, at(dormer.cell(d, s), rel), BlockForm::Stairs, Some(&stair), None).await;
            }
            // Only the side eaves get a bracket: under the face's own
            // overhang one would stand in front of the posts and window.
            if d.abs() == 2 {
                edge.place_block(editor, at(dormer.cell(d, -1), rel - 1), BlockForm::Stairs, Some(&bracket), None).await;
            }
        }
    }
}
