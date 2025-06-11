use std::collections::HashMap;

use crate::{editor::World, geometry::{Point3D, Rect3D}, http_mod::{GDMCHTTPProvider, PositionedBlock}, minecraft::Block, noise::RNG};

#[derive(Debug, Clone)]
pub struct Editor {
    build_area: Rect3D,
    provider : GDMCHTTPProvider,
    block_buffer : Vec<PositionedBlock>,
    buffer_size : usize,
    block_cache : HashMap<Point3D, Block>,
}

impl Editor {
    pub fn new(build_area: Rect3D) -> Self {
        Self {
            build_area,
            provider: GDMCHTTPProvider::new(),
            block_buffer: Vec::new(),
            buffer_size: 32,
            block_cache: HashMap::new(),
        }
    }

    pub async fn place_block(&mut self, block : &Block, point : Point3D) {
        self.block_cache.insert(point, block.clone());
        self.block_buffer.push(PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into()));
        if self.block_buffer.len() >= self.buffer_size {
            self.flush_buffer().await;
        }
    }

    pub async fn place_block_chance(&mut self, block : &Block, point : Point3D, rng : &mut RNG, chance : i32) {
        if rng.rand_i32_range(1, 100) <= chance {
            self.block_cache.insert(point, block.clone());
            self.block_buffer.push(PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into()));
            if self.block_buffer.len() >= self.buffer_size {
                self.flush_buffer().await;
            }
        }
    }

    pub fn get_block(&mut self, point : Point3D, world : &World) -> Block {
        if let Some(block) = self.block_cache.get(&(point - self.build_area.origin)) {
            return block.clone();
        }

        world.get_block(point).expect("Failed to get block from world")
    }

    pub async fn flush_buffer(&mut self) {
        self.provider.put_blocks(&self.block_buffer).await.expect("Failed to send blocks");
        self.block_buffer.clear();
    }
}