
#[cfg(test)]
mod tests {
    use log::info;

    use crate::{data::Loadable, editor::World, generator::materials::{gradient::{Gradient, PerlinSettings}, placer::MaterialPlacer, Material, MaterialId}, http_mod::GDMCHTTPProvider, minecraft::BlockForm, util::init_logger};

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
        let mut world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());
        let world_rect = world.world_rect_2d();
        let placer : MaterialPlacer = MaterialPlacer::new(
            material.clone(),
            &materials,
        ).with_shade_function(move |point| {
            point.x as f32 / world_rect.size.x as f32
        });

        for point in world.world_rect_2d().iter() {
            placer.place_block(&mut editor, world.add_height(point), BlockForm::Block).await;
        }
    }

    #[tokio::test]
    async fn perlin() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let mut world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());

        let perlin = PerlinSettings::large(42.into());
        let placer: MaterialPlacer = MaterialPlacer::new(
            material.clone(),
            &materials,
        ).with_shade_function(move |point| {
            perlin.get(point) as f32 + 0.5
        });

        placer.place_blocks(
            &mut editor, 
            world.world_rect_2d()
                .iter()
                .map(|point| world.add_height(point))
                .into_iter(), 
            BlockForm::Block).await;
    }

    #[tokio::test]
    async fn gradient() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let mut world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");
        let material = MaterialId::new("cobblestone".to_string());

        let gradient = Gradient::new(PerlinSettings::small(25.into()), 1.0, 0.05)
            .with_x(0, world.build_area.width());

        let placer: MaterialPlacer = MaterialPlacer::new(
            material.clone(),
            &materials,
        ).with_shade_function(move |point| {
            info!("Point: {:?}", gradient.get_value(point));
            gradient.get_value(point)
        });

        placer.place_blocks(
            &mut editor, 
            world.world_rect_2d()
                .iter()
                .map(|point| world.add_height(point))
                .into_iter(), 
            BlockForm::Block).await;
    }
    
}