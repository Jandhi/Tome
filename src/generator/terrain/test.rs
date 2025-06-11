
#[cfg(test)]
mod tests {
    use std::hash::Hash;
    use std::collections::HashMap;
    use log::info;
    use schemars::gen;

    use crate::{data::Loadable, editor::{World, Editor}, generator::terrain::{generate_tree, Forest, Tree}, util::init_logger, noise::{RNG, Seed}, http_mod::GDMCHTTPProvider, geometry::Point3D};

    #[test]
    fn deserialize_tree() {
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
    fn load_forests() {
        init_logger();

        let forests = Forest::load()
            .expect("Failed to load forests");

        info!("Loaded {} forests", forests.len());
    }

    
    #[tokio::test]
    async fn tree_generation() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let mut editor = Editor::new(build_area);

        let mut palette: HashMap<String, HashMap<String, f32>> = HashMap::new();

        let wood: HashMap<String, f32> = [
            ("minecraft:birch_wood".to_string(), 5.0),
            ("minecraft:stripped_birch_wood".to_string(), 2.0),
            ("minecraft:stripped_oak_wood".to_string(), 1.0),
        ].into_iter().collect();
        palette.insert("wood".to_string(), wood);

        let leaves: HashMap<String, f32> = [
            ("minecraft:oak_leaves[persistent=true]".to_string(), 1.0),
            ("minecraft:acacia_leaves[persistent=true]".to_string(), 2.0),
            ("minecraft:birch_leaves[persistent=true]".to_string(), 5.0),
        ].into_iter().collect();
        palette.insert("leaves".to_string(), leaves);


        generate_tree(Tree::MegaBirch, &mut editor, Point3D::new(0, 0, 0), &mut rng, &palette).await;

        editor.flush_buffer().await;
    }

}