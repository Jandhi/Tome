
#[cfg(test)]
mod tests {

    use log::info;

    use crate::{data::Loadable, editor::World, generator::{buildings::{walls::Wall, Grid}, materials::{Material, Palette}, nbts::{Rotation, Structure}}, geometry::Point3D, http_mod::GDMCHTTPProvider, util::init_logger};


    #[tokio::test]
    async fn grid_placement() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");

        let palettes = Palette::load().expect("Failed to load palettes");
        let input_palette = palettes.get("test1").expect("Default palette not found");
        let output_palette = palettes.get("test2").expect("Default palette not found");

       
        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let structures = Structure::load().expect("Failed to load structures");
        let structure = structures.get(&"rotation_test".into()).expect("Structure not found");
        
        grid.build_structure(&mut editor, &structure, Point3D::new(0, 0, 0), Rotation::None, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &structure, Point3D::new(0, 1, 0), Rotation::Once, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &structure, Point3D::new(0, 2, 0), Rotation::Twice, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &structure, Point3D::new(0, 3, 0), Rotation::Thrice, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }


    #[tokio::test]
    async fn grid_placement_wall() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let world = World::new(&provider).await.unwrap();
        let mut editor = world.get_editor();
        let materials = Material::load().expect("Failed to load materials");

        let palettes = Palette::load().expect("Failed to load palettes");
        let input_palette = palettes.get("test1").expect("Default palette not found");
        let output_palette = palettes.get("test2").expect("Default palette not found");

       
        let midpoint = editor.world().world_rect_2d().size / 2;
        let point = editor.world().add_height(midpoint);

        println!("Placing structure at: {:?}", point);

        let grid = Grid::new(point.into());

        let walls = Wall::load().expect("Failed to load structures");
        let wall = walls.get(&"japanese_wall_single_plain".into()).expect("Structure not found");
        
        grid.build_structure(&mut editor, &wall.structure, Point3D::new(0, 0, 0), Rotation::None, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &wall.structure, Point3D::new(0, 0, 0), Rotation::Once, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &wall.structure, Point3D::new(0, 0, 0), Rotation::Twice, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");
        grid.build_structure(&mut editor, &wall.structure, Point3D::new(0, 0, 0), Rotation::Thrice, &materials, input_palette, output_palette).await
            .expect("Failed to build structure");

        info!("NBT structure placed successfully");

        editor.flush_buffer().await;
    }
    
}