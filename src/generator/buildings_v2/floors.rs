use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{MaterialPlacer, MaterialRole, Palette, Placer};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::BlockForm;
use crate::noise::RNG;
use super::frame::Frame;

/// Place floor slabs for upper stories and ceiling on the top floor.
/// Ground floor is handled by foundation.
/// Uses top slabs at floor_y - 1 so they sit flush with the beam level.
pub async fn place_floors(
    editor: &Editor,
    frame: &Frame,
    data: &LoadedData,
    palette: &Palette,
    rng: &mut RNG,
) {
    let material_id = palette
        .get_material(MaterialRole::PrimaryWood)
        .expect("No primary wood material")
        .clone();

    let mut placer_rng = rng.derive();
    let mut placer = MaterialPlacer::new(
        Placer::new(&data.materials, &mut placer_rng),
        material_id,
    );

    let slab_state = std::collections::HashMap::from([
        ("type".to_string(), "top".to_string()),
    ]);

    // Upper floor slabs
    for floor in frame.floors() {
        if floor == 0 {
            continue; // ground floor covered by foundation
        }

        let y = frame.floor_y(floor) - 1;
        let points = frame.filled_points_at_floor(floor);

        for point in &points {
            placer.place_block(
                editor,
                Point3D::new(point.x, y, point.y),
                BlockForm::Slab,
                Some(&slab_state),
                None,
            ).await;
        }
    }

    // Ceiling slabs at top of each rect (roof_y - 1)
    let rects = frame.footprint().rects();
    let mut placed: std::collections::HashSet<(i32, i32, i32)> = std::collections::HashSet::new();
    for i in 0..rects.len() {
        let y = frame.roof_y(i) - 2;
        for point in rects[i].iter() {
            if placed.insert((point.x, y, point.y)) {
                placer.place_block(
                    editor,
                    Point3D::new(point.x, y, point.y),
                    BlockForm::Slab,
                    Some(&slab_state),
                    None,
                ).await;
            }
        }
    }
}
