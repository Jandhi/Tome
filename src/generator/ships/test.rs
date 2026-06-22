//! Phase 1 ship tests. The offline build mirrors `build_furnished_houses_offline`:
//! a synthetic (here water) world + offline editor, no server. The property test
//! is the geometry regression guard, analogous to `pipeline_invariants_property_test`.

use crate::generator::ships::dimensions::{self, ShipDimensions};
use crate::generator::ships::hull::{self, check_ship_invariants};
use crate::generator::ships::{HullShape, ShipClass};

/// Build the rowboat hull model for a known seed and dump its dimensions/cells.
fn model_for(class: ShipClass, shape: HullShape, seed: i64) -> hull::HullModel {
    let mut rng = crate::noise::RNG::new(seed);
    let dims: ShipDimensions = dimensions::resolve(class, &mut rng);
    hull::build_model(shape, dims)
}

/// Every class × seed must produce a watertight, symmetric hull. Phase 1 hulls
/// all use the rowboat rib profile, but the solid→shell assembly is exercised at
/// every size envelope here.
#[test]
fn rowboat_invariants_property_test() {
    for class in [
        ShipClass::Rowboat,
        ShipClass::Sloop,
        ShipClass::Cog,
        ShipClass::Caravel,
        ShipClass::Galleon,
    ] {
        for seed in 0..40i64 {
            let shape = class.hull_shapes()[0];
            let model = model_for(class, shape, seed);

            check_ship_invariants(&model)
                .unwrap_or_else(|e| panic!("invariant failed for {class:?} seed {seed}: {e}"));

            assert!(!model.hull_cells.is_empty(), "{class:?} seed {seed}: empty hull");
            assert!(!model.deck_cells.is_empty(), "{class:?} seed {seed}: empty deck");
            assert!(
                model.hatch.is_none(),
                "{class:?} seed {seed}: Phase 1 hold must be sealed (no hatch)"
            );
        }
    }
}

/// Full offline pipeline: build a rowboat on a water flatworld, verify the hull
/// planking and deck landed and the hold is dry, and write the ASCII diagnostic.
#[tokio::test]
async fn build_rowboat_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::blueprint::render_ascii;
    use crate::generator::ships::{ShipContext, ShipCtx, build_ship, HullShape, RigPlan};
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
        .get(&PaletteId::from("medieval_spruce"))
        .expect("medieval_spruce palette")
        .clone();

    let mut rng = RNG::new(7);
    let context = ShipContext::new(
        ShipClass::Rowboat,
        HullShape::RowboatHull,
        RigPlan::Oars,
        Cardinal::North,
        64,
    );
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &context, anchor).await.expect("build_ship failed");
    editor.flush_buffer().await;

    let model = &ship.hull_model;
    let place = &ship.placement;
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();

    // A hull plank cell is solid.
    let hull_cell = model.hull_cells[0].local;
    let hull_world = place.to_world(hull_cell);
    assert!(
        editor.try_get_block(hull_world).as_ref().map_or(false, |b| !is_air(b)),
        "expected hull planking at {hull_world:?}, got {:?}",
        editor.try_get_block(hull_world),
    );

    // A deck cell is solid.
    let deck = model.deck_cells[0];
    let deck_world = place.to_world(Point3D::new(deck.x, model.deck_y, deck.y));
    assert!(
        editor.try_get_block(deck_world).as_ref().map_or(false, |b| !is_air(b)),
        "expected deck planking at {deck_world:?}, got {:?}",
        editor.try_get_block(deck_world),
    );

    // The hold is dry: an interior cell was never filled.
    assert!(!model.hold_volume.is_empty(), "rowboat should have a hold cavity");
    let hold_world = place.to_world(model.hold_volume[0]);
    assert!(
        editor.try_get_block(hold_world).map_or(true, |b| is_air(&b)),
        "hold cell at {hold_world:?} should be air, got {:?}",
        editor.try_get_block(hold_world),
    );

    let ascii = render_ascii(model, ship.rig.as_ref());
    std::fs::create_dir_all("output/ships").ok();
    std::fs::write("output/ships/rowboat.txt", &ascii).expect("write ASCII");

    println!(
        "Rowboat OK: length={}, beam={}, hull_cells={}, deck_cells={}, hold={}",
        model.dims.length,
        model.dims.beam,
        model.hull_cells.len(),
        model.deck_cells.len(),
        model.hold_volume.len(),
    );
    println!("{ascii}");
}

/// Live build: places a rowboat into the running Minecraft server's build area.
///
/// Requires a live GDMC HTTP Interface server (like most tests in this crate);
/// it fails fast if none is reachable. Set the in-game build area over water (or
/// flat ground) with `/setbuildarea`, then run:
///
/// ```text
/// cargo test build_rowboat_in_minecraft -- --nocapture --ignored
/// ```
///
/// The ship is anchored at the centre of the build area; the waterline is read
/// from the motion-blocking heightmap there, so the hull floats at the surface.
#[tokio::test]
async fn build_rowboat() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{ShipContext, ShipCtx, build_ship, HullShape, RigPlan};
    use crate::geometry::Cardinal;
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
        .get(&PaletteId::from("medieval_spruce"))
        .expect("medieval_spruce palette")
        .clone();

    // Anchor at the centre of the build area; waterline = surface there (local Y).
    let center = editor.world().world_rect_2d().midpoint();
    let waterline_y = editor.world().get_motion_blocking_height_at(center);

    let mut rng = RNG::new(7);
    let context = ShipContext::new(
        ShipClass::Rowboat,
        HullShape::RowboatHull,
        RigPlan::Oars,
        Cardinal::North,
        waterline_y,
    );

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &context, center)
        .await
        .expect("build_ship failed");
    editor.flush_buffer().await;

    println!(
        "Placed rowboat at local {center:?}, waterline_y={waterline_y}, length={}, beam={}",
        ship.dims.length, ship.dims.beam,
    );
}

// --- Phase 2: cog (curved hull + single mast + sail + rudder + laddered hold) ---

/// Every class × seed produces a watertight cog hull with an accessible hold when
/// rigged. Exercises the curved cross-section assembly and the hatch placement.
#[test]
fn cog_invariants_property_test() {
    use crate::generator::ships::fittings;
    use crate::generator::ships::rig;
    use crate::generator::ships::RigPlan;

    for class in [ShipClass::Sloop, ShipClass::Cog, ShipClass::Caravel, ShipClass::Galleon] {
        for seed in 0..40i64 {
            let mut rng = crate::noise::RNG::new(seed);
            let dims = dimensions::resolve(class, &mut rng);
            let mut model = hull::build_model(HullShape::RoundCog, dims);

            // Mirror the pipeline: rigged ships get a hatch over the hold.
            model.hatch = fittings::plan_hatch(&model, dims.length);
            assert!(
                model.hatch.is_some(),
                "{class:?} seed {seed}: a cog hold should admit a hatch"
            );

            check_ship_invariants(&model)
                .unwrap_or_else(|e| panic!("hull invariant failed for {class:?} seed {seed}: {e}"));

            let rig_model = rig::build_plan(RigPlan::SingleMast, &model, &dims);
            rig::check_rig_invariants(&model, &rig_model)
                .unwrap_or_else(|e| panic!("rig invariant failed for {class:?} seed {seed}: {e}"));
            assert_eq!(rig_model.masts.len(), 1, "{class:?} seed {seed}: expected one mast");
        }
    }
}

/// Full offline pipeline for a cog: verify hull, mast, sail, hatch trapdoor,
/// hold ladder, and rudder all landed, then write the ASCII diagnostic.
#[tokio::test]
async fn build_cog_offline() {
    use crate::editor::World;
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::blueprint::render_ascii;
    use crate::generator::ships::{ShipContext, ShipCtx, build_ship, HullShape, RigPlan};
    use crate::geometry::{Cardinal, Point2D, Point3D, Rect3D};
    use crate::minecraft::Block;
    use crate::noise::RNG;
    use crate::util::init_logger;

    init_logger();

    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(255, 127, 255));
    let world = World::synthetic_water(build_area, 50, 64);
    let mut editor = world.get_offline_editor();

    let data = LoadedData::load().expect("Failed to load data");
    let palette = data
        .palettes
        .get(&PaletteId::from("medieval_spruce"))
        .expect("medieval_spruce palette")
        .clone();

    let mut rng = RNG::new(11);
    let context = ShipContext::new(
        ShipClass::Cog,
        HullShape::RoundCog,
        RigPlan::SingleMast,
        Cardinal::North,
        64,
    );
    let anchor = Point2D::new(96, 96);

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &context, anchor).await.expect("build_ship failed");
    editor.flush_buffer().await;

    let model = &ship.hull_model;
    let place = &ship.placement;
    let rig = ship.rig.as_ref().expect("cog should be rigged");
    let id_of = |p: Point3D| editor.try_get_block(p).map(|b| b.id);
    let is_air = |b: &Block| b.id == "air".into() || b.id == "minecraft:air".into();

    // Curvature: the hull should use stair bevels (rounded bilge), not be all blocks.
    let stairs = model
        .hull_cells
        .iter()
        .filter(|p| matches!(p.form, crate::minecraft::BlockForm::Stairs))
        .count();
    assert!(stairs > 0, "cog hull should have stair bevels for curvature");

    // Mast: a solid block one above the deck at the mast base.
    let mast = &rig.masts[0];
    let mast_world = place.to_world(Point3D::new(mast.base.x, mast.base.y + 1, mast.base.z));
    assert!(
        editor.try_get_block(mast_world).as_ref().map_or(false, |b| !is_air(b)),
        "expected mast at {mast_world:?}, got {:?}", editor.try_get_block(mast_world),
    );

    // Sail: white wool somewhere in the sail surface.
    assert!(!rig.sail_cells.is_empty(), "cog should have a sail");
    let sail_world = place.to_world(rig.sail_cells[rig.sail_cells.len() / 2]);
    assert_eq!(
        id_of(sail_world),
        Some("minecraft:white_wool".into()),
        "expected sail wool at {sail_world:?}",
    );

    // Hatch: a trapdoor where the deck hatch was cut.
    let hatch = model.hatch.expect("cog has a hatch");
    let hatch_world = place.to_world(Point3D::new(hatch.x, model.deck_y, hatch.y));
    assert!(
        id_of(hatch_world).map_or(false, |id| id.as_str().contains("trapdoor")),
        "expected trapdoor at hatch {hatch_world:?}, got {:?}", id_of(hatch_world),
    );

    // Ladder: a rung just below the hatch.
    let ladder_world = place.to_world(Point3D::new(hatch.x, model.deck_y - 1, hatch.y));
    assert_eq!(
        id_of(ladder_world),
        Some("minecraft:ladder".into()),
        "expected ladder below hatch at {ladder_world:?}",
    );

    // Rudder: a solid blade aft of the sternpost.
    let rudder_world = place.to_world(Point3D::new(-1, model.waterline_y, 0));
    assert!(
        editor.try_get_block(rudder_world).as_ref().map_or(false, |b| !is_air(b)),
        "expected rudder at {rudder_world:?}, got {:?}", editor.try_get_block(rudder_world),
    );

    let ascii = render_ascii(model, ship.rig.as_ref());
    std::fs::create_dir_all("output/ships").ok();
    std::fs::write("output/ships/cog.txt", &ascii).expect("write ASCII");

    println!(
        "Cog OK: length={}, beam={}, hull={}, hold={}, sail={}, hatch={:?}",
        model.dims.length, model.dims.beam,
        model.hull_cells.len(), model.hold_volume.len(), rig.sail_cells.len(), hatch,
    );
    println!("{ascii}");
}

/// Live build: places a cog into the running server's build area, offset from the
/// centre so it doesn't overlap [`build_rowboat_in_minecraft`] when both run.
#[tokio::test]
async fn build_cog() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::materials::PaletteId;
    use crate::generator::ships::{ShipContext, ShipCtx, build_ship, HullShape, RigPlan};
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
        .get(&PaletteId::from("medieval_spruce"))
        .expect("medieval_spruce palette")
        .clone();

    let mid = editor.world().world_rect_2d().midpoint();
    let size = editor.world().world_rect_2d().size;
    // Offset along the beam axis, clamped inside the build area.
    let center = Point2D::new(mid.x, (mid.y + 40).min(size.y - 1));
    let waterline_y = editor.world().get_motion_blocking_height_at(center);

    let mut rng = RNG::new(11);
    let context = ShipContext::new(
        ShipClass::Cog,
        HullShape::RoundCog,
        RigPlan::SingleMast,
        Cardinal::North,
        waterline_y,
    );

    let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
    let ship = build_ship(&mut ctx, &context, center).await.expect("build_ship failed");
    editor.flush_buffer().await;

    println!(
        "Placed cog at local {center:?}, waterline_y={waterline_y}, length={}, beam={}",
        ship.dims.length, ship.dims.beam,
    );
}

// --- Phase 3: variety (all hull shapes, multi-mast rigs, palettes) ---

/// Every class × each of its valid hull shapes × seeds must produce a watertight,
/// symmetric hull, and every valid rig must pass its own invariants. Exercises
/// the caravel/longship profiles and the multi-mast layouts.
#[test]
fn variety_invariants_property_test() {
    use crate::generator::ships::fittings;
    use crate::generator::ships::rig;
    use crate::generator::ships::{RigPlan, SHIP_CLASSES};

    for class in SHIP_CLASSES {
        for hull_shape in class.hull_shapes() {
            for rig_plan in class.rig_plans() {
                for seed in 0..30i64 {
                    let mut rng = crate::noise::RNG::new(seed);
                    let dims = dimensions::resolve(class, &mut rng);
                    let mut model = hull::build_model(hull_shape, dims);

                    let rigged = !matches!(rig_plan, RigPlan::Oars);
                    if rigged {
                        model.hatch = fittings::plan_hatch(&model, dims.length);
                        assert!(
                            model.hatch.is_some(),
                            "{class:?}/{hull_shape:?} seed {seed}: rigged hull needs a hatch"
                        );
                    }

                    check_ship_invariants(&model).unwrap_or_else(|e| {
                        panic!("hull invariant {class:?}/{hull_shape:?} seed {seed}: {e}")
                    });

                    let rig_model = rig::build_plan(rig_plan, &model, &dims);
                    rig::check_rig_invariants(&model, &rig_model).unwrap_or_else(|e| {
                        panic!("rig invariant {class:?}/{rig_plan:?} seed {seed}: {e}")
                    });

                    let expected_masts = match rig_plan {
                        RigPlan::Oars => 0,
                        RigPlan::SingleMast => 1,
                        RigPlan::TwoMast => 2,
                        RigPlan::ThreeMast => 3,
                    };
                    assert_eq!(
                        rig_model.masts.len(), expected_masts,
                        "{class:?}/{rig_plan:?} seed {seed}: mast count"
                    );
                }
            }
        }
    }
}

/// Live build: places one ship of every class with the ship palette, lined up
/// side by side across the running server's build area so they can be compared in
/// a single screenshot. Each floats at its local surface (motion-blocking height).
/// Set a wide build area over water; the bows point north (-z), so ships extend
/// toward the north edge and are spaced along x.
#[tokio::test]
async fn build_fleet() {
    use crate::editor::{Editor, World};
    use crate::generator::data::LoadedData;
    use crate::generator::ships::{default_ship_palette, ShipContext, ShipCtx, build_ship, SHIP_CLASSES};
    use crate::geometry::Point2D;
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
        .get(&default_ship_palette())
        .expect("ship_oak palette (data/palettes/ships/)")
        .clone();

    // Spread the five ships along x; anchor near the +z (south) edge so each
    // hull's length has room to extend toward -z (north).
    let size = editor.world().world_rect_2d().size;
    let step = (size.x / (SHIP_CLASSES.len() as i32 + 1)).max(16);
    let anchor_z = (size.y - 3).max(0);

    let mut rng = RNG::new(3);

    for (i, class) in SHIP_CLASSES.iter().enumerate() {
        let (hull_shape, rig_plan) = class.pick_combo(&mut rng);
        let anchor = Point2D::new(step * (i as i32 + 1), anchor_z);
        let waterline_y = editor.world().get_motion_blocking_height_at(anchor);
        let context = ShipContext::new(*class, hull_shape, rig_plan, crate::geometry::Cardinal::North, waterline_y);

        let mut ctx = ShipCtx::new(&mut editor, &data, &palette, &mut rng);
        let ship = build_ship(&mut ctx, &context, anchor)
            .await
            .unwrap_or_else(|e| panic!("{class:?} build failed: {e}"));

        println!(
            "{class:?}: {hull_shape:?} + {rig_plan:?} at {anchor:?} (waterline {waterline_y}) — length={}, beam={}, masts={}",
            ship.dims.length,
            ship.dims.beam,
            ship.rig.as_ref().map_or(0, |r| r.masts.len()),
        );
    }

    editor.flush_buffer().await;
}

