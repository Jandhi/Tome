use std::cell::RefCell;
use crate::util::HashMap;

use anyhow::Ok;
use log::{error, info, warn};

use crate::{data::Loadable, editor::World, generator::materials::{Material, MaterialId}, geometry::{Point2D, Point3D, Rect3D}, http_mod::{Coordinate, CommandResponse, GDMCHTTPProvider, PositionedBlock, PositionedEntity}, minecraft::{Block, BlockForm, BlockID}, noise::RNG};

/// Editor provides the interface for modifying the Minecraft world.
///
/// Uses interior mutability (RefCell) for block_buffer, block_cache, and block_form_cache
/// so that generators can read from World and write blocks without borrow conflicts.
///
/// Note: Keep RefCell borrows short-lived, especially around async operations.
/// Do not hold Ref/RefMut across .await points.
#[derive(Debug)]
pub struct Editor {
    build_area: Rect3D,
    provider: GDMCHTTPProvider,
    block_buffer: RefCell<Vec<PositionedBlock>>,
    buffer_size: usize,
    /// When false, `flush_buffer` sends buffered blocks with `doBlockUpdates=false`,
    /// so the server places exactly the blocks given without running placement side
    /// effects. Needed when pasting authored structures: with updates on, placing a
    /// bed foot makes the server auto-spawn a head (and a door bottom an upper half),
    /// duplicating the second half the NBT already contains — often into a wall.
    /// Defaults to true; toggle via `set_block_updates` around a structure paste.
    block_updates: RefCell<bool>,
    block_cache: RefCell<HashMap<Point3D, Block>>,
    /// While `Some`, every placement's build-area-local cell is appended
    /// (see `start_recording` / `stop_recording`).
    recording: RefCell<Option<Vec<Point3D>>>,
    world: World,
    materials: HashMap<MaterialId, Material>,
    block_form_cache: RefCell<HashMap<BlockID, BlockForm>>,
    /// When true, skip all outbound HTTP traffic. Block placements still land
    /// in `block_cache` so reads stay consistent, but nothing reaches the
    /// Minecraft server. Use for offline pipeline tests that only exercise
    /// generator logic + blueprint rendering.
    offline: bool,
}

impl Editor {
    /// Start collecting the build-area-local cell of every placement until
    /// `stop_recording`; nested starts restart the list.
    pub fn start_recording(&self) {
        *self.recording.borrow_mut() = Some(Vec::new());
    }

    /// The cells placed since `start_recording` (empty if not recording).
    pub fn stop_recording(&self) -> Vec<Point3D> {
        self.recording.borrow_mut().take().unwrap_or_default()
    }

    fn record(&self, point: Point3D) {
        if let Some(log) = self.recording.borrow_mut().as_mut() {
            log.push(point);
        }
    }

    pub fn new(build_area: Rect3D, world: World) -> Self {
        let mut editor = Self {
            build_area,
            provider: GDMCHTTPProvider::new(),
            block_buffer: RefCell::new(Vec::new()),
            buffer_size: 32,
            block_updates: RefCell::new(true),
            block_cache: RefCell::new(HashMap::default()),
            recording: RefCell::new(None),
            world,
            materials: HashMap::default(),
            block_form_cache: RefCell::new(HashMap::default()),
            offline: false,
        };
        editor.load_data().expect("Failed to load materials");
        editor
    }

    /// Construct an editor that skips all HTTP traffic. Pair with
    /// `World::synthetic` for a fully offline pipeline run.
    pub fn new_offline(build_area: Rect3D, world: World) -> Self {
        let mut editor = Self::new(build_area, world);
        editor.offline = true;
        editor
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn set_buffer_size(&mut self, size: usize) {
        self.buffer_size = size;
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Controls whether buffered blocks are flushed with block updates on
    /// (`true`, the default) or off. Flushes any pending buffer first so the
    /// switch only affects blocks placed afterwards. Disable around a structure
    /// paste so the server doesn't auto-spawn the second half of beds/doors that
    /// the structure already contains; re-enable when done.
    pub async fn set_block_updates(&self, on: bool) {
        self.flush_buffer().await;
        *self.block_updates.borrow_mut() = on;
    }

    fn load_data(&mut self) -> anyhow::Result<()> {
        info!("Loading editor data");
        self.materials = Material::load()?;
        Ok(())
    }

    pub async fn place_block(&self, block: &Block, point: Point3D) {
        self.place_block_options(block, point, false).await;
    }

    pub async fn place_block_forced(&self, block: &Block, point: Point3D) {
        self.place_block_options(block, point, true).await;
    }

    pub async fn place_block_options(&self, block: &Block, point: Point3D, force: bool) {
        if !self.world.build_area.contains(point + self.build_area.origin) {
            warn!("Point {:?} is outside the build area {:?} and will be ignored", point + self.build_area.origin, self.world.build_area);
            return;
        }

        // Never place an `axis` blockstate on a block that doesn't support one
        // (e.g. a log palette-swapped into sandstone). Minecraft rejects the
        // whole placement for an invalid state and the block silently vanishes,
        // so strip the stray axis here — the single chokepoint all placers hit.
        let stripped;
        let block = if block.state.as_ref().map_or(false, |s| s.contains_key("axis")) && !block.id.is_axis_block() {
            let mut b = block.clone();
            if let Some(s) = b.state.as_mut() { s.remove("axis"); }
            stripped = b;
            &stripped
        } else {
            block
        };

        if !force {
            let cache = self.block_cache.borrow();
            if cache.contains_key(&point) {
                let density = self.get_block_form(&block.id).density();
                let current_block = cache.get(&point).expect("Block should be in cache").id.clone();
                drop(cache); // Release borrow before calling get_block_form again

                if density <= self.get_block_form(&current_block).density() {
                    info!("Block at {:?} is already placed with a denser block, skipping", point);
                    return;
                }
            }
        }

        self.block_cache.borrow_mut().insert(point, block.clone());
        self.record(point);
        self.block_buffer.borrow_mut().push(
            PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into())
        );

        // Check buffer size and flush if needed
        // Note: We get the length first, then flush, to avoid holding borrow across await
        let should_flush = self.block_buffer.borrow().len() >= self.buffer_size;
        if should_flush {
            self.flush_buffer().await;
        }
    }

    fn get_block_form(&self, id: &BlockID) -> BlockForm {
        // Check if already cached
        if let Some(form) = self.block_form_cache.borrow().get(id) {
            return *form;
        }

        // Compute and cache
        let form = BlockForm::infer_from_block(id);
        self.block_form_cache.borrow_mut().insert(id.clone(), form);
        form
    }

    pub async fn place_block_chance(&self, block: &Block, point: Point3D, rng: &mut RNG, chance: i32) {
        if rng.rand_i32_range(1, 100) <= chance {
            self.place_block(block, point).await;
        }
    }

    /// Place a block immediately without triggering block updates.
    /// This is useful for placing support blocks (like floors) that might otherwise
    /// cause attached blocks (like doors) to break.
    pub async fn place_block_no_update(&self, block: &Block, point: Point3D) {
        if !self.world.build_area.contains(point + self.build_area.origin) {
            warn!("Point {:?} is outside the build area {:?} and will be ignored", point + self.build_area.origin, self.world.build_area);
            return;
        }

        self.block_cache.borrow_mut().insert(point, block.clone());
        self.record(point);

        if self.offline {
            return;
        }

        let positioned = PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into());
        let _ = self.provider.put_blocks_no_updates(&vec![positioned]).await;
    }

    /// Spawns entities at the given local points. Each tuple is
    /// `(point, entity_id, nbt_data)` where `entity_id` is e.g. `"minecraft:sheep"`
    /// and `nbt_data` is an optional SNBT string (e.g. a `CustomName` tag). Points
    /// are local to the build area; absolute coordinates are sent to the server.
    /// No-op in offline mode (like `flush_buffer`).
    pub async fn spawn_entities(&self, entities: &[(Point3D, String, Option<String>)]) {
        if self.offline || entities.is_empty() {
            return;
        }

        let positioned: Vec<PositionedEntity> = entities
            .iter()
            .map(|(point, id, data)| {
                let abs = *point + self.build_area.origin;
                PositionedEntity {
                    x: abs.x.into(),
                    y: abs.y.into(),
                    z: abs.z.into(),
                    id: id.clone(),
                    data: data.clone(),
                }
            })
            .collect();

        // Origin 0/0/0 — entity coordinates above are already absolute.
        if let Err(e) = self.provider.put_entities(0, 0, 0, &positioned).await {
            warn!("spawn_entities: failed to spawn {} entities: {}", positioned.len(), e);
        }
    }

    pub fn get_block(&self, point: Point3D) -> Block {
        if let Some(block) = self.block_cache.borrow().get(&(point - self.build_area.origin)) {
            return block.clone();
        }

        self.world.get_block(point).expect(&format!("Block at {:?} not found in world", point))
    }

    /// Like `get_block` but returns `None` instead of panicking when the block
    /// is not in the cache or the world (e.g. synthetic/offline worlds).
    pub fn try_get_block(&self, point: Point3D) -> Option<Block> {
        if let Some(block) = self.block_cache.borrow().get(&(point - self.build_area.origin)) {
            return Some(block.clone());
        }
        self.world.get_block(point)
    }

    /// Like [`try_get_block`](Self::try_get_block) but returns only the id, skipping
    /// the clone of the block-state map (same cache-then-world lookup).
    pub fn try_get_block_id(&self, point: Point3D) -> Option<BlockID> {
        if let Some(block) = self.block_cache.borrow().get(&(point - self.build_area.origin)) {
            return Some(block.id.clone());
        }
        self.world.get_block_id(point)
    }

    /// Reads a block straight from the placement cache by its **local** coordinate
    /// — the same space `place_block` writes with. Returns `None` when nothing has
    /// been placed at `local` this run.
    ///
    /// Use this to read back blocks *this run placed*. `get_block`/`try_get_block`
    /// subtract the build-area origin (so they expect absolute coords and, given
    /// the pipeline's local coords, fall through to world terrain instead of the
    /// freshly-placed block on a live server). The cache survives `flush_buffer`,
    /// so a placed block stays readable here for the rest of the run.
    pub fn get_cached_block(&self, local: Point3D) -> Option<Block> {
        self.block_cache.borrow().get(&local).cloned()
    }

    pub async fn flush_buffer(&self) {
        // Drain the buffer first, releasing the borrow before the await
        let buffer: Vec<_> = self.block_buffer.borrow_mut().drain(..).collect();

        if buffer.is_empty() {
            return;
        }

        if self.offline {
            // Offline mode: blocks already live in block_cache; skip HTTP.
            return;
        }

        let do_updates = *self.block_updates.borrow();
        let result = self.provider.put_blocks_options(&buffer, do_updates).await.expect("Failed to send blocks");

        for (index, response) in result.iter().enumerate() {
            let point: Point3D = buffer[index].get_coordinate().into();
            let block = buffer[index].get_block();
            if response.status == 0 && self.world.get_block(point).is_none_or(|b| b != block) {
                if block.id == "air".into() && self.world.get_block(point).is_none() {
                    continue;
                }

                if self.block_cache.borrow().contains_key(&(point - self.build_area.origin)) && self.get_block(point) == block {
                    continue;
                }

                error!("Failed to place block {:?} at {:?}, world block is {:?}", block, point, self.world.get_block(point));
            }
        }
    }

    /// Every block this run placed, in build-area LOCAL coords, including air
    /// (cleared terrain). Order is unspecified.
    pub fn placed_blocks(&self) -> Vec<(Point3D, Block)> {
        self.block_cache.borrow().iter().map(|(p, b)| (*p, b.clone())).collect()
    }

    /// Snapshot the town as a [`Scene`](crate::render::Scene) in absolute coords:
    /// a terrain layer derived from the World's heightmaps and chunks, then every
    /// placed block (air included) on top. Sets no meta; callers do.
    pub fn capture_scene(&self) -> crate::render::Scene {
        let origin = self.build_area.origin;
        let world = self.world();
        let mut scene = crate::render::Scene::new(self.build_area);
        let mut put = |local: Point3D, block: &Block| scene.insert(local + origin, &block_string(block));
        // Real block from the chunks, ignoring air (missing data or a column the
        // generator since raised); callers fall back to a stand-in block.
        let solid_at = |p: Point3D| world.get_block(p).filter(|b| !b.id.is_air());

        // Top of the solid ground per column (local Y of the first cell above it):
        // the seabed for water columns, the surface otherwise.
        let solid_top = |c: Point2D| -> Option<i32> {
            if world.is_water(c) { world.get_ocean_floor_height_at(c) } else { world.get_height_at(c) }
        };

        for c in world.iter_points_2d() {
            let (Some(ground), Some(surface), Some(top)) = (world.get_height_at(c), world.get_ground_block(c), solid_top(c)) else {
                continue;
            };
            let (x, z) = (c.x, c.y);
            let at = |y: i32| Point3D::new(x, y, z);

            if surface.id.is_water() {
                // Water (or ice/kelp) from the seabed up to the surface block.
                for y in top..ground - 1 {
                    put(at(y), &solid_at(at(y)).unwrap_or_else(|| "minecraft:water".into()));
                }
                put(at(ground - 1), surface);
                put(at(top - 1), &solid_at(at(top - 1)).unwrap_or_else(|| "minecraft:sand".into()));
            } else {
                put(at(ground - 1), surface);
            }

            // Fill down to the lowest neighbouring ground so slopes and cliffs
            // render as solid faces instead of floating tiles, but at most
            // MAX_FILL_DEPTH below this column's own top so cave mouths and
            // ravines don't hang long columns below the terrain.
            const MAX_FILL_DEPTH: i32 = 8;
            let low = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter_map(|&(dx, dz)| solid_top(Point2D::new(x + dx, z + dz)))
                .fold(top, i32::min)
                .max(top - MAX_FILL_DEPTH);
            let filler = solid_at(at(top - 1)).unwrap_or_else(|| surface.clone());
            for y in low - 1..top - 1 {
                put(at(y), &solid_at(at(y)).unwrap_or_else(|| filler.clone()));
            }

            // Trees and tall plants above the no-plants surface.
            if let Some(mb) = world.get_motion_blocking_height_at(c) {
                for y in ground..mb {
                    if let Some(b) = solid_at(at(y)) {
                        put(at(y), &b);
                    }
                }
            }
        }

        for (p, b) in self.block_cache.borrow().iter() {
            put(*p, b);
        }
        scene
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub async fn give_player_book(&self, pages: &[&str], title: &str, author: &str) -> anyhow::Result<CommandResponse> {
        if self.offline {
            log::debug!("offline editor: skipping give_player_book {title:?}");
            return Ok(CommandResponse::default());
        }
        let title = if title.chars().count() > 32 {
            title.chars().take(32).collect::<String>()
        } else {
            title.to_string()
        };
        let author = if author.chars().count() > 32 {
            author.chars().take(32).collect::<String>()
        } else {
            author.to_string()
        };
        self.provider.give_player_book(pages, &title, &author).await
    }

    /// Generate a vanilla worldgen feature via the server's `place feature`
    /// command — e.g. `place feature minecraft:fancy_oak X Y Z`.
    ///
    /// `point` is in build-area-local coordinates (the origin is added to reach
    /// world space, matching [`place_block`](Self::place_block)). Unlike block
    /// placements this goes straight to the server and **bypasses the block
    /// buffer/cache**, so the placed blocks won't be visible to later
    /// `get_block` reads.
    pub async fn place_feature(&self, feature: &str, point: Point3D) -> anyhow::Result<()> {
        if self.offline {
            return Ok(());
        }
        let world = point + self.build_area.origin;
        let command = format!("place feature {} {} {} {}", feature, world.x, world.y, world.z);
        self.provider.command(vec![command]).await?;
        Ok(())
    }

    /// Set a server gamerule, e.g. `set_gamerule("block_drops", "false")`. Rule
    /// names are snake_case on 1.21.11+; the server rejects unknown names with
    /// status 0, so a typo fails silently rather than as an `Err`. Goes straight
    /// to the server; a no-op in offline mode.
    pub async fn set_gamerule(&self, rule: &str, value: &str) -> anyhow::Result<()> {
        if self.offline {
            return Ok(());
        }
        self.provider.command(vec![format!("gamerule {} {}", rule, value)]).await?;
        Ok(())
    }

    /// Set the world difficulty, e.g. `set_difficulty("easy")`. Goes straight to
    /// the server; a no-op in offline mode.
    pub async fn set_difficulty(&self, difficulty: &str) -> anyhow::Result<()> {
        if self.offline {
            return Ok(());
        }
        self.provider.command(vec![format!("difficulty {}", difficulty)]).await?;
        Ok(())
    }

    /// Spawn an entity of type `id` (e.g. `"minecraft:villager"`) at build-area-
    /// local `point`, with optional SNBT `data` (entity tags such as `NoAI`,
    /// `CustomName`, `Rotation`, …).
    ///
    /// `point` is in build-area-local coordinates (the origin is added to reach
    /// world space, matching [`place_block`](Self::place_block)). Like
    /// [`place_feature`](Self::place_feature) this goes straight to the server and
    /// **bypasses the block buffer/cache**; it is a no-op in offline mode.
    pub async fn spawn_entity(&self, id: &str, point: Point3D, data: Option<&str>) -> anyhow::Result<()> {
        if self.offline {
            return Ok(());
        }
        let world = point + self.build_area.origin;
        let entity = PositionedEntity {
            x: Coordinate::Absolute(world.x),
            y: Coordinate::Absolute(world.y),
            z: Coordinate::Absolute(world.z),
            id: id.to_string(),
            data: data.map(|s| s.to_string()),
        };
        self.provider.put_entities(0, 0, 0, &vec![entity]).await
    }

    /// Like [`spawn_entity`] but raises the entity by a fractional `y_offset`
    /// blocks (entity positions are doubles) — e.g. to stand an NPC on top of a
    /// slab. The horizontal coordinates stay on the block grid.
    pub async fn spawn_entity_offset(
        &self,
        id: &str,
        point: Point3D,
        y_offset: f32,
        data: Option<&str>,
    ) -> anyhow::Result<()> {
        if self.offline {
            return Ok(());
        }
        let world = point + self.build_area.origin;
        let entity = PositionedEntity {
            x: Coordinate::Absolute(world.x),
            y: Coordinate::AbsoluteF(world.y as f64 + y_offset as f64),
            z: Coordinate::Absolute(world.z),
            id: id.to_string(),
            data: data.map(|s| s.to_string()),
        };
        self.provider.put_entities(0, 0, 0, &vec![entity]).await
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if !self.block_buffer.borrow().is_empty() {
            error!("Editor was dropped with non-empty block buffer!");
        }
    }
}
/// `minecraft:id[k=v,...]` with sorted state keys (deterministic output);
/// bare ids get the `minecraft:` namespace.
fn block_string(block: &Block) -> String {
    let id = block.id.as_str();
    let mut out = if id.contains(':') { id.to_string() } else { format!("minecraft:{id}") };
    if let Some(state) = block.state.as_ref().filter(|s| !s.is_empty()) {
        let mut kv: Vec<_> = state.iter().collect();
        kv.sort();
        let parts: Vec<String> = kv.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        out.push('[');
        out.push_str(&parts.join(","));
        out.push(']');
    }
    out
}
