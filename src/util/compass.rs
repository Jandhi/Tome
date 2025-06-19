use crate::{editor::Editor, geometry::{EAST, NORTH, SOUTH, UP, WEST}, minecraft::BlockID};

pub async fn build_compass(editor : &mut Editor) {
    let midpoint = editor.world_mut().world_rect_2d().size / 2;
    let point = editor.world_mut().add_height(midpoint);
    let offset = UP * 30;
    let point = point + offset;

    editor.place_block(&BlockID::RedWool.into(), point + NORTH).await;
    editor.place_block(&BlockID::Stone.into(), point).await;
    editor.place_block(&BlockID::BlueWool.into(), point + SOUTH).await;
    editor.place_block(&BlockID::GreenWool.into(), point + EAST).await;
    editor.place_block(&BlockID::OrangeWool.into(), point + WEST).await;
}