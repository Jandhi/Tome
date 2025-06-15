use std::collections::HashMap;

use anyhow::Ok;
use log::{error, info, warn};

use crate::{data::Loadable, editor::World, generator::materials::{Material, MaterialId}, geometry::{Point3D, Rect3D}, http_mod::{GDMCHTTPProvider, PositionedBlock}, minecraft::{Block, BlockForm, BlockID}, noise::RNG};

#[derive(Debug)]
pub struct Editor {
    build_area: Rect3D,
    provider : GDMCHTTPProvider,
    block_buffer : Vec<PositionedBlock>,
    buffer_size : usize,
    block_cache : HashMap<Point3D, Block>,
    world : World,
    materials : HashMap<MaterialId, Material>,
    block_form_cache : HashMap<BlockID, BlockForm>,
}

impl Editor {
    // Note: You will need to update the new() function to accept a &'a mut World parameter
    pub fn new(build_area: Rect3D, world: World) -> Self {
        let mut editor = Self {
            build_area,
            provider: GDMCHTTPProvider::new(),
            block_buffer: Vec::new(),
            buffer_size: 32,
            block_cache: HashMap::new(),
            world,
            materials: HashMap::new(),
            block_form_cache: HashMap::new(),
        };
        editor.load_data().expect("Failed to load materials");
        editor
    }

    pub fn set_buffer_size(&mut self, size: usize) {
        self.buffer_size = size;
    }

    fn load_data(&mut self) -> anyhow::Result<()> {
        info!("Loading editor data");
        self.materials = Material::load()?;
        Ok(())
    }

    pub async fn place_block(&mut self,  block : &Block, point : Point3D) {
        if !self.world.build_area.contains(point + self.build_area.origin) {
            warn!("Point {:?} is outside the build area {:?} and will be ignored", point + self.build_area.origin, self.world.build_area);
            return;
        }

        if block.id == BlockID::Unknown {
            warn!("Attempted to place an unknown block at {:?}, skipping", point);
            return;
        }

        if self.block_cache.contains_key(&(point)) {
            let current_block = self.block_cache.get(&(point)).expect("Block should be in cache").id;

            if self.get_block_form(block.id).density() <= self.get_block_form(current_block).density() {
                info!("Block at {:?} is already placed with a denser block, skipping", point);
                return;
            }
        }

        self.block_cache.insert(point, block.clone());
        self.block_buffer.push(PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into()));
        if self.block_buffer.len() >= self.buffer_size {
            self.flush_buffer().await;
        }
    }

    fn get_block_form(&mut self, id : BlockID) -> BlockForm {
        if !self.block_form_cache.contains_key(&id) {
            let form = BlockForm::infer_from_block(id);
            self.block_form_cache.insert(id, form.clone());
        }

        *self.block_form_cache.get(&id).expect("Block form not found")
    }

    pub async fn place_block_chance(&mut self, block : &Block, point : Point3D, rng : &mut RNG, chance : i32) {

        if rng.rand_i32_range(1, 100) <= chance {
            self.place_block(block, point).await;
        }
    }

    pub fn get_block(&mut self, point : Point3D) -> Block {
        if let Some(block) = self.block_cache.get(&(point - self.build_area.origin)) {
            return block.clone();
        }

        self.world.get_block(point).expect("Failed to get block from world")
    }

    pub async fn flush_buffer(&mut self) {
        let result = self.provider.put_blocks(&self.block_buffer).await.expect("Failed to send blocks");
        
        for (index, response) in result.iter().enumerate() {
            let point : Point3D = self.block_buffer[index].get_coordinate().into();
            let block = self.block_buffer[index].get_block();
            if response.status == 0 && self.world.get_block(point).is_none_or(|b| b != block) {
                if block.id == BlockID::Air && self.world.get_block(point).is_none() {
                    continue;
                }

                if self.block_cache.contains_key(&(point - self.build_area.origin)) && self.get_block(point) == block {
                    continue;
                }
                
                error!("Failed to place block {:?} at {:?}, world block is {:?}", block, point, self.world.get_block(point));
            }
        }
        
        self.block_buffer.clear();
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if !self.block_buffer.is_empty() {
            error!("Editor was dropped with non-empty block buffer!");
        }
    }
}