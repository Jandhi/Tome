
#[cfg(test)]
mod tests {
    use log::info;

    use crate::{data::Loadable, editor::World, generator::materials::{gradient::{Gradient, PerlinSettings}, placer::Placer, Material, MaterialId}, geometry::Point3D, http_mod::GDMCHTTPProvider, minecraft::BlockForm, util::init_logger};

    #[test]
    fn deserialize_material() {
        use crate::generator::materials::{Material, MaterialFeature};
        use serde_json::json;

        let json_data = json!({
            "id": "test_material",
            "connections": {
                "lighter": "lighter_material",
                "darker": "darker_material",
                "less_worn": "less_worn_material",
                "more_decorated": "more_decorated_material",
                "less_decorated": "less_decorated_material"
            },
            "blocks": {
                "block" : "minecraft:stone",
            }
        });

        let material: Material = serde_json::from_value(json_data).unwrap();

        assert_eq!(material.id().as_str(), "test_material");
        assert_eq!(material.more(MaterialFeature::Shade).unwrap().as_str(), "lighter_material");
        assert_eq!(material.less(MaterialFeature::Shade).unwrap().as_str(), "darker_material");
    }

    #[test]
    fn load_materials() {
        init_logger();

        let materials = Material::load()
            .expect("Failed to load materials");

        info!("Loaded {} materials", materials.len());
    }

    #[tokio::test]
    async fn test_linear_mapping() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());
        let world_rect = editor.world_mut().world_rect_2d();
        let placer : Placer = Placer::new(
            &materials
        ).with_shade_function(move |point| {
            point.x as f32 / world_rect.size.x as f32
        });

        for point in editor.world_mut().world_rect_2d().clone().iter() {
            let point = editor.world_mut().add_height(point);
            placer.place_block(&mut editor, point, &material, BlockForm::Block, None, None).await;
        }
    }

    #[tokio::test]
    async fn perlin() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());

        let perlin = PerlinSettings::large(42.into());
        let placer: Placer = Placer::new(
            &materials,
        ).with_shade_function(move |point| {
            perlin.get(point) as f32 + 0.5
        });


        let points : Vec<Point3D> = editor.world_mut()
            .world_rect_2d()
            .clone()
            .iter()
            .map(|point| editor.world_mut().add_height(point))
            .collect();
        placer.place_blocks(
            &mut editor, 
            points.into_iter(),
            &material,
            BlockForm::Block, 
            None, 
            None).await;
    }

    #[tokio::test]
    async fn gradient() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());

        let gradient = Gradient::new(PerlinSettings::small(25.into()), 1.0, 0.05)
            .with_x(0, editor.world_mut().build_area.width());

        let placer: Placer = Placer::new(
            &materials,
        ).with_shade_function(move |point| {
            info!("Point: {:?}", gradient.get_value(point));
            gradient.get_value(point)
        });

        let points : Vec<Point3D> = editor.world_mut()
            .world_rect_2d()
            .clone()
            .iter()
            .map(|point| editor.world_mut().add_height(point))
            .collect();
        placer.place_blocks(
            &mut editor, 
            points.into_iter(),
            &material,
            BlockForm::Block,
            None,
            None).await;
    }
    
}