//! Live-server visual test: lays out a 5×5 grid of 10×10 plazas — one row per
//! [`PlazaType`], five of each — inside the build area, then prints which row is
//! which type so each can be flown to and debugged in-world. Five copies per
//! type show the random variation (centrepiece variant, stall layout). Needs a
//! running GDMC HTTP server.
//!
//! Run with output shown:
//! `cargo test plaza_types_gallery -- --nocapture`

use crate::editor::World;
use crate::generator::buildings_v2::Culture;
use crate::geometry::Point2D;
use crate::http_mod::GDMCHTTPProvider;
use crate::noise::{Seed, RNG};

use super::plaza::{furnish_plaza_as, PlazaType};
use super::{OpenSpaceNames, Region, RegionKind, Theme};

/// Each plaza is a `PLOT`×`PLOT` square; plots are pitched `STRIDE` apart (square
/// + gap) and the grid starts `MARGIN` in from the build-area corner. The grid
/// is `COPIES` columns × one row per type.
const PLOT: i32 = 20;
const STRIDE: i32 = PLOT + 6;
const MARGIN: i32 = 6;
/// Plazas built per type (columns in the grid).
const COPIES: i32 = 5;

#[tokio::test]
async fn plaza_types_gallery() {
    let provider = GDMCHTTPProvider::new();
    let world = World::new(&provider).await.expect("connect to GDMC server");
    // All open-space coords are LOCAL (0..size); the editor adds the build-area
    // origin on write. We print origin + local so you can /tp to each plaza.
    let origin = world.origin();
    let size = world.size();
    let editor = world.get_editor();
    let theme = Theme::for_culture(Culture::Desert);
    let mut rng = RNG::new(Seed(20_260_618));

    let types = [
        PlazaType::Market,
        PlazaType::Fountain,
        PlazaType::Well,
        PlazaType::Monument,
        PlazaType::Stage,
    ];

    // One row per type, `COPIES` plots across — a 5×5 grid in local space.
    let span_x = MARGIN + COPIES * STRIDE;
    let span_z = MARGIN + types.len() as i32 * STRIDE;
    assert!(
        span_x <= size.x && span_z <= size.z,
        "build area too small ({}x{}) for a {COPIES}×{} plaza grid (need {span_x}x{span_z}) — select a bigger area",
        size.x,
        size.z,
        types.len(),
    );

    // NPC scenes harvested from every plot, staffed in one pass after the grid
    // is built so the gallery shows the stage performers, market vendors, and
    // onlookers in place.
    let mut all_scenes: Vec<crate::generator::population::AnchorScene> = Vec::new();

    println!("\n=== Plaza gallery: {COPIES} of each type ===");
    for (row, &t) in types.iter().enumerate() {
        let bz = MARGIN + row as i32 * STRIDE;
        print!("  {t:>9?}  ({COPIES}x) ->");
        for col in 0..COPIES {
            let bx = MARGIN + col * STRIDE; // local plot corner

            let mut cells: Vec<Point2D> = Vec::with_capacity((PLOT * PLOT) as usize);
            for dx in 0..PLOT {
                for dz in 0..PLOT {
                    cells.push(Point2D::new(bx + dx, bz + dz));
                }
            }
            let region = Region {
                area: cells.len(),
                cells,
                kind: RegionKind::Interior,
                large: true,
            };

            // A monument fallback means the plot's terrain left too small a flat
            // plateau for that type (e.g. a 5×5 fountain on bumpy ground); flag it
            // rather than panic, so the rest of the gallery still builds.
            let (built, scenes) = furnish_plaza_as(&editor, &region, &mut rng, &theme, t).await;
            all_scenes.extend(scenes);
            let c = region.centroid();
            let (wx, wz) = (origin.x + c.x, origin.z + c.y); // absolute centre for /tp
            if built == t {
                print!("  /tp {wx} ~ {wz}");
            } else {
                print!("  /tp {wx} ~ {wz} [!{built:?}]");
            }
        }
        println!();
    }
    println!("=== end gallery — each row above is one type; /tp to any centre ===\n");

    // Flush the blocks first so the NPCs spawn into finished plazas (entities
    // bypass the block buffer, so a stale buffer would otherwise drop them onto
    // not-yet-placed paving).
    editor.flush_buffer().await;

    // Staff the harvested anchors: vendors at stalls, a performer on each stage,
    // a scatter of onlookers in the crowd. Roster supplies names/dialogue/biome.
    use crate::generator::population::{build_roster, populate_npcs, IdAllocator, NpcData};
    let scene_count = all_scenes.len();
    match NpcData::load() {
        Ok(data) => {
            let mut id_alloc = IdAllocator::new();
            let roster =
                build_roster(scene_count, Culture::Desert, &data, &mut id_alloc, &mut rng.derive());
            let placed = populate_npcs(&editor, all_scenes, roster, scene_count, &data, &mut rng)
                .await
                .expect("populate plaza NPCs");
            println!("Spawned {placed} plaza NPCs across the gallery");
        }
        Err(e) => println!("skipped NPC staffing (npcs.yaml load failed: {e})"),
    }

    editor.flush_buffer().await;
}

/// Desert markets should be named souks and bazaars, not generic squares. Offline
/// (reads only `data/open_space_names.yaml`).
#[test]
fn desert_market_names_use_souk_bazaar() {
    let names = OpenSpaceNames::load().expect("load open_space_names.yaml");
    let mut rng = RNG::new(Seed(7));
    let mut used = std::collections::HashSet::new();
    let desert_suffixes = ["Souk", "Bazaar", "Market", "Caravanserai"];
    let mut saw_souk_or_bazaar = false;
    for _ in 0..20 {
        let name = names
            .name_plaza(PlazaType::Market, Culture::Desert, &mut rng, &mut used)
            .expect("market name");
        let last = name.rsplit(' ').next().unwrap();
        assert!(
            desert_suffixes.contains(&last),
            "desert market '{name}' should end in a souk/bazaar suffix"
        );
        saw_souk_or_bazaar |= last == "Souk" || last == "Bazaar";
    }
    assert!(saw_souk_or_bazaar, "expected at least one Souk/Bazaar across 20 rolls");
}

/// Medieval markets keep their generic names — the desert override must not leak.
#[test]
fn medieval_market_names_have_no_souk() {
    let names = OpenSpaceNames::load().expect("load open_space_names.yaml");
    let mut rng = RNG::new(Seed(7));
    let mut used = std::collections::HashSet::new();
    for _ in 0..20 {
        let name = names
            .name_plaza(PlazaType::Market, Culture::Medieval, &mut rng, &mut used)
            .expect("market name");
        assert!(
            !name.contains("Souk") && !name.contains("Bazaar"),
            "medieval market '{name}' should not use desert words"
        );
    }
}
