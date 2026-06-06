//! Timber frame: corner posts, floor/ceiling crossbeams, and the decorative
//! overlay (studs, knee braces, per-panel motifs) laid over the baseline
//! skeleton. Also holds the `TimberPattern` taste roll and the panel/stud
//! geometry it depends on.

use std::collections::HashMap;

use crate::editor::Editor;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Placer};
use crate::geometry::{Cardinal, Point3D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;

use super::super::footprint::SizeClass;
use super::super::frame::Frame;
use super::super::pipeline::BuildCtx;
use super::segments::{build_segments, is_inside_opening, segment_cells};

/// Per-panel decorative motif laid inside a stud-bounded panel. `Empty` is a
/// blank wattle-and-daub panel; the others place stair-block braces at the
/// panel corners. `Pillar` is a full-height extra stud, useful as a divider
/// inside composite sequences like `/`-`|`-`\`.
// `Pillar`, `KRight`, and `KLeft` are wired through `place_frame` but not yet
// emitted by `pick_panel_sequence`; kept as ready-to-use motifs.
#[allow(dead_code)]
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
pub(super) fn stud_indices(length: u32, spacing: u32) -> Vec<u32> {
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
