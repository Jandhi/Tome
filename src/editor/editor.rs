use std::collections::HashMap;

use crate::{geometry::{Point3D, Rect3D}, http_mod::{GDMCHTTPProvider, PositionedBlock}, minecraft::Block};

use super::Placer;

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

    pub async fn get_block(&mut self, point : Point3D) -> Block {
        if let Some(block) = self.block_cache.get(&point) {
            return block.clone();
        }

        let x = (point.x >> 3) << 3;
        let y = (point.y >> 3) << 3;
        let z = (point.z >> 3) << 3;

        let dx = x + 8;
        let dy = y + 8;
        let dz = z + 8;

        let blocks = self.provider.get_blocks(x, y, z, dx, dy, dz).await.expect("Failed to get block");
        
        for block in blocks.iter() {
            self.block_cache.insert(block.get_coordinate().into(), block.get_block());
        }

        self.block_cache.get(&point).expect("Block not found").clone()
    }

    pub async fn flush_buffer(&mut self) {
        self.provider.put_blocks(&self.block_buffer).await.expect("Failed to send blocks");
        self.block_buffer.clear();
    }
}

impl Placer for Editor {
    fn place_block(&mut self, block: &Block, point: Point3D) -> impl std::future::Future<Output = ()> + Send {
        let block = block.clone();
        let point = point.clone();
        async move {
            self.place_block(&block, point).await;
        }
    }
}