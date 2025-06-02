
#[cfg(test)]
mod tests {
    use log::info;

    use crate::{data::Loadable, editor::{Editor, World}, generator::materials::{feature::{map_feature, MaterialFeatureMapping}, Material, MaterialFeature, MaterialId}, http_mod::GDMCHTTPProvider, minecraft::BlockForm, util::init_logger};

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

        let width = world.world_rect_2d().size.x as f32;
        for point in world.world_rect_2d().iter() {
            let shade = point.x as f32 / width;

            info!("Mapping shade value: {}", shade);
            let id = map_feature(shade, &material, MaterialFeature::Shade, &materials, MaterialFeatureMapping::Fitted);
            let block = materials.get(&id).expect("Material not found").get_block(&BlockForm::Block)
                .expect("Block not found");

            editor.place_block(&block.into(), world.add_height(point)).await;
        }
    }
}