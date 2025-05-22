use crate::{geometry::Point3D, minecraft::Block};

pub trait Placer {
    fn place_block(&mut self, block : &Block, point : Point3D) -> impl std::future::Future<Output = ()> + Send;
}