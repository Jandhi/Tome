use crate::{editor::Editor, geometry::{EAST, NORTH, SOUTH, UP, WEST}, minecraft::{BasicStone, BlockID, ColoredBlock, Stone, Wool}};

pub async fn build_compass(editor : &mut Editor) {
    let midpoint = editor.world_mut().world_rect_2d().size / 2;
    let point = editor.world_mut().add_height(midpoint);
    let offset = UP * 30;
    let point = point + offset;

    editor.place_block(&BlockID::ColoredBlock(ColoredBlock::Wool(Wool::Red)).into(), point + NORTH).await;
    editor.place_block(&BlockID::Stone(Stone::BasicStone(BasicStone::Stone)).into(), point).await;
    editor.place_block(&BlockID::ColoredBlock(ColoredBlock::Wool(Wool::Blue)).into(), point + SOUTH).await;
    editor.place_block(&BlockID::ColoredBlock(ColoredBlock::Wool(Wool::Green)).into(), point + EAST).await;
    editor.place_block(&BlockID::ColoredBlock(ColoredBlock::Wool(Wool::Orange)).into(), point + WEST).await;
}