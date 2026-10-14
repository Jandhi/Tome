//! Block-level comparison of two saved scenes (e.g. a live run and its offline
//! twin): which positions hold a block only in A, only in B, or a different
//! block in each. Air and missing entries both count as "no block".
//!
//! [`diff_scenes`] produces the [`SceneDiff`] summary (saved as `diff.json`);
//! [`render_diff_top`] draws `diff_top.png`, a grey height-shaded top view of
//! both scenes with every differing column tinted red (only in A), green (only
//! in B) or yellow (changed). When a column has several kinds of difference the
//! most severe wins, in that order.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use resvg::tiny_skia::Pixmap;
use serde::{Deserialize, Serialize};

use super::grid::{canvas, fill_square, Rgba};
use super::scene::Scene;
use crate::geometry::Point3D;
use crate::util::{HashMap, HashSet};

/// How many ids per category `diff.json` keeps.
const TOP_KEEP: usize = 25;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct SceneDiff {
    pub a_meta: BTreeMap<String, String>,
    pub b_meta: BTreeMap<String, String>,
    /// Non-air blocks in each scene.
    pub a_blocks: usize,
    pub b_blocks: usize,
    /// Positions holding the same block string in both.
    pub same: usize,
    pub only_a: usize,
    pub only_b: usize,
    /// Different block strings at the same position (includes `changed_state_only`).
    pub changed: usize,
    /// Changed positions where the id matches and only the `[state]` differs.
    pub changed_state_only: usize,
    /// Columns `(x, z)` with any difference.
    pub columns_differing: usize,
    /// `(id, count)`, largest first, ids without the `minecraft:` prefix.
    pub top_only_a: Vec<(String, usize)>,
    pub top_only_b: Vec<(String, usize)>,
    /// `("a_id -> b_id", count)`; state-only changes read `"id -> id"`.
    pub top_changed: Vec<(String, usize)>,
}

/// Difference at one position. Ordered least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Changed,
    OnlyB,
    OnlyA,
}

/// The non-air block string at `p`, if any.
fn solid_at(s: &Scene, p: Point3D) -> Option<&str> {
    s.block_str(p).filter(|b| !Scene::is_air_id(Scene::strip_state(b)))
}

fn short_id(block: &str) -> &str {
    let id = Scene::strip_state(block);
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// Every position recorded in either scene, sorted for a stable walk.
fn all_positions(a: &Scene, b: &Scene) -> Vec<Point3D> {
    let set: HashSet<Point3D> = a.blocks.keys().chain(b.blocks.keys()).copied().collect();
    let mut v: Vec<Point3D> = set.into_iter().collect();
    v.sort_by_key(|p| (p.x, p.z, p.y));
    v
}

fn top_n(counts: HashMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    v.truncate(n);
    v
}

/// Compare `a` and `b` position by position.
pub fn diff_scenes(a: &Scene, b: &Scene) -> SceneDiff {
    let mut d = SceneDiff { a_meta: a.meta.clone(), b_meta: b.meta.clone(), ..Default::default() };
    let mut only_a: HashMap<String, usize> = HashMap::default();
    let mut only_b: HashMap<String, usize> = HashMap::default();
    let mut changed: HashMap<String, usize> = HashMap::default();
    let mut columns: HashSet<(i32, i32)> = HashSet::default();
    for p in all_positions(a, b) {
        let (ba, bb) = (solid_at(a, p), solid_at(b, p));
        d.a_blocks += ba.is_some() as usize;
        d.b_blocks += bb.is_some() as usize;
        match (ba, bb) {
            (None, None) => continue,
            (Some(x), Some(y)) if x == y => {
                d.same += 1;
                continue;
            }
            (Some(x), None) => {
                d.only_a += 1;
                *only_a.entry(short_id(x).to_string()).or_insert(0) += 1;
            }
            (None, Some(y)) => {
                d.only_b += 1;
                *only_b.entry(short_id(y).to_string()).or_insert(0) += 1;
            }
            (Some(x), Some(y)) => {
                d.changed += 1;
                if short_id(x) == short_id(y) {
                    d.changed_state_only += 1;
                }
                *changed.entry(format!("{} -> {}", short_id(x), short_id(y))).or_insert(0) += 1;
            }
        }
        columns.insert((p.x, p.z));
    }
    d.columns_differing = columns.len();
    d.top_only_a = top_n(only_a, TOP_KEEP);
    d.top_only_b = top_n(only_b, TOP_KEEP);
    d.top_changed = top_n(changed, TOP_KEEP);
    d
}

impl SceneDiff {
    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?).with_context(|| format!("write {}", path.display()))
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<SceneDiff> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Readable summary: totals, then the top `n` ids of each category.
    pub fn table(&self, n: usize) -> String {
        let mut out = String::new();
        let tag = |m: &BTreeMap<String, String>| {
            format!(
                "{} seed {}",
                m.get("source").map(String::as_str).unwrap_or("?"),
                m.get("seed").map(String::as_str).unwrap_or("?")
            )
        };
        let _ = writeln!(out, "A: {}  ({} blocks)", tag(&self.a_meta), self.a_blocks);
        let _ = writeln!(out, "B: {}  ({} blocks)", tag(&self.b_meta), self.b_blocks);
        let _ = writeln!(out, "{:<22}{:>10}", "same", self.same);
        let _ = writeln!(out, "{:<22}{:>10}", "only in A", self.only_a);
        let _ = writeln!(out, "{:<22}{:>10}", "only in B", self.only_b);
        let _ = writeln!(out, "{:<22}{:>10}", "changed", self.changed);
        let _ = writeln!(out, "{:<22}{:>10}", "  of which state only", self.changed_state_only);
        let _ = writeln!(out, "{:<22}{:>10}", "columns differing", self.columns_differing);
        for (title, list) in [("only in A", &self.top_only_a), ("only in B", &self.top_only_b), ("changed (A -> B)", &self.top_changed)] {
            let _ = writeln!(out, "\nTop {title}:");
            for (id, c) in list.iter().take(n) {
                let _ = writeln!(out, "  {c:>9}  {id}");
            }
        }
        out
    }
}

const RED: Rgba = [220, 40, 40, 255];
const GREEN: Rgba = [40, 180, 60, 255];
const YELLOW: Rgba = [235, 200, 30, 255];
/// Share of the category colour in a tinted column.
const TINT: f32 = 0.7;

/// Top-down diff image at `px` pixels per block over the union of both scenes'
/// x/z bounds, with a 1-block margin. Base: highest non-air block of either
/// scene as a grey level (darker = lower). Differing columns are tinted.
pub fn render_diff_top(a: &Scene, b: &Scene, px: u32) -> anyhow::Result<Pixmap> {
    let px = px.max(1) as usize;
    let (ba, bb) = (a.bounds, b.bounds);
    let (x0, z0) = (ba.origin.x.min(bb.origin.x), ba.origin.z.min(bb.origin.z));
    let x1 = ba.max().x.max(bb.max().x);
    let z1 = ba.max().z.max(bb.max().z);
    let (nx, nz) = ((x1 - x0 + 1).max(1) as usize, (z1 - z0 + 1).max(1) as usize);

    let mut top: HashMap<(i32, i32), i32> = HashMap::default();
    for s in [a, b] {
        for (p, block) in s.iter() {
            if !Scene::is_air_id(Scene::strip_state(block)) {
                let e = top.entry((p.x, p.z)).or_insert(p.y);
                *e = (*e).max(p.y);
            }
        }
    }
    let mut kind: HashMap<(i32, i32), Kind> = HashMap::default();
    for p in all_positions(a, b) {
        let k = match (solid_at(a, p), solid_at(b, p)) {
            (Some(_), None) => Some(Kind::OnlyA),
            (None, Some(_)) => Some(Kind::OnlyB),
            (Some(x), Some(y)) if x != y => Some(Kind::Changed),
            _ => None,
        };
        if let Some(k) = k {
            let e = kind.entry((p.x, p.z)).or_insert(k);
            *e = (*e).max(k);
        }
    }

    let (lo, hi) = top.values().fold((i32::MAX, i32::MIN), |(l, h), &y| (l.min(y), h.max(y)));
    let span = (hi - lo).max(1) as f32;
    let mut pm = canvas((nx + 2) * px, (nz + 2) * px)?;
    for x in 0..nx {
        for z in 0..nz {
            let col = (x0 + x as i32, z0 + z as i32);
            let grey = match top.get(&col) {
                Some(&y) => (70.0 + 150.0 * (y - lo) as f32 / span) as u8,
                None => continue,
            };
            let base: Rgba = [grey, grey, grey, 255];
            let c = match kind.get(&col) {
                None => base,
                Some(k) => {
                    let t = match k {
                        Kind::OnlyA => RED,
                        Kind::OnlyB => GREEN,
                        Kind::Changed => YELLOW,
                    };
                    let mix = |i: usize| (t[i] as f32 * TINT + base[i] as f32 * (1.0 - TINT)) as u8;
                    [mix(0), mix(1), mix(2), 255]
                }
            };
            fill_square(&mut pm, (x + 1) * px, (z + 1) * px, px, c);
        }
    }
    Ok(pm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect3D;

    fn scene(origin_x: i32) -> Scene {
        Scene::new(Rect3D { origin: Point3D::new(origin_x, 60, 0), size: Point3D::new(4, 4, 4) })
    }

    #[test]
    fn diff_counts_added_removed_changed() {
        let mut a = scene(0);
        let mut b = scene(2);
        // Shared, identical.
        a.insert(Point3D::new(1, 60, 1), "minecraft:stone");
        b.insert(Point3D::new(1, 60, 1), "minecraft:stone");
        // Removed in B (B records air there, which counts as absence).
        a.insert(Point3D::new(0, 61, 0), "minecraft:oak_planks");
        b.insert(Point3D::new(0, 61, 0), "minecraft:air");
        // Added in B.
        b.insert(Point3D::new(5, 60, 3), "minecraft:lantern[hanging=false]");
        // Changed state only.
        a.insert(Point3D::new(2, 62, 2), "minecraft:oak_stairs[facing=north]");
        b.insert(Point3D::new(2, 62, 2), "minecraft:oak_stairs[facing=south]");
        // Air in both: ignored.
        a.insert(Point3D::new(3, 63, 3), "minecraft:air");

        let d = diff_scenes(&a, &b);
        assert_eq!((d.a_blocks, d.b_blocks), (3, 3));
        assert_eq!(d.same, 1);
        assert_eq!(d.only_a, 1);
        assert_eq!(d.only_b, 1);
        assert_eq!(d.changed, 1);
        assert_eq!(d.changed_state_only, 1);
        assert_eq!(d.columns_differing, 3);
        assert_eq!(d.top_only_a, vec![("oak_planks".to_string(), 1)]);
        assert_eq!(d.top_only_b, vec![("lantern".to_string(), 1)]);
        assert_eq!(d.top_changed, vec![("oak_stairs -> oak_stairs".to_string(), 1)]);

        let path = std::env::temp_dir().join("tome_diff_test").join("diff.json");
        d.save(&path).unwrap();
        assert_eq!(SceneDiff::load(&path).unwrap(), d);

        // Union of x 0..=3 and 2..=5 is 6 wide; z 0..=3 is 4; plus margins.
        let pm = render_diff_top(&a, &b, 2).unwrap();
        assert_eq!((pm.width(), pm.height()), (16, 12));
    }
}
