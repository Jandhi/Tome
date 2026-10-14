//! A self-contained voxel snapshot of a town: the terrain surface plus every
//! block the editor placed, in **absolute** world coordinates.
//!
//! This is both the input to the software renderer (`crate::render`) and the
//! on-disk "saved town" format, so a run can be re-rendered or inspected later
//! without a Minecraft server. Blocks are interned into a palette of full block
//! strings (`minecraft:oak_stairs[facing=north,half=bottom]`) so the file stays
//! small and block state survives for future renderers.
//!
//! File format (`.scene`, gzip-compressed text, one record per line):
//!
//! ```text
//! #tome-scene 1
//! #bounds <ox> <oy> <oz> <sx> <sy> <sz>
//! #meta <key>\t<value>        (repeated)
//! #palette <n>
//! <block string>              (n lines)
//! #blocks <m>
//! <x> <y> <z> <palette index> (m lines)
//! ```

use std::collections::BTreeMap;

use crate::util::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};

use crate::geometry::{Point3D, Rect3D};

#[derive(Debug, Clone)]
pub struct Scene {
    /// Absolute world bounds of the snapshot (normally the build area).
    pub bounds: Rect3D,
    /// Full block strings, `id[state]`, indexed by the values in `blocks`.
    pub palette: Vec<String>,
    /// Absolute world coordinate → palette index. Air entries are kept: they
    /// record terrain the generator cleared.
    pub blocks: HashMap<Point3D, u32>,
    /// Free-form provenance: `seed`, `culture`, `source` (`live` | `offline`),
    /// `captured_at`, …
    pub meta: BTreeMap<String, String>,
    index: HashMap<String, u32>,
}

impl Scene {
    pub fn new(bounds: Rect3D) -> Self {
        Scene { bounds, palette: Vec::new(), blocks: HashMap::default(), meta: BTreeMap::new(), index: HashMap::default() }
    }

    /// Intern a block string and record it at `p` (absolute coords), replacing
    /// whatever was there. `block` is `id` or `id[k=v,...]`.
    pub fn insert(&mut self, p: Point3D, block: &str) {
        let idx = match self.index.get(block) {
            Some(&i) => i,
            None => {
                let i = self.palette.len() as u32;
                self.palette.push(block.to_string());
                self.index.insert(block.to_string(), i);
                i
            }
        };
        self.blocks.insert(p, idx);
    }

    /// Full block string at `p`, if any.
    pub fn block_str(&self, p: Point3D) -> Option<&str> {
        self.blocks.get(&p).map(|&i| self.palette[i as usize].as_str())
    }

    /// Block id at `p` with any `[state]` suffix stripped, e.g. `minecraft:oak_stairs`.
    pub fn block_id(&self, p: Point3D) -> Option<&str> {
        self.block_str(p).map(Self::strip_state)
    }

    /// `true` when `p` holds a non-air block. Air and missing both count as empty.
    pub fn is_solid(&self, p: Point3D) -> bool {
        matches!(self.block_id(p), Some(id) if !Self::is_air_id(id))
    }

    pub fn strip_state(block: &str) -> &str {
        block.split('[').next().unwrap_or(block)
    }

    pub fn is_air_id(id: &str) -> bool {
        matches!(id.trim_start_matches("minecraft:"), "air" | "cave_air" | "void_air")
    }

    /// Iterate `(position, full block string)` over every entry, including air.
    pub fn iter(&self) -> impl Iterator<Item = (Point3D, &str)> + '_ {
        self.blocks.iter().map(move |(p, &i)| (*p, self.palette[i as usize].as_str()))
    }

    /// Highest non-air block in the column `(x, z)`, if any.
    pub fn top_y(&self, x: i32, z: i32) -> Option<i32> {
        let min_y = self.bounds.origin.y;
        let max_y = self.bounds.max().y;
        (min_y..=max_y).rev().find(|&y| self.is_solid(Point3D::new(x, y, z)))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut w = BufWriter::new(GzEncoder::new(file, Compression::default()));
        let b = &self.bounds;
        writeln!(w, "#tome-scene 1")?;
        writeln!(w, "#bounds {} {} {} {} {} {}", b.origin.x, b.origin.y, b.origin.z, b.size.x, b.size.y, b.size.z)?;
        for (k, v) in &self.meta {
            let clean: String = v.chars().map(|c| if c == '\n' || c == '\t' { ' ' } else { c }).collect();
            writeln!(w, "#meta {}\t{}", k, clean)?;
        }
        writeln!(w, "#palette {}", self.palette.len())?;
        for s in &self.palette {
            writeln!(w, "{s}")?;
        }
        writeln!(w, "#blocks {}", self.blocks.len())?;
        let mut entries: Vec<_> = self.blocks.iter().collect();
        entries.sort_by_key(|(p, _)| (p.y, p.z, p.x));
        for (p, i) in entries {
            writeln!(w, "{} {} {} {}", p.x, p.y, p.z, i)?;
        }
        w.flush()?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Scene> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let r = BufReader::new(GzDecoder::new(file));
        let mut lines = r.lines();
        let mut next = || -> Result<String> {
            lines.next().ok_or_else(|| anyhow!("unexpected end of scene file"))?.map_err(Into::into)
        };
        let header = next()?;
        if header.trim() != "#tome-scene 1" {
            return Err(anyhow!("not a tome scene file: {header:?}"));
        }
        let bounds_line = next()?;
        let nums: Vec<i32> = bounds_line
            .trim_start_matches("#bounds")
            .split_whitespace()
            .map(|s| s.parse())
            .collect::<std::result::Result<_, _>>()?;
        if nums.len() != 6 {
            return Err(anyhow!("bad bounds line: {bounds_line:?}"));
        }
        let mut scene = Scene::new(Rect3D {
            origin: Point3D::new(nums[0], nums[1], nums[2]),
            size: Point3D::new(nums[3], nums[4], nums[5]),
        });
        // meta lines until #palette
        let mut line = next()?;
        while let Some(rest) = line.strip_prefix("#meta ") {
            if let Some((k, v)) = rest.split_once('\t') {
                scene.meta.insert(k.to_string(), v.to_string());
            }
            line = next()?;
        }
        let n: usize = line
            .strip_prefix("#palette ")
            .ok_or_else(|| anyhow!("expected #palette, got {line:?}"))?
            .trim()
            .parse()?;
        for _ in 0..n {
            let s = next()?;
            let i = scene.palette.len() as u32;
            scene.index.insert(s.clone(), i);
            scene.palette.push(s);
        }
        let line = next()?;
        let m: usize = line
            .strip_prefix("#blocks ")
            .ok_or_else(|| anyhow!("expected #blocks, got {line:?}"))?
            .trim()
            .parse()?;
        scene.blocks.reserve(m);
        for _ in 0..m {
            let s = next()?;
            let mut it = s.split_whitespace();
            let parsed = match (it.next(), it.next(), it.next(), it.next()) {
                (Some(x), Some(y), Some(z), Some(i)) => (x.parse::<i32>()?, y.parse::<i32>()?, z.parse::<i32>()?, i.parse::<u32>()?),
                _ => return Err(anyhow!("bad block line: {s:?}")),
            };
            scene.blocks.insert(Point3D::new(parsed.0, parsed.1, parsed.2), parsed.3);
        }
        Ok(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_roundtrips_through_file() {
        let mut s = Scene::new(Rect3D { origin: Point3D::new(-10, 60, 5), size: Point3D::new(4, 3, 4) });
        s.meta.insert("seed".into(), "42".into());
        s.insert(Point3D::new(-10, 60, 5), "minecraft:grass_block");
        s.insert(Point3D::new(-9, 61, 5), "minecraft:oak_stairs[facing=north,half=bottom]");
        s.insert(Point3D::new(-9, 62, 5), "minecraft:air");
        s.insert(Point3D::new(-8, 60, 6), "minecraft:grass_block");
        let dir = std::env::temp_dir().join("tome_scene_test");
        let path = dir.join("t.scene");
        s.save(&path).unwrap();
        let l = Scene::load(&path).unwrap();
        assert_eq!(l.bounds.origin, s.bounds.origin);
        assert_eq!(l.meta.get("seed").map(String::as_str), Some("42"));
        assert_eq!(l.block_str(Point3D::new(-9, 61, 5)), Some("minecraft:oak_stairs[facing=north,half=bottom]"));
        assert_eq!(l.block_id(Point3D::new(-9, 61, 5)), Some("minecraft:oak_stairs"));
        assert!(!l.is_solid(Point3D::new(-9, 62, 5)));
        assert_eq!(l.top_y(-9, 5), Some(61));
        assert_eq!(l.palette.len(), 3);
        assert_eq!(l.blocks.len(), 4);
    }
}
