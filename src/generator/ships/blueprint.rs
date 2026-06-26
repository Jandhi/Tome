//! Minimal diagnostics. A side-profile (x = length →, y = up) dump of the keel
//! for quick offline sanity checks before a live screenshot.

use crate::minecraft::BlockForm;

use super::additions::bowsprit::BowspritModel;
use super::hull::HullModel;
use super::keel::KeelModel;
use super::ShipDir;

/// Render the keel as a side profile: stern (`x = 0`) at the left, bow at the
/// right; `#` = full block, `/` = stair, `_` = slab. The waterline row is marked
/// with `~`.
pub fn render_keel_ascii(model: &KeelModel) -> String {
    let w = model.length.max(1) as usize;
    let h = (model.depth + 2).max(1) as usize;
    let mut grid = vec![vec![' '; w]; h];

    for cell in &model.cells {
        let (x, y) = (cell.local.x, cell.local.y);
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            continue;
        }
        let ch = match cell.form {
            BlockForm::Block => '#',
            BlockForm::Stairs => match cell.facing {
                Some(ShipDir::Stern) => '\\',
                _ => '/',
            },
            BlockForm::Slab => '_',
            _ => '?',
        };
        grid[y as usize][x as usize] = ch;
    }

    let mut out = String::new();
    out.push_str(&format!(
        "keel  length={}  depth={}  bow_rake={}  stern_steps={}x{}\n",
        model.length, model.depth, model.bow_rake_len, model.stern_steps, model.stern_step_run
    ));
    out.push_str("(stern x=0 left, bow right;  # block  / \\ stair  _ slab;  ~ = waterline)\n");
    for y in (0..h).rev() {
        out.push(if y as i32 == model.waterline_y { '~' } else { ' ' });
        for x in 0..w {
            out.push(grid[y][x]);
        }
        out.push('\n');
    }
    out
}

/// Top-down plan of the hull shell at the **widest layer** (the waterline). Stern
/// (x=0) at the left, bow at the right; centreline marked, `#` = shell block.
pub fn render_hull_plan(model: &HullModel) -> String {
    let w = model.length.max(1) as usize;
    let max_hw = (model.max_beam / 2).max(0);
    let zspan = (max_hw * 2 + 1).max(1) as usize; // -max_hw..=max_hw
    let mut grid = vec![vec![' '; w]; zspan];

    let top_layer = model.cells.iter().map(|c| c.y).max().unwrap_or(0);
    for c in model.cells.iter().filter(|c| c.y == top_layer) {
        let x = c.x;
        let zi = c.z + max_hw; // shift so -max_hw → 0
        if x < 0 || zi < 0 || x as usize >= w || zi as usize >= zspan {
            continue;
        }
        grid[zi as usize][x as usize] = '#';
    }

    let mut out = String::new();
    out.push_str(&format!(
        "hull plan @ waterline  length={}  max_beam={}\n",
        model.length, model.max_beam
    ));
    out.push_str("(stern x=0 left, bow right; rows = beam, centre marked; # = shell)\n");
    for (zi, row) in grid.iter().enumerate() {
        out.push(if zi as i32 == max_hw { '|' } else { ' ' });
        out.extend(row.iter());
        out.push('\n');
    }
    out
}

/// Cross-section of the hull at the widest station: beam (z) across, keel→waterline
/// (y) up. `#` = full-block shell, `/`/`\` = bilge-flare bevel stair (facing inboard),
/// `.` = hollow interior. The fast pre-check for the bilge smoothing before a live
/// screenshot.
pub fn render_hull_section(model: &HullModel) -> String {
    // Widest station = the one with the most shell+bevel cells across all layers.
    let station = {
        let mut counts = vec![0usize; model.length.max(1) as usize];
        for c in &model.cells {
            if (0..model.length).contains(&c.x) {
                counts[c.x as usize] += 1;
            }
        }
        for b in &model.bevel {
            if (0..model.length).contains(&b.local.x) {
                counts[b.local.x as usize] += 1;
            }
        }
        counts
            .iter()
            .enumerate()
            .max_by_key(|(_, n)| **n)
            .map(|(x, _)| x as i32)
            .unwrap_or(0)
    };

    let max_hw = (model.max_beam / 2).max(0);
    let zspan = (max_hw * 2 + 1).max(1) as usize; // -max_hw..=max_hw
    let h = (model.depth + 1).max(1) as usize;
    let mut grid = vec![vec![' '; zspan]; h];

    let plot = |grid: &mut Vec<Vec<char>>, y: i32, z: i32, ch: char| {
        let zi = z + max_hw;
        if y >= 0 && (y as usize) < grid.len() && zi >= 0 && (zi as usize) < zspan {
            grid[y as usize][zi as usize] = ch;
        }
    };

    for c in model.cells.iter().filter(|c| c.x == station) {
        plot(&mut grid, c.y, c.z, '#');
    }
    for c in model.interior.iter().filter(|c| c.x == station) {
        plot(&mut grid, c.y, c.z, '.');
    }
    for b in model.bevel.iter().filter(|b| b.local.x == station) {
        // Bevel faces inboard: a port-side cell (z>0) reads as `\`, starboard as `/`.
        let ch = if b.local.z > 0 { '\\' } else { '/' };
        plot(&mut grid, b.local.y, b.local.z, ch);
    }

    let mut out = String::new();
    out.push_str(&format!(
        "hull section @ x={station}  max_beam={}  depth={}\n",
        model.max_beam, model.depth
    ));
    out.push_str("(beam across, keel bottom → waterline up; # block  / \\ bilge bevel  . hollow)\n");
    for y in (0..h).rev() {
        out.extend(grid[y].iter());
        out.push('\n');
    }
    out
}

/// Side profile of the bowsprit **spar** (the centerline z=0 beam): stem at the left,
/// forward tip at the right; `x` →, `y` up. `#` = full block, `^` = top slab (upper
/// half), `_` = bottom slab (lower half). The fast pre-check for the block/slab step
/// pattern before a live screenshot.
pub fn render_spar_profile(model: &BowspritModel) -> String {
    let cells = &model.spar;
    let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for c in cells {
        x0 = x0.min(c.local.x);
        x1 = x1.max(c.local.x);
        y0 = y0.min(c.local.y);
        y1 = y1.max(c.local.y);
    }
    if cells.is_empty() {
        return format!("spar {:?}: (empty)\n", model.rake);
    }
    let w = (x1 - x0 + 1) as usize;
    let h = (y1 - y0 + 1) as usize;
    let mut grid = vec![vec![' '; w]; h];
    for c in cells {
        let ch = match (c.form, c.top_half) {
            (BlockForm::Slab, true) => '^',
            (BlockForm::Slab, false) => '_',
            _ => '#',
        };
        grid[(c.local.y - y0) as usize][(c.local.x - x0) as usize] = ch;
    }

    let mut out = String::new();
    out.push_str(&format!(
        "spar {:?}  (stem left → tip right;  # block  ^ top slab  _ bottom slab)\n",
        model.rake
    ));
    for y in (0..h).rev() {
        out.extend(grid[y].iter());
        out.push('\n');
    }
    out
}
