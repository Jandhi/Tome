use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::Ok;
use log::{error, info, warn};

use crate::{data::Loadable, editor::World, generator::materials::{Material, MaterialId}, geometry::{Point3D, Rect3D}, http_mod::{CommandResponse, GDMCHTTPProvider, PositionedBlock, PositionedEntity}, minecraft::{Block, BlockForm, BlockID}, noise::RNG};

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
    block_cache: RefCell<HashMap<Point3D, Block>>,
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
    pub fn new(build_area: Rect3D, world: World) -> Self {
        let mut editor = Self {
            build_area,
            provider: GDMCHTTPProvider::new(),
            block_buffer: RefCell::new(Vec::new()),
            buffer_size: 32,
            block_cache: RefCell::new(HashMap::new()),
            world,
            materials: HashMap::new(),
            block_form_cache: RefCell::new(HashMap::new()),
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

        let result = self.provider.put_blocks(&buffer).await.expect("Failed to send blocks");

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

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub async fn give_player_book(&self, pages: &[&str], title: &str, author: &str) -> anyhow::Result<CommandResponse> {
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
}

impl Drop for Editor {
    fn drop(&mut self) {
        if !self.block_buffer.borrow().is_empty() {
            error!("Editor was dropped with non-empty block buffer!");
        }
    }
}