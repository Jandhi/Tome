use crate::editor::World;
use crate::generator::buildings_v2::footprint::{generate_footprint, Footprint, Plot, SizeClass};
use crate::generator::buildings_v2::foundation::place_foundation;
use crate::generator::buildings_v2::BuildCtx;
use crate::generator::data::LoadedData;
use crate::generator::materials::PaletteId;
use crate::geometry::{Point2D, Rect2D};
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::RNG;
use crate::util::init_logger;

/// Generate footprints in a plot, marking each as unusable for the next.
fn fill_plot(rng: &mut RNG, plot: &mut Plot, size_class: &SizeClass, max: usize) -> Vec<Footprint> {
    let mut footprints = Vec::new();
    let plot_min = plot.bounds.min();
    for _ in 0..max {
        let footprint = match generate_footprint(rng, plot, size_class) {
            Some(f) => f,
            None => break,
        };
        for point in footprint.filled_points() {
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let p = Point2D::new(point.x + dx, point.y + dz);
                    let lx = (p.x - plot_min.x) as usize;
                    let lz = (p.y - plot_min.y) as usize;
                    if lx < plot.usable.len() && lz < plot.usable[0].len() {
                        plot.usable[lx][lz] = false;
                    }
                }
            }
        }
        footprints.push(footprint);
    }
    footprints
}

#[tokio::test]
async fn build_foundations_in_world() {
    init_logger();

    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.unwrap();
    let mut editor = world.get_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette_id: PaletteId = "medieval_spruce".into();
    let palette = data.palettes.get(&palette_id).expect("Palette not found").clone();

    let world_rect = editor.world().world_rect_2d();
    let center = world_rect.midpoint();

    // Central 32x32 area
    let plot_min = Point2D::new(center.x - 16, center.y - 16);
    let plot_max = Point2D::new(center.x + 15, center.y + 15);
    let bounds = Rect2D::from_points(plot_min, plot_max);
    let mut plot = Plot::fully_usable(bounds);

    let mut rng = RNG::new(42);
    let footprints = fill_plot(&mut rng, &mut plot, &SizeClass::House, 20);
    println!("Placed {} house footprints in 32x32 area", footprints.len());

    let mut ctx = BuildCtx::new(&mut editor, &data, &palette, &mut rng);
    for (i, footprint) in footprints.iter().enumerate() {
        let base_y = place_foundation(&mut ctx, footprint).await;
        let area = footprint.filled_points().len();
        println!("  Foundation {}: base_y={}, area={}", i, base_y, area);
    }

    editor.flush_buffer().await;
    println!("Done — {} foundations placed and flushed", footprints.len());
}
