//! Minimal v2 diagnostics. A side-profile (x = length →, y = up) dump of the keel
//! for quick offline sanity checks before a live screenshot.

use crate::minecraft::BlockForm;

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
