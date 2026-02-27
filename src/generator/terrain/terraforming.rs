use std::collections::HashSet;

use crate::{editor::Editor, geometry::Point3D};

pub async fn force_height(editor: &mut Editor, points: &HashSet<Point3D>, skip_water : bool) {
    for point in points {
        if editor.world().is_water(point.drop_y()) && skip_water {
            continue;
        }

        let height = editor.world().get_ocean_floor_height_at(point.drop_y());

        let surface_block = editor.world().get_block(point.with_y(height)).expect("Expected a block at the surface");

        if surface_block.id.is_water() && skip_water {
            continue;
        }

        if height < point.y {
            for y in height..=point.y {
                editor.place_block_forced(&surface_block, point.with_y(y)).await;
            }
        } else {
            for y in point.y..=height {
                editor.place_block_forced(&"air".into(), point.with_y(y)).await;
            }
        }
    }
    
    editor.world_mut().set_heights(points);
}