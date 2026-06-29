use std::collections::{HashMap, HashSet};

use crate::data::Loadable;
use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::districts::{build_wall, generate_parcels, ParcelType, TowerSkin, WallType};
use crate::generator::buildings_v2::style::local_wood_palette;
use crate::generator::materials::{Material, MaterialId, MaterialRole, Placer};
use crate::generator::nbts::Structure;
use crate::generator::paths::{build_paths_merged, build_road_network, build_rural_road_network, find_blocks, Path, PathPriority, RuralBuilding};
use crate::generator::placement::{resolve_rural_production, try_place_rural, PlacedRural};
use crate::generator::resource_chain::paint_production_area_for;
use crate::generator::terrain::{drain_liquids, flatten_urban_area, force_height, log_trees};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::{Block, Color};
use crate::noise::{Seed, RNG};

/// Full town-generation pipeline: feathered urban flatten + tiered A* road
/// network, then hierarchical house placement.
///
/// parcels -> wall+gates -> flatten -> industrial buildings -> arterials(MST) +
/// collectors(gates) -> blocks/subdivision -> roads -> houses -> verge + lights.
///
/// The caller is responsible for constructing the `Editor` (and the `World`
/// behind it) and for flushing/finalising afterwards beyond the final
/// `flush_buffer` performed here.
/// Residents per bed of sleeping capacity. A house's population budget is
/// `max(1, round(beds * POPULATION_PER_BED))`, so a single-bed house houses ~2
/// and a double bed (which sleeps two) ~3 — enough to read as lived-in.
const POPULATION_PER_BED: f32 = 1.5;

/// The settlement's colour identity. Picked once per town from the culture's
/// curated [`Culture::color_pool`], it gives the town two recurring colours plus
/// a unique family colour per manor, so a street reads as a coherent palette
/// with two dominant hues and occasional variety rather than 16 random dyes.
struct ColorScheme {
    /// The two recurring town colours: `town[0]` is dominant, `town[1]` second.
    town: [Color; 2],
    /// One family colour per manor (`MANOR_CAP` entries), handed out in
    /// placement order. Drawn freely from the pool — they may coincide with a
    /// town colour, but never with each other.
    manor: Vec<Color>,
    /// The full culture pool, sampled for the occasional off-scheme accent.
    pool: Vec<Color>,
}

impl ColorScheme {
    /// Pick two distinct town colours and `manor_count` distinct manor colours
    /// from the culture pool. The pool always has ≥ 4 entries (see
    /// `Culture::color_pool`), so the distinct draws never run dry.
    fn new(culture: crate::generator::buildings_v2::Culture, manor_count: usize, rng: &mut RNG) -> Self {
        let pool = culture.color_pool();
        let mut town_pick = pool.clone();
        rng.shuffle(&mut town_pick);
        let town = [town_pick[0], town_pick[1 % town_pick.len()]];
        let mut manor_pick = pool.clone();
        rng.shuffle(&mut manor_pick);
        let manor: Vec<Color> = manor_pick.into_iter().take(manor_count.max(1)).collect();
        Self { town, manor, pool }
    }

    /// A colour for one ordinary building: 50% the dominant town colour, 25% the
    /// second town colour, 25% a random pool colour for variety.
    fn next_color(&self, rng: &mut RNG) -> Color {
        match rng.rand_i32_range(0, 100) {
            0..=49 => self.town[0],
            50..=74 => self.town[1],
            _ => *rng.choose(&self.pool),
        }
    }

    /// A pool colour guaranteed different from `avoid` — the secondary (charge)
    /// colour for a manor's banner design, so the charge always contrasts with
    /// its family (field) colour. The pool always has ≥ 4 entries, so excluding
    /// one never empties it.
    fn distinct_from(&self, avoid: Color, rng: &mut RNG) -> Color {
        let opts: Vec<Color> = self.pool.iter().copied().filter(|&c| c != avoid).collect();
        if opts.is_empty() { avoid } else { *rng.choose(&opts) }
    }
}

/// A manor family harvested during generation for the chronicle. Surname is only
/// known after the population pass, so these are gathered when each manor sign is
/// lettered (where surname + the house's `HouseAnchors` meet) and converted to a
/// [`Landmark`] in [`assemble_dossier`].
struct ManorFact {
    surname: String,
    designation: String,
    pos: Point2D,
    color: Option<Color>,
    blazon: Option<String>,
}

/// Culture → its lowercase word for the dossier/prose.
fn culture_word(culture: crate::generator::buildings_v2::Culture) -> String {
    use crate::generator::buildings_v2::Culture;
    match culture {
        Culture::Medieval => "medieval",
        Culture::Desert => "desert",
        Culture::Japanese => "japanese",
    }
    .to_string()
}

/// A colour as an English word for prose ("light_blue" → "light blue").
fn color_word(c: Color) -> String {
    let s: String = c.into();
    s.replace('_', " ")
}

/// Average of a set of cells (rounded). Empty → origin.
fn cells_centroid(cells: &[Point2D]) -> Point2D {
    if cells.is_empty() {
        return Point2D::new(0, 0);
    }
    let (sx, sz) = cells.iter().fold((0i64, 0i64), |(x, z), p| (x + p.x as i64, z + p.y as i64));
    let n = cells.len() as i64;
    Point2D::new((sx / n) as i32, (sz / n) as i32)
}

/// Coarse adjectival quarter of `p` relative to the town centre: "central",
/// "eastern", "on the northern edge". Deterministic, so it can't contradict the
/// built town. `radius` is the town's outer reach (max cell distance from centre).
fn quarter_of(p: Point2D, centre: Point2D, radius: f32) -> String {
    let dx = (p.x - centre.x) as f32;
    let dz = (p.y - centre.y) as f32; // Point2D.y is the Z (north/south) axis.
    let dist = (dx * dx + dz * dz).sqrt();
    if radius <= 0.0 || dist < radius * 0.30 {
        return "central".to_string();
    }
    // North is -Z, south +Z, east +X, west -X.
    let dir = if dx.abs() >= dz.abs() {
        if dx >= 0.0 { "eastern" } else { "western" }
    } else if dz >= 0.0 {
        "southern"
    } else {
        "northern"
    };
    if dist > radius * 0.75 {
        format!("on the {dir} edge")
    } else {
        dir.to_string()
    }
}

/// Cardinal word of `p` relative to centre — for gates, which always sit on the
/// perimeter, so "central" never applies.
fn compass_word(p: Point2D, centre: Point2D) -> &'static str {
    let dx = p.x - centre.x;
    let dz = p.y - centre.y;
    if dx.abs() >= dz.abs() {
        if dx >= 0 { "east" } else { "west" }
    } else if dz >= 0 {
        "south"
    } else {
        "north"
    }
}

/// The name of the labelled road nearest `p`, or `None` if the closest road is
/// further than `max_d` cells (don't claim "by X" for something across town).
fn nearest_road(
    p: Point2D,
    road_labels: &HashMap<Point2D, u32>,
    road_names: &HashMap<u32, String>,
    max_d: i32,
) -> Option<String> {
    let mut best: Option<(i32, u32)> = None;
    for (&cell, &rid) in road_labels {
        let dx = cell.x - p.x;
        let dz = cell.y - p.y;
        let d2 = dx * dx + dz * dz;
        if best.map_or(true, |(bd, _)| d2 < bd) {
            best = Some((d2, rid));
        }
    }
    let (d2, rid) = best?;
    if d2 > max_d * max_d {
        return None;
    }
    road_names.get(&rid).cloned()
}

/// Biomes across the town's parcels, most common first.
fn biome_counts(world: &crate::editor::World) -> Vec<(crate::minecraft::Biome, u32)> {
    let mut by_count: Vec<(crate::minecraft::Biome, u32)> = world
        .parcel_analysis_data
        .iter()
        .flat_map(|(_, data)| data.biome_count())
        .fold(HashMap::new(), |mut acc, (biome, count)| {
            *acc.entry(biome.clone()).or_insert(0u32) += *count;
            acc
        })
        .into_iter()
        .collect();
    by_count.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    by_count
}

/// Top-`n` biomes, prettified ("snowy_taiga" → "snowy taiga").
fn top_biomes(world: &crate::editor::World, n: usize) -> Vec<String> {
    biome_counts(world).into_iter().take(n).map(|(b, _)| b.name().replace('_', " ")).collect()
}

/// The single most common biome across the town's parcels — used by the legacy
/// `place_buildings` pipeline to pick a paving type. `None` if no parcels were
/// analysed.
pub fn dominant_biome(world: &crate::editor::World) -> Option<crate::minecraft::Biome> {
    biome_counts(world).into_iter().next().map(|(b, _)| b)
}

/// Assemble the chronicle's [`CityDossier`] from the final settlement state. All
/// inputs are locals still in scope at the end of [`generate_town`]; this does the
/// id→name / colour→word / coords→quarter conversion so the LLM sees only
/// human-readable, relational facts. Roads are emitted first because they are the
/// `near` vocabulary every other landmark anchors to.
fn assemble_dossier(
    editor: &Editor,
    urban: &HashSet<Point2D>,
    culture: crate::generator::buildings_v2::Culture,
    named: &crate::generator::naming::SettlementName,
    town_colors: &[Color],
    civic_blazon: &str,
    road_names: &HashMap<u32, String>,
    road_labels: &HashMap<Point2D, u32>,
    place_labels: &[(Point2D, String)],
    manor_facts: &[ManorFact],
    house_count: usize,
    population: usize,
    harvests: &[String],
    produces: &[String],
    rng: &mut RNG,
) -> crate::generator::chronicle::CityDossier {
    use crate::generator::chronicle::{size_word, CityDossier, DossierDistrict, Landmark};
    use crate::generator::BuildClaim;

    let centre = cells_centroid(&urban.iter().copied().collect::<Vec<_>>());
    let radius = urban
        .iter()
        .map(|&c| {
            let dx = (c.x - centre.x) as f32;
            let dz = (c.y - centre.y) as f32;
            (dx * dx + dz * dz).sqrt()
        })
        .fold(0.0_f32, f32::max);

    // ── Name the urban districts (the unit the chronicle is organised around) ──
    // Each landmark is later stamped with the district it sits in, and the guide
    // walks the districts one at a time. Manor/park positions feed family- and
    // green-themed district names; trades are scanned from the world per district.
    let manor_pts: Vec<(Point2D, String)> =
        manor_facts.iter().map(|mf| (mf.pos, mf.surname.clone())).collect();
    let park_pts: Vec<Point2D> = place_labels.iter().map(|(p, _)| *p).collect();
    let district_names = crate::generator::districts::name_districts(
        editor.world(), culture, centre, &manor_pts, &park_pts, rng,
    );
    // Name of the named district containing `p`, or "" if it has no urban district.
    let district_for = |p: Point2D| -> String {
        editor
            .world()
            .get_district_at(p)
            .and_then(|id| district_names.get(&id))
            .map(|d| d.name.clone())
            .unwrap_or_default()
    };
    // World `(x, y, z)` to stand a player at `p` (local) — the chronicle turns each
    // landmark into a `/tp` link. `get_height_at` is local (relative to origin.y) and
    // points at the first air cell above the surface, i.e. where a player stands.
    let origin = editor.world().origin();
    let tp_for = |p: Point2D| -> Option<(i32, i32, i32)> {
        editor
            .world()
            .get_height_at(p)
            .map(|h| (p.x + origin.x, origin.y + h, p.y + origin.z))
    };

    let mut landmarks: Vec<Landmark> = Vec::new();

    // ── Roads first (the `near` vocabulary) ──
    let mut road_cells: HashMap<u32, Vec<Point2D>> = HashMap::new();
    for (&c, &rid) in road_labels {
        road_cells.entry(rid).or_default().push(c);
    }
    let mut rids: Vec<u32> = road_names.keys().copied().collect();
    rids.sort_unstable();
    for rid in rids {
        let Some(name) = road_names.get(&rid) else { continue };
        let cen = road_cells.get(&rid).map(|cs| cells_centroid(cs));
        let quarter = cen.map(|c| quarter_of(c, centre, radius)).unwrap_or_default();
        let district = cen.map(&district_for).unwrap_or_default();
        landmarks.push(Landmark { kind: "road".into(), name: name.clone(), quarter, near: vec![], notes: vec![], district, tp: cen.and_then(&tp_for) });
    }

    // ── Industries: distinct workplace structure types, by location ──
    let mut scan: Vec<Point2D> = editor.world().get_urban_points().into_iter().collect();
    for sd in editor.world().districts.values() {
        if sd.data.parcel_type == ParcelType::Rural {
            scan.extend(sd.data.points_2d.iter().copied());
        }
    }
    let mut by_kind: HashMap<String, Vec<Point2D>> = HashMap::new();
    for p in scan {
        if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
            by_kind.entry(id.structure_type.0.clone()).or_default().push(p);
        }
    }
    let mut kinds: Vec<String> = by_kind.keys().cloned().collect();
    kinds.sort();
    for kind in kinds {
        let cen = cells_centroid(&by_kind[&kind]);
        let quarter = quarter_of(cen, centre, radius);
        let near = nearest_road(cen, road_labels, road_names, 6).into_iter().collect();
        landmarks.push(Landmark {
            kind: "industry".into(),
            name: format!("the {}", kind.replace('_', " ")),
            quarter,
            near,
            notes: vec![],
            district: district_for(cen),
            tp: tp_for(cen),
        });
    }

    // ── Families (manors) ──
    for mf in manor_facts {
        let quarter = quarter_of(mf.pos, centre, radius);
        let near = nearest_road(mf.pos, road_labels, road_names, 8).into_iter().collect();
        let mut notes = Vec::new();
        if let Some(c) = mf.color {
            notes.push(color_word(c));
        }
        if let Some(b) = &mf.blazon {
            notes.push(b.clone());
        }
        landmarks.push(Landmark {
            kind: "manor".into(),
            name: format!("the {} {}", mf.surname, mf.designation),
            quarter,
            near,
            notes,
            district: district_for(mf.pos),
            tp: tp_for(mf.pos),
        });
    }

    // ── Greens & squares (named open spaces) ──
    for (pos, name) in place_labels {
        let quarter = quarter_of(*pos, centre, radius);
        let near = nearest_road(*pos, road_labels, road_names, 6).into_iter().collect();
        landmarks.push(Landmark { kind: "park".into(), name: name.clone(), quarter, near, notes: vec![], district: district_for(*pos), tp: tp_for(*pos) });
    }

    // ── Gates (deduped by side) ──
    let mut gate_names: HashSet<String> = HashSet::new();
    for (gpos, _dir) in &editor.world().gate_locations {
        let dir = compass_word(gpos.drop_y(), centre);
        let name = format!("the {dir} gate");
        if gate_names.insert(name.clone()) {
            landmarks.push(Landmark {
                kind: "gate".into(),
                name,
                quarter: format!("on the {dir} edge"),
                near: vec![],
                notes: vec![],
                // Gates sit on the wall, outside any urban district — the guide
                // collects them under its trailing "around the edge" section.
                district: String::new(),
                tp: tp_for(gpos.drop_y()),
            });
        }
    }

    // Town colours as words, deduped.
    let mut town_colours: Vec<String> = Vec::new();
    for &c in town_colors {
        let w = color_word(c);
        if !town_colours.contains(&w) {
            town_colours.push(w);
        }
    }

    // The named districts with their quarter, ordered by quarter then name so the
    // guide walks them in a stable, geographically-coherent sequence.
    let mut districts: Vec<DossierDistrict> = district_names
        .iter()
        .map(|(id, dn)| {
            let quarter = editor
                .world()
                .districts
                .get(id)
                .map(|d| quarter_of(cells_centroid(&d.data.points_2d.iter().copied().collect::<Vec<_>>()), centre, radius))
                .unwrap_or_default();
            DossierDistrict { name: dn.name.clone(), quarter }
        })
        .collect();
    districts.sort_by(|a, b| a.quarter.cmp(&b.quarter).then_with(|| a.name.cmp(&b.name)));

    CityDossier {
        name: named.name.clone(),
        subtitle: named.subtitle.clone(),
        culture: culture_word(culture),
        town_colours,
        civic_blazon: civic_blazon.to_string(),
        biomes: top_biomes(editor.world(), 3),
        size: size_word(house_count),
        walled: !editor.world().gate_locations.is_empty(),
        population,
        harvests: harvests.to_vec(),
        produces: produces.to_vec(),
        districts,
        landmarks,
    }
}

/// A bare dossier for callers that don't run the full town pipeline (the legacy
/// `place_buildings` path / visualizer): a procedurally-named town with whatever
/// industries and gates exist in the world, but no roads/families/greens. Gives
/// the chronicle a real name without the old AI namer.
pub fn minimal_dossier(
    editor: &Editor,
    culture: crate::generator::buildings_v2::Culture,
    rng: &mut RNG,
) -> crate::generator::chronicle::CityDossier {
    let urban = editor.world().get_urban_points();
    let named = crate::generator::naming::generate_settlement_name(
        editor.world(), &urban, &[], culture, &[], rng,
    );
    let no_roads: HashMap<u32, String> = HashMap::new();
    let no_labels: HashMap<Point2D, u32> = HashMap::new();
    assemble_dossier(
        editor, &urban, culture, &named, &[], "", &no_roads, &no_labels, &[], &[],
        editor.world().buildings.len(), 0, &[], &[], rng,
    )
}

/// The `PrimaryWood` material of the most common biome wood across the build
/// area — the timber a palisade should be built from so it matches what grows
/// locally (spruce in taiga, acacia in savanna, …). Tallies each cell's local
/// wood palette, keeping only palettes that actually have a file and a wood role,
/// and returns the modal one's primary wood. `None` when no biome in the area has
/// a usable wood palette (e.g. all desert/badlands), so the caller can fall back.
fn dominant_local_wood(biome_map: &[Vec<crate::minecraft::Biome>], data: &LoadedData) -> Option<MaterialId> {
    let mut counts: HashMap<MaterialId, usize> = HashMap::new();
    for column in biome_map {
        for biome in column {
            let Some(palette) = local_wood_palette(biome.clone())
                .and_then(|id| data.palettes.get(&id)) else { continue; };
            let Some(wood) = palette.get_material(MaterialRole::PrimaryWood) else { continue; };
            *counts.entry(wood.clone()).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(id, _)| id)
}

pub async fn generate_town(
    editor: &mut Editor,
    seed: Seed,
    culture: Option<crate::generator::buildings_v2::Culture>,
) {
    let mut rng = RNG::new(seed);
    let mut rng2 = RNG::new(seed);

    // Terraforming and block edits go through the HTTP interface with block updates
    // on, which can spawn block-drop item entities (e.g. when a placed block replaces
    // grass/flowers) that pile up and lag the world. Disable blockdrops for the run.
    if let Err(e) = editor.set_gamerule("blockdrops", "false").await {
        log::warn!("Failed to disable blockdrops gamerule: {e}");
    }

    // Settlement culture: an explicit override (tests) wins; otherwise auto-select
    // from the build area's climate so the town fits its biome while the cultures
    // stay roughly even across worlds (see `buildings_v2::climate`). The selection
    // RNG is keyed off the seed independently so it never perturbs `rng`/`rng2`.
    let culture = culture.unwrap_or_else(|| {
        let mut culture_rng = RNG::from_seed_and_string(seed, "culture_select");
        let c = crate::generator::buildings_v2::climate::select_culture(
            editor.world().get_ground_biome_map(),
            &mut culture_rng,
        );
        println!("Auto-selected culture {c:?} from build-area climate");
        c
    });

    // Infrastructure materials follow the culture: a desert town gets sandstone
    // roads and walls, a Japanese town a blackstone wall (matching its palette-
    // skinned towers), everyone else the default stone/cobble.
    let desert = matches!(culture, crate::generator::buildings_v2::Culture::Desert);
    let japanese = matches!(culture, crate::generator::buildings_v2::Culture::Japanese);
    let (wall_mat, arterial_mat, collector_mat): (&str, &str, &str) = if desert {
        ("smooth_sandstone", "smooth_sandstone", "sandstone")
    } else if japanese {
        // Blackstone wall to match the towers (which take the culture palette's
        // `polished_blackstone_bricks` stone); deepslate roads — refined brick
        // arterials, cobbled-deepslate collectors — to sit under the dark town.
        ("polished_blackstone_bricks", "deepslate_bricks", "cobbled_deepslate")
    } else {
        ("stone_bricks", "stone_bricks", "cobblestone")
    };

    generate_parcels(seed, editor).await;

    
    let data = LoadedData::load().expect("Failed to load data");

    // ── Resource chain over rural districts ──────────────────────────────
    let rural_analysis: HashMap<_, _> = editor.world().district_analysis_data.iter()
        .filter(|(id, _)| {
            editor.world().districts.get(id)
                .map(|d| d.data.parcel_type == ParcelType::Rural)
                .unwrap_or(false)
        })
        .map(|(id, analysis)| (*id, analysis.clone()))
        .collect();
    // Resolve the rural economy with placement feasibility folded in: parcels that
    // can't physically seat a resource's gather building (footprint too big for any
    // flat enough pad) are excluded during assignment, so the plan never promises a
    // building placement would later drop. (Rural terrain is still natural here —
    // flatten/walls only touch urban.)
    let result = resolve_rural_production(&data, editor, &rural_analysis, &mut rng);

    // Phase 1 — feathered urban flatten.
    let urban = editor.world().get_urban_points();
    // Log (clear) the urban area of trees so roads, buildings, and houses
    // aren't dropped into standing forest.
    log_trees(&*editor, urban.clone()).await;
    println!("Logged {} urban cells of trees", urban.len());
    // Clear all standing water/lava from the city bounds BEFORE terraforming, so
    // the flatten only grades solid ground (and `is_water` no longer makes it
    // skip those cells).
    drain_liquids(editor, &urban).await;
    println!("Drained liquids from {} urban cells", urban.len());
    flatten_urban_area(editor, &urban, 16, 12, true).await;

    // Wall + gates — gates populate world.gate_locations, used by the network.
    let materials = Material::load().expect("Failed to load materials");
    let wall_material = MaterialId::new(wall_mat.to_string());
    let mut placer: Placer = Placer::new(&materials, &mut rng);
    let structures = Structure::load().expect("Failed to load structures");
    let data = LoadedData::load().expect("Failed to load data");
    // Re-skin wall towers into the culture palette so the placed tower NBT
    // matches the rest of the settlement. The tower's oak cap maps to the roof
    // role, so each culture's tower roof follows its building roofs. Desert is
    // the exception: merge a dark-prismarine roof override so desert tower roofs
    // pop against the sandstone body instead of being sandstone-on-sandstone.
    let tower_palette = data.palettes.get(&culture.palette_id()).cloned().map(|p| {
        if desert {
            let roof = data.palettes.get(&"prismarine_roof".into())
                .expect("prismarine_roof palette not found");
            p.merged_with(roof)
        } else {
            p
        }
    });
    let tower_skin = tower_palette.as_ref().map(|p| TowerSkin { data: &data, palette: p });
    // City size = number of urban super-parcels. Small hamlets (≤3) get a cheap
    // palisade; larger towns (4+) get the full standard-with-inner stone wall.
    let n_urban = editor.world().districts.values()
        .filter(|sd| sd.data.parcel_type == crate::generator::districts::ParcelType::Urban)
        .count();
    let wall_type = if n_urban <= 3 {
        WallType::Palisade
    } else {
        WallType::StandardWithInner
    };
    // A palisade is a timber stockade (logs + fences), so it uses the local wood
    // rather than the stone `wall_material` the standard wall takes: pick the most
    // common biome wood across the build area, so a taiga town gets spruce, a
    // savanna acacia, etc. Falls back to the stone wall material if no biome in the
    // area has a wood palette (desert/badlands) or the palette lacks a wood role.
    let palisade_material = (wall_type == WallType::Palisade)
        .then(|| dominant_local_wood(editor.world().get_ground_biome_map(), &data))
        .flatten();
    let wall_material = palisade_material.as_ref().unwrap_or(&wall_material);
    println!("City size {n_urban} urban super-parcels -> {wall_type:?} wall ({})", wall_material.as_str());
    build_wall(
        &editor.world().get_urban_points(), editor, &mut rng2,
        &mut placer, wall_material, &structures, wall_type, tower_skin.as_ref(),
    ).await;
    drop(placer);

    // DEBUG: how many gates did we actually get?
    {
        let n_total = editor.world().districts.len();
        println!("URBAN super-parcels: {}/{} total | gates: {}", n_urban, n_total, editor.world().gate_locations.len());
    }

    let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);
        // Dropped-by-competition-cap parcels, ordered flattest-first per resource:
        // promoted when a primary fails to seat, so a terrain miss costs us a different
        // parcel rather than the building (and the planned economy) entirely.
        let mut fallbacks: HashMap<String, std::collections::VecDeque<_>> = result
            .fallback_assignments
            .iter()
            .map(|(res, list)| (res.clone(), list.iter().cloned().collect()))
            .collect();
        let mut placed = 0usize;
        // Placed rural buildings, collected so the road network can connect them
        // and the production painters can run *after* the roads (R3 below).
        let mut placed_rural: Vec<PlacedRural> = Vec::new();
        for sd_id in &sd_ids {
            let assignment = result.parcel_assignments[sd_id].clone();
            if let Some(p) = try_place_rural(*sd_id, &assignment, &data, editor, &mut rng).await {
                placed += 1;
                placed_rural.push(p);
                continue;
            }
            // Primary couldn't seat — promote the best dropped same-resource parcel(s)
            // until one places, keeping the per-resource count at its cap.
            while let Some((fb_id, fb_assignment)) = fallbacks
                .get_mut(&assignment.primary_resource)
                .and_then(|q| q.pop_front())
            {
                log::info!(
                    "[resource-chain]   promoting fallback {:?} for resource {} after {:?} failed to place",
                    fb_id, assignment.primary_resource, sd_id,
                );
                if let Some(p) = try_place_rural(fb_id, &fb_assignment, &data, editor, &mut rng).await {
                    placed += 1;
                    placed_rural.push(p);
                    break;
                }
            }
        }
        log::info!("Placed {} of {} rural buildings", placed, sd_ids.len());

    // ── Rural road network (built BEFORE the production painters) ─────────
    // Connect every placed rural building to a town gate, predicting and reusing
    // the `rural_road` border ring each painter will lay. Realise + claim the
    // roads here so the painters' border rings skip the cells the road owns.
    let rural_material = MaterialId::new("rural_road".to_string());
    let rural_buildings: Vec<RuralBuilding> = placed_rural.iter().map(|p| RuralBuilding {
        district: p.district,
        structure: p.structure.clone(),
        has_border_ring: p.has_border_ring,
    }).collect();
    let rural_paths = build_rural_road_network(&*editor, &rural_buildings, rural_material, 1).await;
    if !rural_paths.is_empty() {
        // Flatten the routed corridor to the road heights (skipping building /
        // wall cells so a placed structure isn't re-graded), then meld the
        // surface — mirrors the urban road realization.
        let mut corridor: HashMap<Point2D, i32> = HashMap::new();
        for path in &rural_paths {
            let w = path.width() as i32;
            for pt in path.points() {
                let base = pt.drop_y();
                for dx in -w..=w {
                    for dz in -w..=w {
                        let c = Point2D::new(base.x + dx, base.y + dz);
                        corridor.entry(c).and_modify(|y| *y = (*y).min(pt.y)).or_insert(pt.y);
                    }
                }
            }
        }
        let corridor_pts: HashSet<Point3D> = corridor.iter()
            .filter(|(c, _)| !matches!(
                editor.world().get_claim(**c),
                Some(crate::generator::BuildClaim::Structure(_)
                    | crate::generator::BuildClaim::Building(_)
                    | crate::generator::BuildClaim::Wall)
            ))
            .map(|(c, &y)| Point3D::new(c.x, y, c.y))
            .collect();
        force_height(editor, &corridor_pts, false).await;
        build_paths_merged(&*editor, &data, &rural_paths, &mut rng).await;
        for path in &rural_paths {
            let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
            let mut paved = crate::geometry::get_surrounding_set(&centre, path.width().saturating_sub(1));
            paved.extend(centre);
            for c in paved {
                editor.world_mut().claim(c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Road));
            }
        }
    }
    println!("Rural roads: {} segments", rural_paths.len());

    // A torii gate straddling each rural road, set a little way out from the
    // gate — the threshold into the countryside. Japanese-only; no-op otherwise.
    let torii = crate::generator::paths::place_rural_torii(&*editor, &rural_paths, culture).await;
    if torii > 0 {
        println!("Placed {torii} rural torii gates");
    }

    // ── R3: paint rural production areas (after the roads) ────────────────
    for p in &placed_rural {
        let Some(painter) = &p.painter else { continue };
        let Some(district) = editor.world().districts.get(&p.district).cloned() else { continue };
        paint_production_area_for(&district, painter, &p.resource, &p.structure, &data, editor, &mut rng).await;
    }


    // ---- Industrial buildings FIRST ----
    // Place a handful of big processing buildings on the flattened ground (no
    // roads yet → sited by flatness). They become the destinations the arterial
    // network connects, plus a `blocked` barrier so nothing — roads, the
    // subdivision, alleys, or houses — ever runs through them. (Fixed set here;
    // the resource chain's `resolve_for_parcels` can supply the real mix later.)
    use crate::generator::BuildClaim;
    use crate::generator::placement::place_urban_buildings;

    let mut ind_counts: HashMap<String, u32> = HashMap::new();
    for b in ["smithy", "mill", "bakery", "carpenter", "tannery", "weaver"] {
        ind_counts.insert(b.to_string(), 1);
    }
    let urban_sds: Vec<_> = editor.world().districts.values()
        .filter(|sd| sd.data.parcel_type == crate::generator::districts::ParcelType::Urban)
        .cloned()
        .collect();
    let urban_sd_refs: Vec<_> = urban_sds.iter().collect();
    let n_before = editor.world().structures.len();
    // Re-skin the industrial NBTs into the settlement's culture palette
    // (their baked `resource_base` blocks → medieval spruce/stone).
    let ind_palette = data.palettes
        .get(&culture.palette_id())
        .expect("industry palette not found").clone();
    if let Err(e) = place_urban_buildings(&urban_sd_refs, &ind_counts, &mut rng, editor, &data, Some(&ind_palette)).await {
        log::warn!("industrial placement failed: {}", e);
    }
    println!(
        "Placed {} / {} industrial buildings",
        editor.world().structures.len() - n_before, ind_counts.values().sum::<u32>(),
    );
    let urban_industrial_count = editor.world().structures.len() - n_before;

    // ---- Rural buildings ----
    // The rural resource-chain pass above already placed each gathering/
    // processing building (farm, mine, sawmill, ranch, ...) in its own parcel and
    // painted its production area. Their `Structure` claim ids follow the urban
    // ones, so the worker-staffing pass below picks them up alongside the urban
    // shops.
    let rural_building_count = placed_rural.len();
    let rural_parcel_count = editor.world().districts.values()
        .filter(|sd| sd.data.parcel_type == ParcelType::Rural)
        .count();
    println!(
        "Placed {} rural buildings across {} rural parcels",
        rural_building_count, rural_parcel_count,
    );

    // Footprints → a `blocked` barrier (footprint + margin) and one node per
    // building for the network to connect.
    const IND_MARGIN: i32 = 2;
    let mut ind_footprints: HashMap<u32, Vec<Point2D>> = HashMap::new();
    for &p in &urban {
        if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
            ind_footprints.entry(id.id).or_default().push(p);
        }
    }
    let building_cells: HashSet<Point2D> = ind_footprints.values().flatten().copied().collect();
    let blocked: HashSet<Point2D> = building_cells.iter()
        .flat_map(|p| {
            (-IND_MARGIN..=IND_MARGIN).flat_map(move |dx| {
                (-IND_MARGIN..=IND_MARGIN).map(move |dz| Point2D::new(p.x + dx, p.y + dz))
            })
        })
        .collect();
    let ind_nodes: Vec<Point3D> = ind_footprints.values()
        .filter_map(|cells| {
            let c = cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len().max(1) as i32;
            editor.world().add_height(c)
        })
        .collect();

    // Phase 2 — tiered A* road network, connecting the industrial buildings
    // (anchor nodes) and routed around them (the `blocked` barrier).
    let arterial_material = MaterialId::new(arterial_mat.to_string());
    let collector_material = MaterialId::new(collector_mat.to_string());
    // Keep the whole network (not just `.paths`) so the end-of-run town map can
    // overlay the abstract MST/node graph.
    let road_network = build_road_network(
        &*editor, arterial_material, collector_material, true, &ind_nodes, &blocked, 1,
    ).await;
    let paths = road_network.paths.clone();
    println!("Routed {} road segments", paths.len());

    // DEBUG: Phase A merge check — how many of each path's cells coincide
    // with cells already laid by earlier paths? High overlap = routes are
    // merging onto the network instead of crossing it blindly.
    {
        let mut seen: HashSet<Point2D> = HashSet::new();
        for path in &paths {
            let cells: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
            let shared = cells.iter().filter(|c| seen.contains(c)).count();
            println!("  MERGE prio={:?} pts={} shared_with_network={}", path.priority(), cells.len(), shared);
            seen.extend(cells);
        }
    }

    // DEBUG: does the routed path y match the post-flatten heightmap?
    if let Some(path) = paths.first() {
        println!("--- path[0] sample: road_y vs ground_h vs ocean_h ---");
        for p in path.points().iter().take(25) {
            let xz = p.drop_y();
            let (Some(ground_h), Some(ocean_h)) = (
                editor.world().get_height_at(xz),
                editor.world().get_ocean_floor_height_at(xz),
            ) else {
                continue;
            };
            println!(
                "  ({:>4},{:>4})  road_y={:>3}  ground_h={:>3}  ocean_h={:>3}",
                xz.x, xz.y, p.y,
                ground_h,
                ocean_h,
            );
        }
    }

    // A path's *paved* cells — exactly what `build_paths_merged` lays:
    // centreline ∪ (width-1) ring. Used for block barriers and frontage
    // bands so blocks abut the real road edge (no gap ring).
    let paved = |path: &Path| -> HashSet<Point2D> {
        let centre: HashSet<Point2D> = path.points().iter().map(|p| p.drop_y()).collect();
        let mut cells = crate::geometry::get_surrounding_set(&centre, path.width().saturating_sub(1));
        cells.extend(centre);
        cells
    };

    // Blocks = urban minus the paved main roads and a buffer strip just inside
    // the wall, so houses never butt right up against it. The boundary ring is
    // dilated `WALL_BUFFER` cells inward; the resulting strip is left open (it
    // gets furnished as a green belt / wall-walk by the open-space pass).
    const WALL_BUFFER: i32 = 2;
    let wall_ring: HashSet<Point2D> = urban.iter()
        .filter(|&&c| crate::geometry::CARDINALS_2D.iter().any(|&d| !urban.contains(&(c + d))))
        .copied()
        .collect();
    let mut wall_zone = wall_ring.clone();
    let mut frontier = wall_ring;
    for _ in 0..WALL_BUFFER {
        let mut next: HashSet<Point2D> = HashSet::new();
        for &c in &frontier {
            for d in crate::geometry::CARDINALS_2D {
                let n = c + d;
                if urban.contains(&n) && wall_zone.insert(n) {
                    next.insert(n);
                }
            }
        }
        frontier = next;
    }
    let mut barriers: HashSet<Point2D> = HashSet::new();
    for path in &paths {
        barriers.extend(paved(path));
    }
    barriers.extend(&wall_zone);
    // Industrial buildings (footprint + margin) are barriers too, so blocks —
    // and the subdivision, alleys, and houses inside them — form *around* the
    // buildings, never through them.
    barriers.extend(&blocked);

    // Don't let blocks (and the lots/alleys/houses inside them) span steep
    // terrain. A per-cell cliff test misses a *sustained* slope — a long
    // staircase of 1-block risers passes cell-by-cell yet climbs far. So bar
    // any cell whose local WIN-radius neighbourhood spans more than
    // MAX_LOCAL_RELIEF blocks of height; the flood fill then breaks blocks
    // along slope lines, keeping lots and their lanes on a flat shelf.
    const WIN: i32 = 1; // 3×3 window
    const MAX_LOCAL_RELIEF: i32 = 2;
    let steep: HashSet<Point2D> = urban.iter()
        .filter(|&&c| {
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            for dx in -WIN..=WIN {
                for dz in -WIN..=WIN {
                    let n = Point2D::new(c.x + dx, c.y + dz);
                    if !urban.contains(&n) { continue; }
                    let Some(h) = editor.world().get_ocean_floor_height_at(n) else { continue; };
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            hi - lo > MAX_LOCAL_RELIEF
        })
        .copied()
        .collect();
    println!("Marked {} steep cells as barriers", steep.len());
    barriers.extend(&steep);

    let blocks = find_blocks(&urban, &barriers, 12);
    println!("Found {} blocks", blocks.len());

    // All main-road (arterial + collector) paved cells, used to peel a
    // frontage ribbon off each block before subdividing its interior.
    let main_road_cells: HashSet<Point2D> = {
        let mut s = HashSet::new();
        for path in &paths {
            s.extend(paved(path));
        }
        s
    };

    // Per block: first reserve a frontage ribbon — a band one house deep
    // against each main road — so the long arterial/collector-facing edge
    // stays a single continuous lot instead of being chopped into stubs
    // by subdivision. Then subdivide only the interior with tier-3 alleys.
    // BSP cuts span the interior edge-to-edge, so an alley reaches its edge —
    // adjacent (barriers = paved) to either a main road or the ribbon.
    // Deep enough to absorb both the deepest House (depth_range 7..=10) AND
    // the staircase rise of a diagonal frontage (an axis-aligned rect anchored
    // at the slice's interior extreme reaches `rise + depth` into the band).
    const RIBBON_DEPTH: i32 = 14;
    let mut sub_blocks: Vec<HashSet<Point2D>> = Vec::new();
    // Block index each lot belongs to (parallel to `sub_blocks`), so a lot can be
    // mapped back to its district's style scheme. A "district" here is one city
    // block from `find_blocks`.
    let mut lot_block: Vec<usize> = Vec::new();
    let mut alley_band: HashSet<Point2D> = HashSet::new();
    let mut ribbon_lot_count = 0usize;
    let mut ribbon_cells: HashSet<Point2D> = HashSet::new(); // DEBUG: all reserved ribbon cells
    for (block_idx, block) in blocks.iter().enumerate() {
        let (mut ribbon_lots, interior) =
            crate::generator::districts::subdivide::reserve_road_ribbon(block, &main_road_cells, RIBBON_DEPTH);
        let (subs, alleys) = crate::generator::districts::subdivide::subdivide_block(&interior, &mut rng, 24);

        // Connect the interior alleys to the main roads by carving through the
        // ribbon, then convert those cells from frontage ribbon to alley.
        let ribbon_union: HashSet<Point2D> = ribbon_lots.iter().flatten().copied().collect();
        let connectors = crate::generator::districts::subdivide::carve_ribbon_connectors(
            &ribbon_union, &alleys, &main_road_cells,
        );
        if !connectors.is_empty() {
            for rp in &mut ribbon_lots { rp.retain(|c| !connectors.contains(c)); }
            ribbon_lots.retain(|rp| !rp.is_empty());
        }

        ribbon_lot_count += ribbon_lots.len();
        for rp in &ribbon_lots { ribbon_cells.extend(rp); }
        let lots_this_block = ribbon_lots.len() + subs.len();
        sub_blocks.extend(ribbon_lots);
        alley_band.extend(&alleys);
        alley_band.extend(&connectors);
        sub_blocks.extend(subs);
        // Every lot just added (ribbons then subs) belongs to this block.
        lot_block.extend(std::iter::repeat(block_idx).take(lots_this_block));
    }
    println!(
        "Subdivided into {} lots ({} road-frontage ribbons), {} subdivider-road cells",
        sub_blocks.len(), ribbon_lot_count, alley_band.len(),
    );

    // Assemble every road into one path list (mains + a synthesised width-1
    // alley path), but DON'T build them yet — we build after the houses so
    // house-foundation earth can't bury the road. Houses are placed first and
    // sit their floor at the level of the road they front (see `road_h`).
    let alley_pts: Vec<Point3D> = alley_band.iter().filter_map(|c| editor.world().add_height(*c)).collect();
    let alley_path = Path::new(alley_pts, 1, MaterialId::new(collector_mat.to_string()), PathPriority::Low);
    let mut all_paths = paths.clone();
    all_paths.push(alley_path);

    // Road-height lookup over the paved band of every road (centreline +
    // width ring, min y on overlap), so a house can pin its floor to the
    // road it fronts. Built from `all_paths` so alley-facing houses get the
    // alley level too.
    let mut road_h: HashMap<Point2D, i32> = HashMap::new();
    for path in &all_paths {
        let w = path.width() as i32;
        for pt in path.points() {
            let base = pt.drop_y();
            for dx in -w..=w {
                for dz in -w..=w {
                    let c = Point2D::new(base.x + dx, base.y + dz);
                    road_h.entry(c).and_modify(|y| *y = (*y).min(pt.y)).or_insert(pt.y);
                }
            }
        }
    }

    // Frontage bands per tier (paved cells, matching the roads we'll build).
    let band = |prio: PathPriority| -> HashSet<Point2D> {
        let mut s = HashSet::new();
        for path in paths.iter().filter(|p| p.priority() == prio) {
            s.extend(paved(path));
        }
        s
    };
    let arterial_band = band(PathPriority::High);
    let collector_band = band(PathPriority::Medium);

    // Build the roads FIRST, then the houses. force_height grades the corridor
    // to the routed road heights, then build_paths_merged lays + melds the
    // surface. We then claim every paved cell as `Path` so the following
    // house foundations' terrain blending skips them (blend_terrain ignores
    // Path claims) — the road can't be buried by foundation earth. The graded
    // corridor is exactly `road_h` (same band, same min-on-overlap height).
    let corridor_pts: HashSet<Point3D> = road_h
        .iter()
        .map(|(c, &y)| Point3D::new(c.x, y, c.y))
        .collect();
    force_height(editor, &corridor_pts, false).await;
    // `build_paths_merged` returns the exact cells where it laid a half-step
    // slab; we raise a house a block over a fronting slab off this set rather
    // than reading the placed road back (the editor cache is keyed by local
    // coords while get_block subtracts the build-area origin, so a read here
    // returns world terrain, not the road).
    let road_slabs: HashSet<Point3D> = build_paths_merged(&*editor, &data, &all_paths, &mut rng).await;
    let slab_y_by_cell: HashMap<Point2D, i32> =
        road_slabs.iter().map(|p| (p.drop_y(), p.y)).collect();

    // Claim every paved road cell so house-foundation terraforming can't
    // touch it (blend_terrain skips `BuildClaim::Path`).
    for path in &all_paths {
        for c in paved(path) {
            editor.world_mut().claim(c, crate::generator::BuildClaim::Path(crate::generator::paths::PathType::Pavement));
        }
    }

    // ---- Phase 4: hierarchical house placement ----
    // Per lot, walk frontage densest-tier first: arterial → collector →
    // subdivider. The lot's single Plot is shared across tiers, so houses
    // placed against the arterial claim the prime frontage and later tiers
    // can't overlap them. Size gradient: houses on roads, cottages on lanes.
    use crate::generator::buildings_v2::{BuildCtx, BuildingContext, Culture, build_house};
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::footprint::{Footprint, SizeClass};
    use crate::generator::city_houses::{
        frontage_from_roads, plot_from_block, rect_from_frontage,
        synthetic_plot_bounds, SIDE_BUFFER_CELLS,
    };
    use crate::generator::materials::{Palette, PaletteId};
    use crate::geometry::Point2D as P2;

    // Per-settlement style composition: pick a dominant/secondary/accent trio
    // from this culture's catalog and apply them to buildings in a weighted
    // 60/30/10 mix (see StyleScheme), so the town reads as one coherent material
    // story with rare landmark punctuation. Variable wood pools lean toward the
    // local biome's timber. `style_rng` is derived so style selection doesn't
    // perturb the placement RNG stream.
    use crate::generator::buildings_v2::style::{StyleScheme, local_wood_palette};
    let local_wood: Option<PaletteId> = editor
        .world()
        .get_surface_biome_at(editor.world().world_rect_2d().midpoint())
        .and_then(local_wood_palette);
    let mut style_rng = rng.derive();
    let style_scheme = StyleScheme::generate(culture, &mut style_rng);
    println!(
        "Style scheme ({culture:?}): 60% {} / 30% {} / 10% {} — local wood {:?}",
        style_scheme.dominant().name,
        style_scheme.secondary().name,
        style_scheme.accent().name,
        local_wood.as_ref().map(|p| p.clone()),
    );
    // Per-district twist: each urban DISTRICT (one of the 3–5 `World.districts`
    // blobs, the same unit the chronicle names) keeps the town's dominant (60%)
    // and accent (10%) but re-rolls its 30% secondary to a different everyday
    // style, so a district reads as its own place while the city stays one whole
    // (see `StyleScheme::district_variant`). Each city block is mapped to the
    // urban district its cells mostly fall in; `lot_block` then maps each lot to
    // its block, hence to its district and scheme. Keyed off the seed per district
    // id, so it's deterministic and independent of the placement streams.
    use crate::generator::districts::DistrictID;
    let block_district: Vec<Option<DistrictID>> = blocks
        .iter()
        .map(|block| {
            let mut tally: HashMap<DistrictID, usize> = HashMap::new();
            for &c in block {
                if let Some(id) = editor.world().get_district_at(c) {
                    let urban = editor
                        .world()
                        .districts
                        .get(&id)
                        .map_or(false, |d| d.data.parcel_type == ParcelType::Urban);
                    if urban {
                        *tally.entry(id).or_insert(0) += 1;
                    }
                }
            }
            tally.into_iter().max_by_key(|(_, n)| *n).map(|(id, _)| id)
        })
        .collect();
    let district_schemes: HashMap<DistrictID, StyleScheme> = block_district
        .iter()
        .flatten()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|id| {
            let mut drng = RNG::from_seed_and_string(seed, &format!("district_style_{}", id.0));
            (id, style_scheme.district_variant(culture, &mut drng))
        })
        .collect();
    println!(
        "Per-district secondaries: {:?}",
        district_schemes.iter().map(|(id, s)| (id.0, &s.secondary().name)).collect::<Vec<_>>(),
    );
    // Densest tier first; size pool per tier (houses on the main roads,
    // cottages on the back lanes).
    // House + Hall on every tier. Manor is no longer opportunistic — it's
    // seeded in a deliberate pre-pass: pick MANOR_CAP arterial-eligible lots
    // up front and process them first with their arterial tier forced to
    // Manor-only. Lots without an arterial frontage long enough for a Manor
    // are ineligible. If a chosen Manor build fails we just continue (no
    // fallback to House/Hall on that slice); the lot still gets its other
    // tiers placed normally below.
    const MANOR_CAP: usize = 2;
    let mut manors_placed = 0usize;
    let manor_min_front = *SizeClass::Manor.front_width_range().start();
    // Manors prefer arterial frontage, but fall back to collector if no lot
    // touches an arterial with enough cells (common — arterials run through
    // the urban core but lots are bounded by collectors). `manor_tier_idx`
    // names which tier hosts the Manor pool inside the main loop (0 = arterial,
    // 1 = collector). Alley never hosts Manors.
    let eligible_for_band = |band: &HashSet<Point2D>| -> Vec<usize> {
        sub_blocks
            .iter()
            .enumerate()
            .filter(|(_, lot)| !lot.is_empty())
            .filter(|(_, lot)| {
                frontage_from_roads(lot, band)
                    .iter()
                    .any(|f| (f.cells.len() as i32) >= manor_min_front)
            })
            .map(|(i, _)| i)
            .collect()
    };
    let arterial_eligible = eligible_for_band(&arterial_band);
    let (eligible, manor_tier_idx, manor_tier_label): (Vec<usize>, usize, &str) =
        if !arterial_eligible.is_empty() {
            (arterial_eligible, 0, "arterial")
        } else {
            (eligible_for_band(&collector_band), 1, "collector (arterial empty)")
        };
    // Japanese manors are always engawa, which insets the interior by 2 cells a
    // side and so makes the default Manor footprint feel cramped. For that
    // culture only, run a candidate scan: dry-evaluate every eligible lot's manor
    // tier, find the slice with the largest *usable interior* (floorspace left
    // after the veranda inset), and build the two best at a widened, forced size.
    // Other cultures keep the original behaviour — two random eligible lots, the
    // standard Manor range, default random placement.
    #[derive(Clone, Copy)]
    struct ManorChoice { frontage_idx: usize, cursor: i32, fw: i32, depth: i32, score: i32 }
    let (manor_lots, manor_choices): (HashSet<usize>, HashMap<usize, ManorChoice>) =
        if culture == Culture::Japanese {
            const MANOR_MIN_FIT_DEPTH: i32 = 5; // mirrors MIN_FIT_DEPTH in the build loop
            // Widened range for engawa manors so the picked spot yields a grander
            // hall once the veranda eats its ring. Engawa-only — does not touch
            // the culture-agnostic `SizeClass::Manor` range other cultures use.
            const ENGAWA_MANOR_FW: std::ops::RangeInclusive<i32> = 11..=14;
            const ENGAWA_MANOR_DEPTH_HI: i32 = 16;
            let manor_band: &HashSet<Point2D> =
                if manor_tier_idx == 0 { &arterial_band } else { &collector_band };
            let mut best_by_lot: Vec<(usize, ManorChoice)> = Vec::new();
            for &lot_idx in &eligible {
                let Some(plot) = plot_from_block(&sub_blocks[lot_idx]) else { continue; };
                let mut best: Option<ManorChoice> = None;
                for (fi, frontage) in frontage_from_roads(&sub_blocks[lot_idx], manor_band).into_iter().enumerate() {
                    let chain_len = frontage.cells.len() as i32;
                    let mut cursor = 0;
                    while cursor + *ENGAWA_MANOR_FW.start() <= chain_len {
                        // Largest front width that fits at this cursor, then the
                        // deepest depth that fits — the biggest box this slice holds.
                        for fw in ENGAWA_MANOR_FW.rev() {
                            if cursor + fw > chain_len { continue; }
                            let chain_slice = &frontage.cells[cursor as usize..(cursor + fw) as usize];
                            let Some(depth) = (MANOR_MIN_FIT_DEPTH..=ENGAWA_MANOR_DEPTH_HI).rev()
                                .find(|&d| plot.is_rect_usable(&rect_from_frontage(chain_slice, frontage.outward, d)))
                            else { continue; };
                            // Usable interior after the 2-cell engawa inset a side.
                            let score = (fw - 4).max(0) * (depth - 4).max(0);
                            if best.map_or(true, |b| score > b.score) {
                                best = Some(ManorChoice { frontage_idx: fi, cursor, fw, depth, score });
                            }
                            break; // largest fw at this cursor found
                        }
                        cursor += 1;
                    }
                }
                if let Some(c) = best { best_by_lot.push((lot_idx, c)); }
            }
            // Highest usable interior first; lot index breaks ties for determinism.
            best_by_lot.sort_by(|a, b| b.1.score.cmp(&a.1.score).then(a.0.cmp(&b.0)));
            best_by_lot.truncate(MANOR_CAP);
            println!(
                "Manor pre-pass (engawa scan): {} {}-eligible lots, chose {} by largest interior (scores {:?})",
                eligible.len(), manor_tier_label, best_by_lot.len(),
                best_by_lot.iter().map(|(_, c)| c.score).collect::<Vec<_>>(),
            );
            let choices: HashMap<usize, ManorChoice> = best_by_lot.iter().copied().collect();
            let lots: HashSet<usize> = choices.keys().copied().collect();
            (lots, choices)
        } else {
            let mut pool = eligible.clone();
            rng.derive().shuffle(&mut pool);
            let lots: HashSet<usize> = pool.into_iter().take(MANOR_CAP).collect();
            println!(
                "Manor pre-pass: {} {}-eligible lots, chose {} to host Manors",
                eligible.len(), manor_tier_label, lots.len(),
            );
            (lots, HashMap::new())
        };
    // Iterate manor-lots first (in shuffled order), then everything else in
    // natural sub_block order. "Before the rest of the houses" is enforced
    // by the iteration order alone — no separate code path.
    let lot_order: Vec<usize> = manor_lots
        .iter()
        .copied()
        .chain((0..sub_blocks.len()).filter(|i| !manor_lots.contains(i)))
        .collect();

    // Town colour identity: two recurring colours sampled per ordinary building
    // (50/25/25 with a random accent) plus a unique family colour per manor. A
    // dedicated derived RNG keeps colour draws from shifting the placement
    // streams. The two town colours feed the settlement namer further below.
    let color_scheme = ColorScheme::new(culture, MANOR_CAP, &mut rng.derive());
    let mut color_rng = rng.derive();
    println!(
        "Town colours: dominant={:?}, second={:?}; manor family colours={:?}",
        color_scheme.town[0], color_scheme.town[1], color_scheme.manor,
    );
    // Civic banner: mint the town's arms from its two colours and fly them, facing
    // outward, on the wall towers and gates (built earlier). The blazon rides along
    // to the chronicle so the book can name the arms.
    let civic_centre = {
        let n = urban.len().max(1) as i32;
        urban.iter().fold(Point2D::ZERO, |a, &p| a + p) / n
    };
    let mut civic_rng = rng.derive();
    let civic_blazon = crate::generator::civic_banner::place_civic_banners(
        editor, civic_centre, color_scheme.town, &mut civic_rng,
    )
    .await
    .unwrap_or_default();
    if !civic_blazon.is_empty() {
        println!("Civic banner: {civic_blazon}");
    }
    // Per-manor name signs are planned during the building loop (geometry known
    // once the door is cut) and lettered after the population pass rolls each
    // family's surname. `sign_rng` keeps the designation draw off the placement
    // streams. Designation varies per manor for flavour.
    const MANOR_DESIGNATIONS: [&str; 4] = ["Manor", "Estate", "Hall", "House"];
    let mut sign_rng = rng.derive();
    let mut manor_sign_sites: Vec<crate::generator::buildings_v2::exterior::ManorSignSite> =
        Vec::new();
    // Manor families for the chronicle, paired with their surname once the
    // population pass names each household (see the manor-sign lettering loop).
    let mut manor_facts: Vec<ManorFact> = Vec::new();

    let mut total_buildings = 0usize;
    // Per-house NPC anchors + bed-derived population budget, gathered from every
    // house and fed to the town-wide population pass once the town is built.
    let mut town_anchors: Vec<crate::generator::population::HouseAnchors> = Vec::new();
    // Houses placed per SizeClass — used to size the wealth distribution
    // (Cottage/House = common, Hall = wealthy craftsman, Manor = elite).
    let mut size_counts: HashMap<String, usize> = HashMap::new();
    // Footprint rect-count distribution (1 = single-rect / no wings, 2 = one
    // wing, 3+ = multi-wing L/T/U shapes). Reads how often wings actually land.
    let mut rect_count_dist: HashMap<usize, usize> = HashMap::new();
    let mut tier_cells = [0usize; 3];   // frontage cells found per tier
    let mut tier_placed = [0usize; 3];  // houses placed per tier
    let mut tier_fail = [0usize; 3];    // build_house failures per tier
    let mut tier_short = [0usize; 3];   // chains dropped: shorter than min_front
    let mut tier_unfit = [0usize; 3];   // slots skipped: rect didn't fit the lot
    // DEBUG: every cell detected as frontage, per tier, so we can float a
    // marker above it and see what the placement loop actually "sees".
    let mut tier_frontage: [HashSet<Point2D>; 3] = Default::default();
    // Verge cells per main-road tier (arterial, collector): the gap between
    // the road and each house front, which we pave into a forecourt so the
    // unavoidable set-back on a diagonal reads as a shoulder, not bare grass.
    let mut tier_verge: [HashSet<Point2D>; 2] = Default::default();
    for lot_idx in lot_order {
        let lot = &sub_blocks[lot_idx];
        if lot.is_empty() { continue; }
        let Some(mut plot) = plot_from_block(lot) else { continue; };

        // On a chosen manor-lot, the manor's tier (arterial when arterials
        // had eligible frontages; otherwise collector) gets a Manor-only
        // pool until the cap is reached. Other tiers — and other lots —
        // stay House+Hall. Alley never hosts Manors.
        let is_manor_lot = manor_lots.contains(&lot_idx) && manors_placed < MANOR_CAP;
        let arterial_pool: &[SizeClass] = if is_manor_lot && manor_tier_idx == 0 {
            &[SizeClass::Manor]
        } else {
            &[SizeClass::House, SizeClass::Hall]
        };
        let collector_pool: &[SizeClass] = if is_manor_lot && manor_tier_idx == 1 {
            &[SizeClass::Manor]
        } else {
            &[SizeClass::House, SizeClass::Hall]
        };
        let tiers_local: [(&HashSet<Point2D>, &[SizeClass]); 3] = [
            (&arterial_band, arterial_pool),
            (&collector_band, collector_pool),
            (&alley_band, &[SizeClass::House, SizeClass::Hall]),
        ];

        'tier_loop: for (ti, (band, pool)) in tiers_local.iter().enumerate() {
            let min_front = pool.iter().map(|s| *s.front_width_range().start()).min().unwrap_or(0);
            for (fi, frontage) in frontage_from_roads(lot, band).into_iter().enumerate() {
                tier_cells[ti] += frontage.cells.len();
                tier_frontage[ti].extend(&frontage.cells);
                let chain_len = frontage.cells.len() as i32;
                if chain_len < min_front { tier_short[ti] += 1; continue; }
                // On a manor lot's manor tier, place the one manor the scan sized
                // at its chosen slice; skip every other frontage of that tier so
                // no second, smaller manor gets seated.
                let manor_here: Option<ManorChoice> = if is_manor_lot && ti == manor_tier_idx {
                    match manor_choices.get(&lot_idx) {
                        // Engawa scan picked this lot: build only at its chosen
                        // slice, skipping the tier's other frontages.
                        Some(c) if c.frontage_idx == fi => Some(*c),
                        Some(_) => continue,
                        // No scan (non-engawa culture): fall through to the
                        // default random Manor placement on this tier.
                        None => None,
                    }
                } else {
                    None
                };
                let mut cursor: i32 = match manor_here {
                    Some(c) => c.cursor,
                    None => if min_front > 1 { rng.rand_i32_range(0, min_front) } else { 0 },
                };
                // Shallowest depth we'll accept on a slice that can't take the
                // rolled depth — lets diagonal frontage (where an axis-aligned
                // rect overruns the staircased ribbon) still seat a house.
                const MIN_FIT_DEPTH: i32 = 5;
                while cursor + min_front <= chain_len {
                    // A scanned manor forces its size + slice; everything else
                    // rolls a size from the tier pool and the deepest fitting depth.
                    let size_class = match manor_here {
                        Some(_) => SizeClass::Manor,
                        None => *rng.choose(pool),
                    };
                    let fw = match manor_here {
                        Some(c) => c.fw,
                        None => rng.rand_i32_range(*size_class.front_width_range().start(), *size_class.front_width_range().end() + 1),
                    };
                    if cursor + fw > chain_len {
                        if manor_here.is_some() { break; }
                        cursor += 1; continue;
                    }
                    let chain_slice = &frontage.cells[cursor as usize..(cursor + fw) as usize];
                    let depth = match manor_here {
                        Some(c) => c.depth,
                        None => {
                            // Square-frontage bias: with the culture's square chance,
                            // make the house a square (depth = front width) if it fits,
                            // so it gets a dome. Guarded so a 0 bias never draws RNG.
                            // Otherwise pick the deepest depth (down to MIN_FIT_DEPTH)
                            // that fits, shrinking the house to hug a diagonal ribbon.
                            let max_depth = rng.rand_i32_range(*size_class.depth_range().start(), *size_class.depth_range().end() + 1);
                            let want_square = culture.square_bias() > 0
                                && rng.percent(culture.square_bias())
                                && plot.is_rect_usable(&rect_from_frontage(chain_slice, frontage.outward, fw));
                            if want_square {
                                fw
                            } else if let Some(d) = (MIN_FIT_DEPTH..=max_depth).rev()
                                .find(|&d| plot.is_rect_usable(&rect_from_frontage(chain_slice, frontage.outward, d)))
                            {
                                d
                            } else {
                                tier_unfit[ti] += 1; cursor += 1; continue;
                            }
                        }
                    };
                    let rect = rect_from_frontage(chain_slice, frontage.outward, depth);
                    // The frontage rect becomes the core; try to grow wings
                    // into the lot's remaining usable cells (away from the
                    // road). Square_bias = 0 here matches the live town gen —
                    // domes on wings haven't been wired through yet.

                    // Pick this building's style from its *district's* scheme
                    // (60% town dominant / 30% district secondary / 10% town
                    // accent) and roll its palette, leaning on the local wood.
                    // Falls back to the town scheme for the rare lot whose block
                    // mapped to no urban district (footprint cells regularized
                    // beyond raw district coverage).
                    let scheme = block_district[lot_block[lot_idx]]
                        .and_then(|id| district_schemes.get(&id))
                        .unwrap_or(&style_scheme);
                    let mut palette = scheme
                        .next_style(&mut style_rng)
                        .roll_palette(&mut style_rng, &data, local_wood.as_ref());
                    // Apply the town colour identity: a manor flies its unique
                    // family colour; every other building draws from the weighted
                    // town scheme. This `primary_color` then tints the building's
                    // dyed accents (banners, beds, carpets) via the recolour pass.
                    let family_color: Option<Color> = if size_class == SizeClass::Manor {
                        color_scheme.manor.get(manors_placed).copied()
                    } else {
                        None
                    };
                    palette.primary_color = Some(
                        family_color.unwrap_or_else(|| color_scheme.next_color(&mut color_rng)),
                    );
                    // A manor family also flies one heraldic banner design on
                    // every banner it owns (door + interior): the family colour
                    // is the field, a distinct second colour the charge. Stamped
                    // via `palette.banner_data` wherever a banner is recoloured.
                    // The blazon rides along to the household for the chronicle.
                    palette.banner_data = None;
                    let mut banner_blazon: Option<String> = None;
                    if let Some(primary) = family_color {
                        let secondary = color_scheme.distinct_from(primary, &mut color_rng);
                        if let Some(b) = crate::generator::heraldry::pick_family_banner(
                            primary, secondary, &mut color_rng,
                        ) {
                            println!("Manor banner: {}", b.blazon);
                            palette.banner_data = Some(b.data);
                            banner_blazon = Some(b.blazon);
                        }
                    }
                    // Roof style weighted by culture + size (irimoya skews to the
                    // grander buildings; see `roof_styles_for`).
                    let roof_styles = culture.roof_styles_for(size_class);
                    let roof_style = roof_styles[rng.rand_i32_range(0, roof_styles.len() as i32) as usize];
                    let footprint = crate::generator::buildings_v2::footprint::generate::generate_footprint_from_core(
                        &mut rng, &plot, rect, frontage.outward, &size_class, culture.square_bias(),
                    );
                    // Door scoring needs the full footprint bounds, not just
                    // the core rect, so a wing extending rearward doesn't
                    // misreport the back wall's distance to the plot edge.
                    let plot_bounds = synthetic_plot_bounds(&footprint.bounds(), frontage.outward);
                    // Align the main door with the road it faces: pin the floor
                    // (= door sill) to the height of the *nearest* road cell to
                    // this frontage. Probe outward from every frontage cell and
                    // keep the closest road-height hit.
                    let road_dir = P2::from(frontage.outward);
                    let base_lvl = {
                        let mut best: Option<(i32, i32, P2)> = None; // (dist, height, road cell)
                        for &c in chain_slice {
                            for step in 1..=RIBBON_DEPTH {
                                let probe = c + P2::new(road_dir.x * step, road_dir.y * step);
                                if let Some(&y) = road_h.get(&probe) {
                                    if best.map_or(true, |(bd, _, _)| step < bd) { best = Some((step, y, probe)); }
                                    break;
                                }
                            }
                        }
                        best.map(|(_, y, cell)| {
                            // If the fronting road cell carries a half-step slab,
                            // raise the floor one block above the slab so the door
                            // steps down onto it instead of opening onto a lip.
                            match slab_y_by_cell.get(&cell) {
                                Some(&slab_y) => slab_y + 1,
                                None => y,
                            }
                        })
                    };
                    let mut bctx = BuildingContext::new(culture, size_class, roof_style);
                    bctx.base_y_override = base_lvl;
                    // Roll the engawa veranda per the culture/size taste (every
                    // Japanese Manor, a third of Halls; see `engawa_chance`).
                    // `build_house` / `plan_engawa` still gate on the core rect
                    // staying large enough and fall back to plain walls otherwise.
                    let (en, ed) = culture.engawa_chance(size_class);
                    bctx.engawa = ed > 0 && rng.rand_i32_range(0, ed as i32) < en as i32;
                    // Roll the jetty per culture taste (medieval timber-frame
                    // upper-floor overhang; see `jetty_chance`). Frame generation
                    // gates on shape/floor-count/plot fit and silently no-ops when
                    // ineligible, so an un-jettiable building just stays flush.
                    // Engawa wins in the pipeline, but the cultures don't overlap
                    // (jetty is medieval-only, engawa Japanese-only).
                    let (jn, jd) = culture.jetty_chance();
                    bctx.jetty = jd > 0 && rng.rand_i32_range(0, jd as i32) < jn as i32;
                    let mut bctx_editor = BuildCtx::new(editor, &data, &palette, &mut rng);
                    match build_house(&mut bctx_editor, footprint, &bctx, plot_bounds).await {
                        Ok(output) => {
                            // A manor flies its family colour: banners flanking
                            // the front door so the street reads the household
                            // before you step inside. Other buildings carry their
                            // colour only on interior accents.
                            if let Some(color) = family_color {
                                crate::generator::buildings_v2::exterior::place_family_banner(
                                    &mut bctx_editor, &output.wall_segs, color,
                                ).await;
                                // Plan its name sign now the door is known; it gets
                                // lettered after the population pass names the family.
                                let designation = MANOR_DESIGNATIONS[
                                    sign_rng.rand_i32_range(0, MANOR_DESIGNATIONS.len() as i32) as usize
                                ].to_string();
                                if let Some(site) = crate::generator::buildings_v2::exterior::plan_manor_sign(
                                    &output.wall_segs, &palette, &data.materials,
                                    &mut sign_rng, town_anchors.len(), designation,
                                ) {
                                    manor_sign_sites.push(site);
                                }
                            }
                            // Population budget tracks sleeping capacity, not bed
                            // furniture: a double/canopy bed sleeps two. Each
                            // bed-tagged item's capacity is its number of
                            // `part=foot` blocks (the head auto-spawns), min 1.
                            let beds: usize = output
                                .room_plan
                                .rooms
                                .iter()
                                .flat_map(|r| &r.furniture)
                                .filter_map(|f| data.furniture.items.get(&f.name))
                                .filter(|it| it.tags.iter().any(|t| t == "bed"))
                                .map(|it| {
                                    it.blocks
                                        .iter()
                                        .filter(|b| b.block.contains("part=foot"))
                                        .count()
                                        .max(1)
                                })
                                .sum();
                            // Scale capacity so houses feel lived-in, floored at 1.
                            let population =
                                ((beds as f32 * POPULATION_PER_BED).round() as usize).max(1);
                            town_anchors.push(crate::generator::population::HouseAnchors {
                                scenes: output.npc_anchors,
                                population,
                                wealth: crate::generator::population::Wealth::from_size_class(size_class),
                                pos: output.footprint.bounds().midpoint(),
                                family_color,
                                banner_blazon,
                            });
                            // Mark every rect in the footprint (core + wings)
                            // as used so subsequent placements on this lot
                            // can't overlap the wing cells.
                            for r in output.footprint.rects() {
                                plot.mark_rect_used(r, SIDE_BUFFER_CELLS);
                            }
                            *rect_count_dist.entry(output.footprint.rects().len()).or_insert(0) += 1;
                            total_buildings += 1;
                            tier_placed[ti] += 1;
                            *size_counts.entry(format!("{:?}", size_class)).or_insert(0) += 1;
                            if size_class == SizeClass::Manor {
                                manors_placed += 1;
                            }
                            // Record the verge: from each frontage cell, walk
                            // into the block (−outward) until we reach the
                            // house. On a straight slice this is just the
                            // frontage row; on a diagonal it's the triangular
                            // set-back we want to pave over.
                            if ti < 2 {
                                let road_dir = P2::from(frontage.outward);
                                let into = P2::new(-road_dir.x, -road_dir.y);
                                for &c in chain_slice {
                                    let mut p = c;
                                    let mut guard = 0;
                                    while !rect.contains(p) && guard < 32 {
                                        tier_verge[ti].insert(p);
                                        p = p + into;
                                        guard += 1;
                                    }
                                }
                            }
                            // A Manor closes out its lot's manor-tier: skip any
                            // remaining frontages/cursors here so we don't tile
                            // additional Manors along the same chain. Other
                            // tiers (collector/alley) of the lot still process
                            // normally below.
                            if size_class == SizeClass::Manor {
                                continue 'tier_loop;
                            }
                            cursor += fw + SIDE_BUFFER_CELLS;
                        }
                        Err(msg) => {
                            tier_fail[ti] += 1;
                            log::warn!("placement build_house failed: {}", msg);
                            // A scanned manor gets one shot; on failure give up its
                            // tier rather than retrying the forced size shifted over.
                            if manor_here.is_some() { continue 'tier_loop; }
                            cursor += 1;
                        }
                    }
                }
            }
        }
    }
    println!("Placed {} buildings across {} lots", total_buildings, sub_blocks.len());
    {
        let order = ["Cottage", "House", "Hall", "Manor"];
        let parts: Vec<String> = order
            .iter()
            .map(|k| format!("{}: {}", k, size_counts.get(*k).copied().unwrap_or(0)))
            .collect();
        println!("Size class breakdown — {}", parts.join("  "));
    }
    {
        let mut rcounts: Vec<(usize, usize)> = rect_count_dist.iter().map(|(&k, &v)| (k, v)).collect();
        rcounts.sort_unstable_by_key(|&(k, _)| k);
        let parts: Vec<String> = rcounts.iter().map(|(k, v)| format!("{} rect: {}", k, v)).collect();
        println!("Footprint shape — {}", parts.join("  "));
    }
    println!(
        "Per-tier [frontage cells / placed / failed] — arterial: {}/{}/{}  collector: {}/{}/{}  subdivider: {}/{}/{}",
        tier_cells[0], tier_placed[0], tier_fail[0],
        tier_cells[1], tier_placed[1], tier_fail[1],
        tier_cells[2], tier_placed[2], tier_fail[2],
    );
    println!(
        "Per-tier skips [short-chain / rect-unfit] — arterial: {}/{}  collector: {}/{}  subdivider: {}/{}",
        tier_short[0], tier_unfit[0],
        tier_short[1], tier_unfit[1],
        tier_short[2], tier_unfit[2],
    );

    // Pave the verge: a forecourt of the road's own material in the gap
    // between each main road and its houses, so the diagonal set-back reads
    // as a paved shoulder. Painted at the live ground top (h-1), matching the
    // post-flatten/foundation surface. Arterial verge = stone bricks (its
    // road material), collector verge = cobblestone.
    let verge_blocks = [
        Block { id: arterial_mat.into(), data: None, state: None },
        Block { id: collector_mat.into(), data: None, state: None },
    ];
    let mut verge_total = 0usize;
    for (ti, cells) in tier_verge.iter().enumerate() {
        for c in cells {
            let Some(h) = editor.world().get_ocean_floor_height_at(*c) else { continue; };
            editor.place_block(&verge_blocks[ti], Point3D::new(c.x, h - 1, c.y)).await;
            verge_total += 1;
        }
    }
    println!("Paved {} verge cells (arterial {} + collector {})", verge_total, tier_verge[0].len(), tier_verge[1].len());

    // Street lighting: run last, after houses have claimed their cells, so
    // lamps line every road's verge without landing on a building. The city
    // generator picks the lantern type city-wide.
    let city_rect = editor.world().world_rect_2d();
    let city_centre = (city_rect.origin + city_rect.max()) / 2;
    let cold = match editor.world().get_surface_biome_at(city_centre) {
        Some(biome) => {
            let n = biome.name();
            n.contains("snowy") || n.contains("frozen") || n.contains("taiga")
        }
        None => false,
    };
    let street_lantern: crate::minecraft::Block = if cold {
        "minecraft:soul_lantern".into()
    } else {
        "minecraft:lantern".into()
    };
    let lamps = crate::generator::paths::place_street_lights(&*editor, &all_paths, &street_lantern).await;
    println!("Placed {} street lamps", lamps.len());

    // Name the roads (layered: landmark → gate/centre → generic) now that all
    // buildings have claimed their cells, then sign the intersections. Runs
    // before the open-space pass; each sign cell is claimed as a path so
    // plazas/parks/etc. won't furnish over it.
    let mut name_rng = RNG::new(seed).derive();
    let road_names = crate::generator::paths::name_roads_layered(
        editor.world(), &road_network.road_labels, &all_paths,
        &editor.world().gate_locations.clone(), culture, &mut name_rng,
    );
    let signs = crate::generator::paths::place_street_signs(
        editor, &all_paths, &road_network.road_labels, &road_names,
    ).await;
    println!("Placed {} street signs", signs.len());

    // ---- Open spaces: furnish the leftover gaps between buildings and roads ----
    // Detect the empty pockets inside the wall and furnish each by type: plazas
    // (paved civic squares), nooks (small ringed gardens), parks (large green
    // commons), and yards (perimeter kitchen gardens).
    let mut place_labels: Vec<(Point2D, String)> = Vec::new();
    // NPC standing-spot scenes harvested from the open spaces — plazas (stage
    // performers, market vendors, onlookers in the crowd) and parks (idle folk
    // strolling the green). Staffed as fixtures after furnishing, independent of
    // the resident bed budget — a market or park is busy regardless of how many
    // beds the town has.
    let mut plaza_scenes: Vec<crate::generator::population::AnchorScene> = Vec::new();
    // Open-space landmark keys (plaza/park `.key()`) gathered for the settlement
    // namer — a town with a market or graveyard can be named for it.
    let mut civic_features: Vec<String> = Vec::new();
    {
        use crate::generator::open_space::{
            detect_regions, furnish_nook, furnish_park, furnish_plaza, furnish_yard, OpenSpaceNames,
            ParkType, Theme, RegionType,
        };
        let regions = detect_regions(editor.world(), &urban);
        let theme = Theme::for_culture(culture);
        let mut os_rng = rng.derive();
        // Names are picked alongside furnishing so a park is named for the type it
        // was actually built as; `used` keeps every name unique within the town.
        let names = OpenSpaceNames::load();
        let mut used: HashSet<String> = HashSet::new();
        let mut counts = [0usize; 4]; // plaza, nook, park, yard
        // Stone lanterns (tōrō) scattered through the green spaces — Japanese
        // only; the call is a no-op for other cultures, so we ring nooks and parks
        // (but not yards, nor paved plazas, nor cemeteries) after furnishing. They
        // are stoned to match each garden's own masonry (`theme.stone`).
        let mut garden_lanterns = 0usize;
        for region in &regions {
            match region.region_type() {
                RegionType::Plaza => {
                    let (plaza_type, scenes) = furnish_plaza(&*editor, region, &mut os_rng, &theme).await;
                    plaza_scenes.extend(scenes);
                    civic_features.push(plaza_type.key().to_string());
                    if let Some(name) = names.as_ref().and_then(|n| n.name_plaza(plaza_type, culture, &mut os_rng, &mut used)) {
                        place_labels.push((region.centroid(), name));
                    }
                    counts[0] += 1;
                }
                RegionType::Nook => {
                    furnish_nook(&*editor, region, &mut os_rng, &theme).await;
                    garden_lanterns += crate::generator::paths::scatter_garden_lanterns(
                        &*editor, region, &data, culture, theme.stone, &mut os_rng,
                    ).await;
                    counts[1] += 1;
                }
                RegionType::Park => {
                    let (park_type, scenes) = furnish_park(editor, region, &mut os_rng, &theme).await;
                    plaza_scenes.extend(scenes);
                    civic_features.push(park_type.key().to_string());
                    if let Some(name) = names.as_ref().and_then(|n| n.name_park(park_type, culture, &mut os_rng, &mut used)) {
                        place_labels.push((region.centroid(), name));
                    }
                    // Skip cemeteries — a glowing lantern doesn't suit a graveyard.
                    if park_type != ParkType::Cemetery {
                        garden_lanterns += crate::generator::paths::scatter_garden_lanterns(
                            &*editor, region, &data, culture, theme.stone, &mut os_rng,
                        ).await;
                    }
                    counts[2] += 1;
                }
                RegionType::Yard => {
                    furnish_yard(&*editor, region, &mut os_rng, &theme).await;
                    counts[3] += 1;
                }
            }
        }
        println!(
            "Furnished open spaces — plaza {} nook {} park {} yard {} | {} garden lanterns",
            counts[0], counts[1], counts[2], counts[3], garden_lanterns,
        );

        // Skin the plaza fixtures from data: a stage's performers and a stall's
        // vendor each roll a look from their fixture pool. Onlookers/browsers in
        // the crowd keep the roster's own look (their slots are left untouched).
        use crate::generator::population::{SceneKind, SlotRole};
        let mut plaza_look_rng = rng.derive();
        for scene in plaza_scenes.iter_mut() {
            let performance = scene.kind == SceneKind::Performance;
            for slot in scene.slots.iter_mut() {
                let fixture = if performance {
                    &data.npc_data.performers
                } else if slot.role == SlotRole::Worker {
                    &data.npc_data.vendors
                } else {
                    continue;
                };
                slot.look = Some(*plaza_look_rng.choose(&fixture.looks));
            }
        }
    }

    // Count plaza employment for the jobs summary: a stall is any scene with a
    // `Worker` slot (market vendors), a stage is a `Performance` scene, and its
    // performer slots are the per-stage cast. Onlookers/browsers aren't jobs.
    let (market_stall_count, stage_count, performer_slot_count) = {
        use crate::generator::population::{SceneKind, SlotRole};
        let stalls = plaza_scenes.iter()
            .filter(|s| s.slots.iter().any(|sl| sl.role == SlotRole::Worker))
            .count();
        let stages = plaza_scenes.iter()
            .filter(|s| s.kind == SceneKind::Performance)
            .count();
        let performers: usize = plaza_scenes.iter()
            .filter(|s| s.kind == SceneKind::Performance)
            .map(|s| s.slots.len())
            .sum();
        (stalls, stages, performers)
    };

    // Town-wide NPC id allocator. Shared across every staffing call below
    // (plaza fixtures, residents, workplace workers, guards) so every NPC has
    // a unique id and kin relationships can reference any of them.
    let mut id_alloc = crate::generator::population::IdAllocator::new();

    // ---- Plaza fixtures: staff every harvested plaza scene ----
    // Stage performers, market vendors, and onlookers are fixtures like the
    // industrial workers below — always placed, independent of the resident bed
    // budget. Each scene already carries its own position, facing, dialogue key,
    // and bubble volume (criers/performers yell), so we just hand them a roster
    // and staff them all. Live-only: no-op offline.
    if !plaza_scenes.is_empty() {
        use crate::generator::population::{build_roster, populate_npcs, Occupant};
        let npc_data = &data.npc_data;
        let budget = plaza_scenes.len();
        // Some market/stage onlookers are kid slots; mint exactly that many
        // children in the roster so those scenes can be staffed (rest adults).
        let kids = plaza_scenes
            .iter()
            .flat_map(|s| &s.slots)
            .filter(|sl| sl.occupant == Occupant::ChildOnly)
            .count();
        let roster = build_roster(budget, kids, culture, npc_data, &mut id_alloc, &mut rng.derive());
        match populate_npcs(editor, plaza_scenes, roster, budget, npc_data, &mut rng).await {
            Ok(staffed) => println!("Staffed {} plaza NPCs", staffed),
            Err(e) => log::warn!("plaza staffing failed: {e}"),
        }
    }

    // Worker binding runs inside the population pass below (before residential
    // placement). These outlive that scope so the worker-fixture block can
    // backfill the posts binding couldn't fill from residents, and the jobs
    // summary can report the full per-trade tally.
    let mut workplace_backfill: Vec<crate::generator::population::WorkerSlot> = Vec::new();
    let mut bound_worker_count = 0usize;
    let mut workplace_count = 0usize;
    let mut worker_by_job: HashMap<String, usize> = HashMap::new();
    // Resident headcount (sum of per-house bed budgets), captured from the
    // population block below so the chronicle can quote the town's size. This is
    // the deterministic target the crowd is sized to, computed regardless of the
    // live NPC placement (which is a no-op offline).
    let mut population_count = 0usize;

    // ---- Population: size the resident crowd to beds, scatter it town-wide ----
    // Each house's budget is max(1, beds); the town total is their sum.
    // Residents come from generated households (kin reciprocally wired, then
    // cross-household links, then employment), and the town-wide draw seeds
    // one resident per house, then fills the rest weighted by anchor weight,
    // halving a house's weights each time it gains a resident so the crowd
    // spreads instead of clustering. Live-only: no-op offline.
    {
        use crate::generator::population::{
            assign_employment, build_households, link_cross_household,
            log_population_stats, log_sample_households, populate_town,
        };
        let budget: usize = town_anchors.iter().map(|h| h.population).sum();
        population_count = budget;
        let candidate_anchors: usize = town_anchors.iter().map(|h| h.scenes.len()).sum();
        println!(
            "Population target: {} residents across {} houses ({} candidate anchors)",
            budget,
            town_anchors.len(),
            candidate_anchors,
        );
        let npc_data = &data.npc_data;
        // Four passes: shape households per house, link kin across town,
        // assign professions, place at anchors. Each pass derives its own
        // RNG so reordering or inserting a future pass doesn't shift
        // downstream rolls.
        let mut population = build_households(
            &town_anchors, culture, npc_data, &mut id_alloc, &mut rng.derive(),
        );
        link_cross_household(&mut population, &mut rng.derive());
        assign_employment(&mut population, &mut rng.derive());

        // Diagnostics: stats + a handful of sampled households so the kin graph
        // is legible in the console without needing a debugger.
        log_population_stats(&population);
        log_sample_households(&population, 8);

        // Letter each manor's name sign now its household surname is known, and
        // record the family for the chronicle (surname + the manor's location,
        // colour, and blazon from its `HouseAnchors`).
        for site in &manor_sign_sites {
            if let Some(hh) = population.households.iter().find(|h| h.home == site.anchor_idx) {
                println!("Manor sign: {} {}", hh.surname, site.designation());
                let anchor = &town_anchors[site.anchor_idx];
                manor_facts.push(ManorFact {
                    surname: hh.surname.clone(),
                    designation: site.designation().to_string(),
                    pos: anchor.pos,
                    color: anchor.family_color,
                    blazon: anchor.banner_blazon.clone(),
                });
                crate::generator::buildings_v2::exterior::place_manor_sign(
                    editor, site, &hh.surname,
                ).await;
            }
        }

        // Bind residents to workplaces before seating anyone at home. The draft
        // weights each unplaced adult by qualification (proximity-led) and spawns
        // the winner at the post; posts with no taker left fall through to
        // anonymous fixtures (`workplace_backfill`). Bound workers are marked
        // `placed`, so `populate_town` skips them — each resident appears once.
        // Every post (resident-filled or not) is tallied by trade for the summary.
        use crate::generator::population::bind_workers;
        let (work_slots, n_workplaces) = discover_worker_slots(editor, &data);
        workplace_count = n_workplaces;
        for s in &work_slots {
            *worker_by_job.entry(s.employment.clone()).or_insert(0) += 1;
        }
        match bind_workers(editor, &mut population, work_slots, npc_data, &mut rng.derive()).await {
            Ok((bound, unfilled)) => {
                println!(
                    "Bound {} residents to workplaces; {} posts need fixtures",
                    bound,
                    unfilled.len(),
                );
                bound_worker_count = bound;
                workplace_backfill = unfilled;
            }
            Err(e) => log::warn!("worker binding failed: {e}"),
        }

        match populate_town(editor, town_anchors, population, npc_data, &mut rng).await {
            Ok(placed) => println!("Populated {} NPCs", placed),
            Err(e) => log::warn!("NPC population failed: {e}"),
        }
    }

    // ---- Worker fixtures: staff every workplace ----
    // Stand a small crew of worker NPCs just outside each placed building (urban
    // processing shop or rural gather building), facing it, wearing the trade
    // outfit that matches its type. These are fixtures: always placed, independent
    // of the resident budget above. The NBT interiors are opaque, so workers stand
    // on clear ground cells at the footprint edge — never inside, never on a road
    // or another building. A workplace can employ several hands (see the per-kind
    // `workers` count in `data/npcs.yaml`).
    {
        use crate::generator::population::{
            build_roster, populate_npcs, AnchorScene, AnchorSlot, SceneKind, SlotRole,
        };

        // Workplace posts that binding couldn't fill from residents get an
        // anonymous fixture: a fresh skin rolled from the post's pool, standing
        // where binding would have seated the resident. (Residents bound to
        // workplaces were already spawned in the population pass above; the claim
        // scan + stand-cell geometry now lives in `discover_worker_slots`.)
        let npc_data = &data.npc_data;
        let mut worker_rng = rng.derive();

        let mut worker_scenes: Vec<AnchorScene> = Vec::new();
        for slot in &workplace_backfill {
            let look = *worker_rng.choose(&slot.looks);
            worker_scenes.push(AnchorScene::worker(slot.stand, slot.facing, look, &slot.employment));
        }

        // Total worker posts = residents bound in the population pass + the
        // anonymous backfill scenes just built; guards are appended below.
        let backfill_slots = worker_scenes.len();
        let industrial_job_slots = bound_worker_count + backfill_slots;

        // ---- Guard posts: gates + wall towers ----
        // Gates get 1–2 guards each; each tower has a 10% chance of 2 guards and a
        // 20% chance of 1 (else none). Guards carry their own `guarding` dialogue,
        // watching the approaches. Each guard's appearance is rolled from the
        // guards fixture's `looks` pool (villager professions and/or mobs like
        // pillagers), so a post can mix both.
        use crate::generator::npc::NpcLook;
        let guard_looks = &npc_data.guards.looks;
        let guard_scene = |feet: Point3D, facing: f32, look: NpcLook| -> AnchorScene {
            let mut slot = AnchorSlot::new(feet, facing, SlotRole::Worker);
            slot.look = Some(look);
            slot.dialogue = Some("guarding".to_string());
            AnchorScene::group(SceneKind::Solo, vec![slot])
        };
        let town_centre = {
            let n = urban.len().max(1) as i32;
            urban.iter().fold(Point2D::ZERO, |a, &p| a + p) / n
        };
        // Gates: one guard a couple cells inside the opening; when a gate gets a
        // second, it stands the same distance outside — a guard on each side of
        // the gate. Both face the opening.
        for (gate_point, dir) in editor.world().gate_locations.clone() {
            let base = gate_point.drop_y();
            let fwd: Point2D = dir.into();
            // One cell to each side of the gate centre — in the opening, not the
            // wall a couple cells away.
            let inside = Point2D::new(base.x - fwd.x, base.y - fwd.y);
            let outside = Point2D::new(base.x + fwd.x, base.y + fwd.y);
            let stands: Vec<Point2D> = if worker_rng.percent(50) {
                vec![inside, outside]
            } else {
                vec![inside]
            };
            for s in stands {
                // Stand on the gate's own cleared floor (`gate_point.y`), which is
                // where the gateway punched its air column upward. Re-deriving the
                // height from the ocean-floor heightmap dropped feet below the
                // threshold, burying — and suffocating — the guard in gate blocks.
                let y = gate_point.y;
                let feet = Point3D::new(s.x, y, s.y);
                let facing =
                    crate::generator::population::yaw_toward(feet, Point3D::new(base.x, y, base.y));
                let look = *worker_rng.choose(guard_looks);
                let mut scene = guard_scene(feet, facing, look);
                // Gates often sit on a slabbed threshold (sandstone slabs on a
                // desert gateway, stair-and-slab approach on a stone one). When
                // the block beneath the guard's feet is a slab, lift them half
                // a block so they stand on the slab top rather than sunk in it.
                let underfoot = editor.try_get_block(Point3D::new(s.x, y - 1, s.y));
                if matches!(
                    underfoot.map(|b| crate::minecraft::BlockForm::infer_from_block(&b.id)),
                    Some(crate::minecraft::BlockForm::Slab),
                ) {
                    scene.slots[0].y_offset = 0.5;
                }
                worker_scenes.push(scene);
            }
        }
        // Towers: weighted small chance of 1–2 guards on the walkway beside each.
        for posts in editor.world().tower_guard_posts.clone() {
            let roll = worker_rng.rand_i32(100);
            let n: usize = if roll < 10 { 2 } else if roll < 30 { 1 } else { 0 };
            for feet in posts.into_iter().take(n) {
                let facing = crate::generator::population::yaw_toward(
                    Point3D::new(town_centre.x, feet.y, town_centre.y),
                    feet,
                );
                let look = *worker_rng.choose(guard_looks);
                let mut scene = guard_scene(feet, facing, look);
                scene.slots[0].y_offset = 0.5; // stand on the battlement slab, not sunk in it
                worker_scenes.push(scene);
            }
        }
        let guard_count = worker_scenes.len() - backfill_slots;

        if !worker_scenes.is_empty() {
            // Roster supplies names/dialogue/biome; each scene's slot
            // overrides the profession, so the roll here is incidental.
            let worker_roster = build_roster(
                worker_scenes.len(), 0, culture, npc_data, &mut id_alloc, &mut rng.derive(),
            );
            let budget = worker_scenes.len();
            match populate_npcs(editor, worker_scenes, worker_roster, budget, npc_data, &mut rng).await {
                Ok(staffed) => println!(
                    "Staffed {} fixture NPCs ({} backfill workers + {} guards); {} of {} posts across {} workplaces filled by residents",
                    staffed, backfill_slots, guard_count, bound_worker_count, industrial_job_slots, workplace_count,
                ),
                Err(e) => log::warn!("worker/guard staffing failed: {}", e),
            }
        }

        // ---- Jobs summary ----
        let total_industrial = urban_industrial_count + rural_building_count;
        let approx_jobs =
            industrial_job_slots + guard_count + market_stall_count + performer_slot_count;
        println!("=== JOBS SUMMARY ===");
        println!(
            "Industrial/resource buildings: {} ({} urban + {} rural)",
            total_industrial, urban_industrial_count, rural_building_count,
        );
        println!("Workers: {}", industrial_job_slots);
        println!("Guards: {} (gates + towers)", guard_count);
        println!("Market stalls: {}", market_stall_count);
        println!("Stages: {} ({} performer slots)", stage_count, performer_slot_count);
        println!(
            "Approx jobs available: {} (workers {} + guards {} + vendors {} + performers {})",
            approx_jobs, industrial_job_slots, guard_count, market_stall_count, performer_slot_count,
        );

        // Employment by job: building trades (sorted by count), then the
        // non-building roles (guards, vendors, performers).
        println!("--- Employment by job ---");
        let mut by_job: Vec<(String, usize)> = worker_by_job.into_iter().collect();
        if guard_count > 0 {
            by_job.push((npc_data.guards.employment.clone(), guard_count));
        }
        if market_stall_count > 0 {
            by_job.push((npc_data.vendors.employment.clone(), market_stall_count));
        }
        if performer_slot_count > 0 {
            by_job.push((npc_data.performers.employment.clone(), performer_slot_count));
        }
        by_job.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (job, count) in &by_job {
            println!("  {:<14} {}", job, count);
        }
    }

    // Top-down town map (SVG) for inspection: footprints + named roads coloured
    // by id + the abstract MST/node overlay, with sign posts marked.
    {
        let svg = crate::generator::paths::render_town_map(
            editor.world(), &urban, &road_network.paths, &road_network.road_labels,
            &road_names, &alley_band, Some(&road_network), &signs, &place_labels,
        );
        std::fs::create_dir_all("output").ok();
        match std::fs::write("output/town.svg", &svg) {
            Ok(()) => println!("Wrote town map to output/town.svg"),
            Err(e) => log::warn!("failed to write town map: {e}"),
        }
        match crate::generator::paths::rasterize_to_png(&svg, "output/town.png") {
            Ok(()) => println!("Wrote town map to output/town.png"),
            Err(e) => log::warn!("failed to render town.png: {e}"),
        }
    }

    // Welcome banner: bury a command-block proximity sensor at the town centre
    // that flashes the settlement name when a player crosses into the urban
    // area. The name is derived procedurally from the place's features (iconic
    // building, land shape, biome), seeded off the town seed so it's stable.
    // Requires `enable-command-block=true` on the server.
    {
        let mut name_rng = RNG::new(seed).derive();
        let named = crate::generator::naming::generate_settlement_name(
            editor.world(), &urban, &civic_features, culture, &color_scheme.town, &mut name_rng,
        );
        crate::generator::welcome::place_welcome_title(
            editor, &urban, &named.name, &named.subtitle,
        ).await;
        println!("Placed welcome-title sensor for \"{}\" ({})", named.name, named.subtitle);

        // Chronicle: digest the finished town into a dossier (name, colours,
        // biomes, roads, trades, families, greens, gates) and have the AI write a
        // guidebook, dropped into the player's inventory. Live-only — `give_player
        // _book` posts to the server; failures are non-fatal.
        let mut chronicle_rng = RNG::from_seed_and_string(seed, "district_names");
        // The settlement's economy as lowercased English nouns: harvests are the
        // raw resources the rural parcels gathered (`supply`), produces are the
        // finished goods the chains turned them into. Pretty names come from the
        // resource registry; ids fall back to underscore-stripped form. Sorted +
        // deduped so the chronicle's fact list is stable across runs.
        let reg = &data.resource_registry;
        let pretty = |id: &str| reg.resources().get(id)
            .map(|r| r.name.to_lowercase())
            .unwrap_or_else(|| id.replace('_', " "));
        let mut harvests: Vec<String> = result.supply.keys().map(|id| pretty(id)).collect();
        harvests.sort();
        harvests.dedup();
        let mut produces: Vec<String> = result.finished_goods.iter().map(|(id, _)| pretty(id)).collect();
        produces.sort();
        produces.dedup();
        let dossier = assemble_dossier(
            editor, &urban, culture, &named, &color_scheme.town, &civic_blazon,
            &road_names, &road_network.road_labels, &place_labels, &manor_facts,
            total_buildings, population_count, &harvests, &produces, &mut chronicle_rng,
        );
        if let Err(e) = crate::generator::chronicle::generate_chronicle(&*editor, &dossier).await {
            log::warn!("Chronicle generation failed: {e}");
        }
    }

    // Scatter free-floating ships onto the settlement's water districts, then crew the
    // afloat ones (a captain at the helm + sailors on deck) from the town roster — the same
    // fixture path as plaza vendors / industry workers. Live-only: staffing is a no-op offline.
    let (ships, crew_scenes) =
        crate::generator::ships::fleet::scatter_ships(editor, &data, seed).await;
    println!("Placed {} ships across water districts", ships);
    if !crew_scenes.is_empty() {
        match crate::generator::ships::crew::staff_crew(
            editor, crew_scenes, culture, &data, &mut id_alloc, &mut rng,
        )
        .await
        {
            Ok(staffed) => println!("Crewed ships with {} sailor/captain NPCs", staffed),
            Err(e) => log::warn!("ship crew staffing failed: {e}"),
        }
    }

    editor.flush_buffer().await;
}

/// Scan the claim map for every placed workplace (urban shop or rural gather
/// building) and produce one [`WorkerSlot`](crate::generator::population::WorkerSlot)
/// per crew position: a clear stand cell at the footprint edge facing the
/// building, the building centre for proximity scoring, and the post's skin pool
/// + job label from `staffing_for`. Returns the slots plus the number of distinct
/// workplaces that yielded any. This is the geometry half of worker staffing;
/// who fills each post is decided by
/// [`bind_workers`](crate::generator::population::bind_workers).
fn discover_worker_slots(
    editor: &Editor,
    data: &LoadedData,
) -> (Vec<crate::generator::population::WorkerSlot>, usize) {
    use crate::generator::population::{yaw_toward, WorkerSlot};
    use crate::generator::BuildClaim;
    let npc_data = &data.npc_data;

    // Placed buildings are the only `Structure` claims (wall towers claim
    // `Wall`), and claims persist, so this recovers each building's footprint +
    // type without threading placement state out. Scan the urban area *and*
    // every rural parcel so rural gather buildings get staffed too.
    let mut scan_cells: Vec<Point2D> = editor.world().get_urban_points().into_iter().collect();
    for sd in editor.world().districts.values() {
        if sd.data.parcel_type == ParcelType::Rural {
            scan_cells.extend(sd.data.points_2d.iter().copied());
        }
    }
    let mut footprints: HashMap<u32, (String, Vec<Point2D>)> = HashMap::new();
    for &p in &scan_cells {
        if let Some(BuildClaim::Structure(id)) = editor.world().get_claim(p) {
            footprints
                .entry(id.id)
                .or_insert_with(|| (id.structure_type.0.clone(), Vec::new()))
                .1
                .push(p);
        }
    }

    // A usable stand spot is an in-bounds open cell — not a road, wall, or
    // building. Road-bordering cells sort first so workers read as street-side.
    let is_clear = |c: Point2D| {
        matches!(
            editor.world().get_claim(c),
            Some(BuildClaim::None) | Some(BuildClaim::Nature)
        )
    };
    let road_side = |c: Point2D| {
        crate::geometry::CARDINALS_2D
            .iter()
            .any(|&d| matches!(editor.world().get_claim(c + d), Some(BuildClaim::Path(_))))
    };

    // Deterministic order over buildings (HashMap iteration isn't stable).
    let mut ids: Vec<u32> = footprints.keys().copied().collect();
    ids.sort_unstable();

    let mut slots: Vec<WorkerSlot> = Vec::new();
    let mut workplaces = 0usize;
    for id in ids {
        let (kind, cells) = &footprints[&id];
        let cell_set: HashSet<Point2D> = cells.iter().copied().collect();
        let centroid =
            cells.iter().fold(Point2D::ZERO, |a, p| a + *p) / cells.len().max(1) as i32;

        // Staffing (skin pool + job label) comes from the building's own structure
        // JSON, falling back to the town-wide default.
        let staffing = npc_data.staffing_for(kind, &data.structures);

        // Hand-authored interior anchors win when present: stand the crew at the
        // exact spots the building declared (already in world coords + yaw), and
        // skip the outside-stand discovery entirely for this building.
        if let Some(posts) = editor.world().structure_anchors.get(&id) {
            if !posts.is_empty() {
                workplaces += 1;
                for &(stand, facing) in posts {
                    slots.push(WorkerSlot {
                        stand,
                        facing,
                        workplace: centroid,
                        looks: staffing.looks.clone(),
                        employment: staffing.employment.clone(),
                    });
                }
                continue;
            }
        }

        let mut candidates: Vec<Point2D> = Vec::new();
        let mut seen: HashSet<Point2D> = HashSet::new();
        for &fc in cells {
            for d in crate::geometry::CARDINALS_2D {
                let c = fc + d;
                if cell_set.contains(&c) || !editor.world().is_in_bounds_2d(c) || !is_clear(c) {
                    continue;
                }
                if seen.insert(c) {
                    candidates.push(c);
                }
            }
        }
        // Road-side cells first (so the seed faces the street), then deterministic.
        candidates.sort_unstable_by_key(|c| (!road_side(*c), c.x, c.y));

        let want = staffing.workers.min(candidates.len());
        if want == 0 {
            log::warn!("no clear stand cell for building '{}' (id {})", kind, id);
            continue;
        }
        workplaces += 1;

        // Spread the crew around the building rather than bunching them at the
        // door: seed from the road-side cell, then greedily add the candidate
        // farthest from everyone already chosen (farthest-point sampling). With a
        // small crew this lands one worker per side, all facing the building.
        let mut chosen: Vec<Point2D> = vec![candidates[0]];
        while chosen.len() < want {
            let Some(&next) = candidates
                .iter()
                .filter(|c| !chosen.contains(c))
                .max_by_key(|c| {
                    chosen
                        .iter()
                        .map(|ch| (ch.x - c.x).pow(2) + (ch.y - c.y).pow(2))
                        .min()
                        .unwrap_or(0)
                })
            else {
                break;
            };
            chosen.push(next);
        }

        for stand in chosen {
            // Stand on the ground at the cell; face the footprint centroid.
            let Some(y) = editor.world().get_ocean_floor_height_at(stand) else { continue; };
            let stand3 = Point3D::new(stand.x, y, stand.y);
            let centre3 = Point3D::new(centroid.x, y, centroid.y);
            let facing = yaw_toward(stand3, centre3);
            slots.push(WorkerSlot {
                stand: stand3,
                facing,
                workplace: centroid,
                looks: staffing.looks.clone(),
                employment: staffing.employment.clone(),
            });
        }
    }
    (slots, workplaces)
}
