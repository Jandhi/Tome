
use http_mod::PositionedBlock;

pub mod geometry;
pub mod minecraft;
pub mod http_mod;
pub mod editor;
pub mod generator;
pub mod noise;

#[tokio::main]
async fn main() {
    let provider = http_mod::GDMCHTTPProvider::new();
    

    let build_area = provider.get_build_area().await.expect("Failed to get build area");

    let mut blocks : Vec<PositionedBlock> = vec![];

    let world = provider.get_blocks(build_area.origin.x, build_area.origin.y, build_area.origin.z, build_area.size.x, build_area.size.y, build_area.size.z).await.expect("Failed to get blocks");

    
}