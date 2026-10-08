//! Saved terrain: the raw inputs `World::new` receives from the GDMC server
//! (chunks + three heightmaps), so a World can be rebuilt without a server via
//! [`World::from_fixture`](super::World::from_fixture).
//!
//! Chunk sections outside the terrain band ([`TerrainFixture::kept_y_range`])
//! are dropped to keep the file small, so a rebuilt World's `get_block`
//! returns `None` for points outside that band (deep underground, high sky).
//! Everything inside the band behaves exactly like the live World.
//!
//! On disk: gzip-compressed NBT (`*.fixture.nbt.gz`), via fastnbt.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::Context;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_derive::{Deserialize, Serialize};

use crate::{geometry::Rect3D, minecraft::Chunk};

/// Margin in blocks kept below the lowest ocean floor and above the highest
/// motion-blocking surface.
const BAND_MARGIN: i32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainFixture {
    pub build_area: Rect3D,
    /// Chunk sections intersecting `kept_y_range` only.
    pub chunks: Vec<Chunk>,
    /// Absolute block Y band `(min, max)`, inclusive, whose sections were kept.
    pub kept_y_range: (i32, i32),
    /// `MOTION_BLOCKING_NO_PLANTS`, absolute Y, `[x][z]`.
    pub ground_map: Vec<Vec<i32>>,
    /// `OCEAN_FLOOR_NO_PLANTS`, absolute Y, `[x][z]`.
    pub ocean_map: Vec<Vec<i32>>,
    /// `MOTION_BLOCKING`, absolute Y, `[x][z]`.
    pub motion_blocking_map: Vec<Vec<i32>>,
}

impl TerrainFixture {
    /// Build a fixture from raw server data, dropping chunk sections whose
    /// 16-block band lies outside `[min ocean floor - 16, max motion-blocking + 16]`.
    pub fn new(
        build_area: Rect3D,
        mut chunks: Vec<Chunk>,
        ground_map: Vec<Vec<i32>>,
        ocean_map: Vec<Vec<i32>>,
        motion_blocking_map: Vec<Vec<i32>>,
    ) -> TerrainFixture {
        let lo = ocean_map.iter().flatten().copied().min().unwrap_or(build_area.origin.y) - BAND_MARGIN;
        let hi = motion_blocking_map.iter().flatten().copied().max().unwrap_or(build_area.max().y) + BAND_MARGIN;
        for chunk in &mut chunks {
            chunk.sections.retain(|s| s.y * 16 <= hi && s.y * 16 + 15 >= lo);
        }
        chunks.sort_by_key(|c| (c.x_pos, c.z_pos));
        TerrainFixture { build_area, chunks, kept_y_range: (lo, hi), ground_map, ocean_map, motion_blocking_map }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let bytes = fastnbt::to_bytes(self).context("encode terrain fixture as NBT")?;
        let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut w = GzEncoder::new(std::io::BufWriter::new(file), Compression::fast());
        w.write_all(&bytes)?;
        w.finish()?.flush()?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<TerrainFixture> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut bytes = Vec::new();
        GzDecoder::new(std::io::BufReader::new(file)).read_to_end(&mut bytes)?;
        fastnbt::from_bytes(&bytes).with_context(|| format!("parse {}", path.display()))
    }
}
