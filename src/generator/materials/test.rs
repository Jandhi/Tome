
#[cfg(test)]
mod tests {
    use log::info;

    use crate::{data::Loadable, editor::World, generator::materials::{feature::MaterialParameters, gradient::{Gradient, PerlinSettings}, placer::MaterialPlacer, Material, MaterialId}, http_mod::GDMCHTTPProvider, minecraft::BlockForm, noise::Seed, util::init_logger};

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
            Box::new(move |point| MaterialParameters {
                shade: point.x as f32 / world_rect.size.x as f32,
                wear: 0.0,
                moisture: 0.0,
                decoration: 0.0,
            }),
            &materials,
        );

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
            Box::new(move |point| {
                let shade = perlin.get(point) as f32 + 0.5;
                MaterialParameters {
                    shade,
                    wear: 0.0,
                    moisture: 0.0,
                    decoration: 0.0,
                }
            }),
            &materials,
        );

        placer.place_blocks(
            &mut editor, 
            world.world_rect_2d()
                .iter()
                .map(|point| world.add_height(point))
                .into_iter(), 
            BlockForm::Block).await;
    }

    
}