
#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::collections::HashMap;
    use log::info;
    

    use crate::{data::Loadable, editor::{World, Editor}, generator::terrain::{generate_tree, generate_tree_feature, Forest, Tree, ForestId, log_trees}, util::init_logger, noise::{RNG, Seed}, http_mod::GDMCHTTPProvider, generator::districts::plant_forest,  geometry::{Point2D, Point3D}};

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
    async fn build_forest() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);
        let provider = GDMCHTTPProvider::new(); 
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = Editor::new(build_area, world);

        let forests = Forest::load().expect("Failed to load forests");
        let forest_id = ForestId::new("birch_forest".to_string());
        let forest = forests.get(&forest_id).expect("Failed to get birch forest").clone();
        let points = HashSet::from_iter(editor.world_mut().world_rect_2d().iter());


        plant_forest(&points, forest, &mut rng, &mut editor, None, true).await
    }

    
    #[tokio::test]
    async fn tree_line_generation() {
        init_logger();

        // Initialize the test data
        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();

        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = Editor::new(build_area, world);

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

        generate_tree(Tree::SmallBirch, &mut editor, Point3D::new(100, 0, 0), &mut rng, &palette).await;
        generate_tree(Tree::MediumBirch, &mut editor, Point3D::new(100, 0, 10), &mut rng, &palette).await;
        generate_tree(Tree::LargeBirch, &mut editor, Point3D::new(100, 0, 20), &mut rng, &palette).await;
        generate_tree(Tree::MegaBirch, &mut editor, Point3D::new(100, 0, 30), &mut rng, &palette).await;
        generate_tree(Tree::SmallHedge, &mut editor, Point3D::new(100, 0, 40), &mut rng, &palette).await;
        generate_tree(Tree::MediumHedge, &mut editor, Point3D::new(100, 0, 50), &mut rng, &palette).await;
        generate_tree(Tree::LargeHedge, &mut editor, Point3D::new(100, 0, 60), &mut rng, &palette).await;
        generate_tree(Tree::MegaHedge, &mut editor, Point3D::new(100, 0, 70), &mut rng, &palette).await;
        generate_tree(Tree::SmallOak, &mut editor, Point3D::new(100, 0, 80), &mut rng, &palette).await;
        generate_tree(Tree::MediumOak, &mut editor, Point3D::new(100, 0, 90), &mut rng, &palette).await;
        generate_tree(Tree::LargeOak, &mut editor, Point3D::new(100, 0, 100), &mut rng, &palette).await;
        generate_tree(Tree::MegaOak, &mut editor, Point3D::new(100, 0, 110), &mut rng, &palette).await;
        generate_tree(Tree::SmallPine, &mut editor, Point3D::new(100, 0, 120), &mut rng, &palette).await;
        generate_tree(Tree::MediumPine, &mut editor, Point3D::new(100, 0, 130), &mut rng, &palette).await;
        generate_tree(Tree::LargePine, &mut editor, Point3D::new(100, 0, 140), &mut rng, &palette).await;
        generate_tree(Tree::MegaPine, &mut editor, Point3D::new(100, 0, 150), &mut rng, &palette).await;

        editor.flush_buffer().await;
    }

    /// Scatter a bunch of random vanilla trees (every species/size in the `Tree`
    /// enum, mapped to a `place feature` call) across the build area, each rooted
    /// at ground height. Needs a live server.
    /// Run with: `cargo test scatter_feature_trees -- --nocapture`.
    #[tokio::test]
    async fn scatter_feature_trees() {
        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let world = World::new(&provider).await.expect("Failed to create world");
        let editor = Editor::new(build_area, world);

        const SPECIES: [Tree; 24] = [
            Tree::SmallOak, Tree::MediumOak, Tree::LargeOak, Tree::MegaOak,
            Tree::SmallBirch, Tree::MediumBirch, Tree::LargeBirch, Tree::MegaBirch,
            Tree::SmallPine, Tree::MediumPine, Tree::LargePine, Tree::MegaPine,
            Tree::SmallJungle, Tree::MediumJungle, Tree::LargeJungle, Tree::MegaJungle,
            Tree::SmallHedge, Tree::MediumHedge, Tree::LargeHedge, Tree::MegaHedge,
            Tree::SmallBaobab, Tree::MediumBaobab, Tree::LargeBaobab, Tree::MegaBaobab,
        ];

        // Work in build-area-local coordinates (see World::new — heightmaps are
        // local to origin.y, and the editor adds the origin back on placement).
        let size = editor.world().world_rect_2d().size;
        let margin = 6;
        if size.x <= 2 * margin || size.y <= 2 * margin {
            panic!("Build area {:?} too small to scatter trees", size);
        }

        let target = 40;
        let min_gap = 7;
        let mut planted: Vec<Point2D> = Vec::new();
        let mut attempts = 0;
        while planted.len() < target && attempts < target * 30 {
            attempts += 1;
            let c = Point2D::new(
                rng.rand_i32_range(margin, size.x - margin),
                rng.rand_i32_range(margin, size.y - margin),
            );
            if planted.iter().any(|p| (p.x - c.x).abs() < min_gap && (p.y - c.y).abs() < min_gap) {
                continue;
            }
            planted.push(c);

            let tree = SPECIES[rng.rand_i32(SPECIES.len() as i32) as usize];
            let height = editor.world().get_ocean_floor_height_at(c);
            generate_tree_feature(tree, &editor, Point3D::new(c.x, height, c.y), &mut rng)
                .await
                .expect("place feature failed");
        }

        info!("Planted {} feature trees in {} attempts", planted.len(), attempts);
    }

    #[tokio::test]
    async fn cut_trees() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        println!("Build area: {:?}", build_area);
        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();
        let mut points = HashSet::new();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                points.insert(Point2D::new(x, z));
            }
        }

        log_trees(&mut editor, points).await;
        editor.flush_buffer().await;

    }

}