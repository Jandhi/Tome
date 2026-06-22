//! Ship v2 tests. The offline build mirrors v1's `build_*_offline`: a synthetic
//! water world + offline editor, no server. The live `build_ship_v2` places keels
//! into the running server's build area for the screenshot loop.

use crate::generator::ships_v2::keel;

/// The keel geometry is well-formed across a range of (no-rowboat) lengths,
/// including several random sizes.
#[test]
fn keel_model_property_test() {
    // v2 has a minimum ship size — no rowboat. Smallest keel ~14, largest ~46.
    let mut rng = crate::noise::RNG::new(99);
    let mut lengths = vec![14, 20, 30, 38, 46];
    for _ in 0..8 {
        lengths.push(rng.rand_i32_range(14, 47));
    }

    for length in lengths {
        let model = keel::build_keel_model(length);

        assert!(!model.cells.is_empty(), "length {length}: empty keel");
        assert_eq!(model.waterline_y, model.depth, "length {length}: waterline == depth");
        assert!(model.depth >= 1, "length {length}: keel must have depth");

        // Depth scales with length: ~30 → 5–6, smallest → 1–2.
        if length == 30 {
            assert!((5..=6).contains(&model.depth), "length 30 depth was {}", model.depth);
        }

        // Sternpost is a full vertical column at x=0 reaching the waterline.
        let post_top = model
            .cells
            .iter()
            .filter(|c| c.local.x == 0)
            .map(|c| c.local.y)
            .max()
            .unwrap();
        assert_eq!(post_top, model.depth, "length {length}: sternpost should reach the waterline");

        // The bow rake reaches the bow tip and the waterline.
        let max_x = model.cells.iter().map(|c| c.local.x).max().unwrap();
        assert_eq!(max_x, length - 1, "length {length}: keel should span to the bow tip");
        let bow_top = model.cells.iter().map(|c| c.local.y).max().unwrap();
        assert_eq!(bow_top, model.depth, "length {length}: rake should rise to the waterline");
    }
}

/// Full offline pipeline: build a keel on a water flatworld, verify the sternpost,
/// flat run and bow rake landed, and write the ASCII side profile.
#[tokio::test]
async fn build_ship_v2_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships_v2::blueprint::{render_hull_plan, render_keel_ascii};
    use crate::generator::ships_v2::{build_ship_v2, ShipV2Context, ShipV2Ctx};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::minecraft::Block;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    // Water flatworld: seabed at y=50, sea surface at y=64.
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    let mut rng = RNG::new(7);
    let context = ShipV2Context::new(Cardinal::North, 30);
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipV2Ctx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship_v2(&mut ctx, &context, anchor).await;
    editor.flush_buffer().await;

    assert!(ship.on_water, "anchor over a water flatworld should be detected as water");

    let keel = &ship.keel;
    let place = &ship.placement;
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();
    let solid_at = |p: Point3D| {
        editor.try_get_block(p).as_ref().map_or(false, |b| !is_air(b))
    };

    // Sternpost base is solid.
    assert!(solid_at(place.to_world(Point3D::new(0, 0, 0))), "sternpost base missing");
    // Sternpost rises to the waterline.
    assert!(
        solid_at(place.to_world(Point3D::new(0, keel.depth, 0))),
        "sternpost should reach the waterline"
    );
    // A flat-run slab amidships is present.
    let mid_x = keel.length / 2;
    assert!(solid_at(place.to_world(Point3D::new(mid_x, 0, 0))), "flat run missing amidships");
    // The bow tip is present at the waterline.
    assert!(
        solid_at(place.to_world(Point3D::new(keel.length - 1, keel.depth, 0))),
        "bow rake should reach the waterline at the stem"
    );

    // Hull shell: a side cell at the widest layer (waterline) is placed.
    let hull = &ship.hull;
    assert!(!hull.cells.is_empty(), "hull shell should have cells");

    // The hull respects the keel: every hull cell sits strictly above the keel's
    // crest at its station, so the keel stays the outermost, water-touching part.
    let top = keel.top_profile();
    for c in &hull.cells {
        let kt = top[c.x as usize];
        assert!(
            kt == i32::MIN || c.y > kt,
            "hull cell {c:?} is not above the keel crest ({kt}) at x={}",
            c.x,
        );
    }
    let side = hull
        .cells
        .iter()
        .find(|c| c.y == hull.depth && c.z != 0)
        .expect("hull should have a side cell at the waterline");
    assert!(
        solid_at(place.to_world(*side)),
        "expected hull shell block at {side:?}",
    );

    // On water, the hollow interior is cleared to air (dry hull).
    if !hull.interior.is_empty() {
        let inside = place.to_world(hull.interior[0]);
        assert!(
            editor.try_get_block(inside).as_ref().map_or(false, |b| is_air(b)),
            "hull interior should be cleared to air at {inside:?}, got {:?}",
            editor.try_get_block(inside),
        );
    }

    // Rudder: a raked blade block and a fence attachment are placed aft of the stern.
    let rud = &ship.rudder;
    assert!(!rud.blade.is_empty(), "rudder should have a blade");
    assert!(!rud.fences.is_empty(), "rudder should have fence attachments");
    assert!(
        solid_at(place.to_world(rud.blade[0].local)),
        "expected rudder blade block at {:?}",
        place.to_world(rud.blade[0].local),
    );
    let fence_world = place.to_world(rud.fences[0]);
    assert!(
        editor.try_get_block(fence_world).map_or(false, |b| b.id.as_str().contains("fence")),
        "expected a fence at {fence_world:?}, got {:?}",
        editor.try_get_block(fence_world).map(|b| b.id),
    );

    // Additional deck: blocks are placed above the main deck (topsides + floor).
    let mid = keel.length / 2;
    let beam_hw = ship.hull.max_beam / 2;
    let mut above_deck = false;
    for dy in 1..=6 {
        for z in -beam_hw..=beam_hw {
            if solid_at(place.to_world(Point3D::new(mid, ship.deck.deck_y + dy, z))) {
                above_deck = true;
            }
        }
    }
    assert!(above_deck, "additional deck should place blocks above the main deck");

    // Deck: a top slab caps the hull's open top.
    let deck = &ship.deck;
    assert!(!deck.cells.is_empty(), "deck should have slabs");
    let deck_world = place.to_world(deck.cells[0]);
    assert!(
        editor.try_get_block(deck_world).map_or(false, |b| b.id.as_str().contains("slab")),
        "expected a deck slab at {deck_world:?}, got {:?}",
        editor.try_get_block(deck_world).map(|b| b.id),
    );

    let ascii = render_keel_ascii(keel);
    let plan = render_hull_plan(hull);
    // Oval variant plan for comparison.
    let oval = crate::generator::ships_v2::hull::build_hull_model(
        keel.length,
        keel.depth,
        context.beam_ratio,
        crate::generator::ships_v2::HullShape::Oval,
        &keel.top_profile(),
    );
    let oval_plan = render_hull_plan(&oval);
    std::fs::create_dir_all("output/ships_v2").ok();
    std::fs::write("output/ships_v2/keel.txt", &ascii).expect("write ASCII");
    std::fs::write("output/ships_v2/hull.txt", &plan).expect("write hull plan");
    std::fs::write("output/ships_v2/hull_oval.txt", &oval_plan).expect("write oval plan");

    println!(
        "Keel OK: length={}, depth={}, bow_rake={}, cells={}",
        keel.length,
        keel.depth,
        keel.bow_rake_len,
        keel.cells.len()
    );
    println!("{ascii}");
    println!(
        "Hull OK: max_beam={}, shell_cells={}",
        hull.max_beam,
        hull.cells.len()
    );
    println!("{plan}");
}

/// On land, the keel is built resting on the ground (everything above it), not
/// buried: the flat bottom sits at the ground surface.
#[tokio::test]
async fn build_ship_v2_offline_land() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships_v2::{build_ship_v2, ShipV2Context, ShipV2Ctx};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::minecraft::Block;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    // Dry-land flatworld: solid ground at y=64.
    let ground_y = 64;
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic(build_area, ground_y);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette")
        .clone();

    let mut rng = RNG::new(7);
    let context = ShipV2Context::new(Cardinal::North, 30);
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipV2Ctx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship_v2(&mut ctx, &context, anchor).await;
    editor.flush_buffer().await;

    assert!(!ship.on_water, "anchor on a land flatworld should be detected as land");

    let place = &ship.placement;
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();

    // The keel's flat bottom (local y=0) rests at the ground surface.
    let bottom_world = place.to_world(Point3D::new(ship.keel.length / 2, 0, 0));
    assert_eq!(bottom_world.y, ground_y, "land keel bottom should rest on the ground");
    assert!(
        editor.try_get_block(bottom_world).as_ref().map_or(false, |b| !is_air(b)),
        "land keel bottom should be solid at the ground surface",
    );

    println!(
        "Land keel OK: on_water={}, bottom_y={}, depth={}",
        ship.on_water, place.origin.y, ship.keel.depth,
    );
}

/// Live build: places a row of keels of increasing length into the running
/// server's build area so the shape and proportional depth can be compared in one
/// screenshot. Set a wide build area over water with `/setbuildarea`, then:
///
/// ```text
/// cargo test build_ship_v2_live -- --nocapture
/// ```
///
/// Bows point north (-z); keels are spaced along x. Each floats at its local
/// surface (motion-blocking height).
#[tokio::test]
async fn build_ship_v2_live() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships_v2::{ShipV2Context, ShipV2Ctx};
    use crate::geometry::{Cardinal, Point2D};
    use crate::http_mod::GDMCHTTPProvider;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    let provider = GDMCHTTPProvider::new();
    let build_area = provider.get_build_area().await.expect("Failed to get build area");
    let world = World::new(&provider).await.expect("Failed to create world");
    let mut editor = Editor::new(build_area, world);

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("ship_oak"))
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    // A row of randomly-sized ships (no rowboat: ~14–46), centred in the build
    // area. Ships are spaced along x; the hull beam is also along x, so spacing must
    // clear the widest possible hull (max length / beam ratio) plus a gap.
    const KEEL_COUNT: usize = 6;
    const MIN_LEN: i32 = 14;
    const MAX_LEN: i32 = 46;
    let max_beam =
        (MAX_LEN as f32 / crate::generator::ships_v2::DEFAULT_BEAM_RATIO).round() as i32;
    let spacing = max_beam + 4; // widest hull + a gap between ships

    let size = editor.world().world_rect_2d().size;
    // Centre the row in the build area: spread along x about the centre, and (since
    // bows point -z) anchor each half a length south of centre so its midpoint lands
    // on the centre z.
    let center_x = size.x / 2;
    let center_z = size.y / 2;
    let row_width = spacing * (KEEL_COUNT as i32 - 1);
    let start_x = center_x - row_width / 2;

    let mut rng = RNG::new(3);

    for i in 0..KEEL_COUNT {
        let length = rng.rand_i32_range(MIN_LEN, MAX_LEN + 1);
        let x = (start_x + spacing * i as i32).clamp(0, size.x - 1);
        let anchor_z = (center_z + length / 2).clamp(0, size.y - 1);
        let anchor = Point2D::new(x, anchor_z);
        // Alternate hull shapes so both are visible in one screenshot.
        let hull_shape = if i % 2 == 0 {
            crate::generator::ships_v2::HullShape::Teardrop
        } else {
            crate::generator::ships_v2::HullShape::Oval
        };
        let context = ShipV2Context::new(Cardinal::North, length).with_hull_shape(hull_shape);

        let mut ctx = ShipV2Ctx::new(&mut editor, &data, &palette, &mut rng);
        let ship = crate::generator::ships_v2::build_ship_v2(&mut ctx, &context, anchor).await;

        println!(
            "length={} {:?} at {anchor:?} — {} footing, bottom_y={}, depth={}, max_beam={}",
            length,
            ship.hull.shape,
            if ship.on_water { "water" } else { "land" },
            ship.placement.origin.y,
            ship.keel.depth,
            ship.hull.max_beam,
        );
    }

    editor.flush_buffer().await;
}
