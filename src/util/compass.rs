use crate::{editor::Editor, geometry::{EAST, NORTH, SOUTH, UP, WEST}};

pub async fn build_compass(editor: &Editor) {
    let midpoint = editor.world().world_rect_2d().size / 2;
    let point = editor.world().add_height(midpoint);
    let offset = UP * 30;
    let point = point + offset;

    editor.place_block(&"red_wool".into(), point + NORTH).await;
    editor.place_block(&"stone".into(), point).await;
    editor.place_block(&"blue_wool".into(), point + SOUTH).await;
    editor.place_block(&"green_wool".into(), point + EAST).await;
    editor.place_block(&"orange_wool".into(), point + WEST).await;
}