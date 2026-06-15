#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::env;

    use crate::minecraft::{Biome, Block};
    use crate::noise::RNG;

    use super::super::registry::{load_yaml, ResourceRegistry};
    use super::super::types::{BiomeResourcesFile, RecipesFile, ResourcesFile};

    fn make_registry() -> ResourceRegistry {
        let base = env::current_dir().unwrap().join("data").join("resource_chains");
        let resources: ResourcesFile = load_yaml(base.join("resources.yaml")).unwrap();
        let recipes: RecipesFile = load_yaml(base.join("recipes.yaml")).unwrap();
        let biome_resources: BiomeResourcesFile = load_yaml(base.join("biome_resources.yaml")).unwrap();
        ResourceRegistry::from_parts(
            resources.resources,
            recipes.recipes,
            biome_resources.biome_resources,
        )
        .expect("Registry should load without errors")
    }

    #[test]
    fn registry_loads_without_errors() {
        make_registry();
    }

    #[test]
    fn forest_resolves_wood_chains() {
        let registry = make_registry();
        let available: HashSet<String> = ["wood", "feathers"]
            .iter().map(|s| s.to_string()).collect();

        let chains = registry.resolve(&available);

        assert!(chains.producible.contains("planks"), "should produce planks");
        assert!(chains.producible.contains("charcoal"), "should produce charcoal");
        assert!(chains.producible.contains("furniture"), "should produce furniture");

        assert!(!chains.producible.contains("iron_ingot"), "no iron ore available");
        assert!(!chains.producible.contains("arrows"), "no flint available");
    }

    #[test]
    fn forest_has_near_miss_for_arrows() {
        let registry = make_registry();
        let available: HashSet<String> = ["wood", "feathers"]
            .iter().map(|s| s.to_string()).collect();

        let chains = registry.resolve(&available);

        let arrow_miss = chains.nearly_unlocked.iter()
            .find(|nm| nm.recipe_id == "flint_feather_to_arrows");
        assert!(arrow_miss.is_some(), "arrows should be a near-miss (only missing flint)");
        assert_eq!(arrow_miss.unwrap().missing, vec!["flint"]);
    }

    #[test]
    fn forest_plus_mountains_unlocks_tools_chain() {
        let registry = make_registry();
        let available: HashSet<String> = ["wood", "feathers", "iron_ore", "flint"]
            .iter().map(|s| s.to_string()).collect();

        let chains = registry.resolve(&available);

        assert!(chains.producible.contains("iron_ingot"), "should smelt iron");
        assert!(chains.producible.contains("tools"), "should craft tools");
        assert!(chains.producible.contains("arrows"), "flint + feathers = arrows");
    }

    /// Regression for the broken-iron-chain bug (run 20260614_111106): iron_ore and coal
    /// were gathered but no wood, yet the chain rigidly took the alphabetically-first
    /// `charcoal` smelting recipe (which needs wood), so the chain's budget collapsed to
    /// zero and all the iron + coal were wasted. Chain construction is now availability
    /// aware: with no wood, smelting must route through the `coal` recipes.
    #[test]
    fn iron_chain_routes_through_coal_when_no_wood() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["iron_ore", "coal"].iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);
        let tools = plan.chains.iter().find(|c| c.finished_good == "tools")
            .expect("tools should be producible from iron_ore + coal");

        assert!(tools.recipe_ids.iter().any(|r| r == "iron_ore_coal_to_ingot"),
            "smelting should use the coal recipe, got {:?}", tools.recipe_ids);
        assert!(!tools.recipe_ids.iter().any(|r| r.contains("charcoal")),
            "must not use the charcoal path when wood is absent, got {:?}", tools.recipe_ids);
        // The cost the executor budgets against must be expressed in the fuel actually used.
        assert!(tools.raw_cost.get("coal").copied().unwrap_or(0.0) > 0.0,
            "tools raw_cost should include coal, got {:?}", tools.raw_cost);
        assert!(!tools.raw_cost.contains_key("wood"),
            "tools raw_cost should not reference wood, got {:?}", tools.raw_cost);
    }

    /// Companion to the above: when wood *is* available the charcoal smelting path is
    /// still preferred (it sorts first), so this change doesn't disturb the common case.
    #[test]
    fn iron_chain_prefers_charcoal_when_wood_present() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["iron_ore", "wood"].iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);
        let tools = plan.chains.iter().find(|c| c.finished_good == "tools")
            .expect("tools should be producible from iron_ore + wood");

        assert!(tools.recipe_ids.iter().any(|r| r == "iron_ore_charcoal_to_ingot"),
            "with wood present, smelting should use the charcoal recipe, got {:?}", tools.recipe_ids);
    }

    #[test]
    fn raw_cost_furniture() {
        let registry = make_registry();
        let cost = registry.raw_cost("furniture").expect("furniture should have a raw cost");
        let wood_cost = cost.get("wood").copied().unwrap_or(0.0);
        assert!((wood_cost - 1.0).abs() < 0.001, "furniture should cost 1 wood, got {}", wood_cost);
    }

    #[test]
    fn raw_cost_arrows() {
        let registry = make_registry();
        let cost = registry.raw_cost("arrows").expect("arrows should have a raw cost");
        let flint = cost.get("flint").copied().unwrap_or(0.0);
        let feathers = cost.get("feathers").copied().unwrap_or(0.0);
        assert!((flint - 0.125).abs() < 0.001, "expected 0.125 flint per arrow, got {}", flint);
        assert!((feathers - 0.25).abs() < 0.001, "expected 0.25 feathers per arrow, got {}", feathers);
    }

    #[test]
    fn raw_cost_tools() {
        let registry = make_registry();
        // iron_ingot_to_tools: iron_ingot(1) → tools(2), so 0.5 ingot per tool
        // iron_ore_charcoal_to_ingot: iron_ore(2) + charcoal(1) → ingot(1)
        // per tool: 0.5 * (2 iron_ore + 1 charcoal) = 1 iron_ore + 0.5 fuel
        let cost = registry.raw_cost("tools").expect("tools should have a raw cost");
        let iron = cost.get("iron_ore").copied().unwrap_or(0.0);
        let fuel = cost.get("wood").copied().unwrap_or(0.0)
                 + cost.get("coal").copied().unwrap_or(0.0);
        assert!((iron - 1.0).abs() < 0.001, "expected 1.0 iron_ore per tool, got {}", iron);
        assert!((fuel - 0.5).abs() < 0.001, "expected 0.5 total fuel per tool, got {}", fuel);
    }

    #[test]
    fn biome_forest_provides_wood_and_feathers() {
        let registry = make_registry();
        let biome = Biome::from("minecraft:forest");
        let resources = registry.resources_for_biome(&biome);
        assert!(resources.contains("wood"));
        assert!(resources.contains("feathers"));
    }

    #[test]
    fn unknown_biome_returns_empty_set() {
        let registry = make_registry();
        let biome = Biome::from("minecraft:void");
        let resources = registry.resources_for_biome(&biome);
        assert!(resources.is_empty());
    }

    #[test]
    fn select_production_ranks_deeper_chains_first() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        // tools: depth 4 (wood_to_charcoal, iron_ore_to_ingot, ingot_charcoal_to_steel, steel_to_tools)
        // furniture: depth 2 (wood_to_planks, planks_to_furniture)
        // arrows: depth 1 (flint_feather_to_arrows)
        let available: HashSet<String> = ["wood", "feathers", "iron_ore", "flint"]
            .iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);

        let goods: Vec<&str> = plan.chains.iter().map(|c| c.finished_good.as_str()).collect();
        let tools_pos = goods.iter().position(|&g| g == "tools").expect("tools should be in plan");
        let furniture_pos = goods.iter().position(|&g| g == "furniture").expect("furniture should be in plan");
        let arrows_pos = goods.iter().position(|&g| g == "arrows").expect("arrows should be in plan");

        // tools: depth 5 (gather_wood, gather_iron_ore, wood_to_charcoal, iron_ore_charcoal_to_ingot, iron_ingot_to_tools)
        // arrows: depth 3, 2 raw inputs (gather_flint, gather_feathers, flint_feather_to_arrows)
        // furniture: depth 3, 1 raw input (gather_wood, wood_to_planks, planks_to_furniture)
        assert!(tools_pos < arrows_pos, "tools (depth 5) should rank above arrows (depth 3)");
        assert!(arrows_pos < furniture_pos, "arrows (depth 3, 2 raw inputs) should rank above furniture (depth 3, 1 raw input)");
    }

    #[test]
    fn select_production_buildings_are_complete() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["wood"].iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);

        // Furniture chain requires sawmill (planks) and carpenter (furniture)
        assert!(plan.building_run_cost.contains_key("sawmill"), "sawmill needed for planks");
        assert!(plan.building_run_cost.contains_key("carpenter"), "carpenter needed for furniture");
        // Charcoal chain requires charcoal_burner — but charcoal is tier 1, not tier 2,
        // so it only appears if it feeds a tier-2 chain. With only wood, no tier-2 chain
        // needs charcoal, so charcoal_burner should NOT appear.
        assert!(!plan.building_run_cost.contains_key("charcoal_burner"), "charcoal_burner not needed without a charcoal consumer");
    }

    #[test]
    fn select_production_chain_depth_matches_recipe_count() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["wood", "iron_ore"]
            .iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);

        let tools_chain = plan.chains.iter().find(|c| c.finished_good == "tools").expect("tools should be in plan");
        // Recipes: gather_wood, gather_iron_ore, wood_to_charcoal, iron_ore_charcoal_to_ingot, iron_ingot_to_tools
        assert_eq!(tools_chain.depth, 5, "tools chain should have 5 recipes (including gather steps)");
        assert_eq!(tools_chain.raw_inputs.len(), 2, "tools chain uses wood and iron_ore");
    }

    /// Generates a standalone HTML file containing an interactive Mermaid graph of all
    /// production chains and opens it in the default browser.
    ///
    /// Run with:
    ///   cargo test generate_production_graph -- --ignored --nocapture
    #[test]
    #[ignore]
    fn generate_production_graph() {
        use std::fs;
        use std::io::Write;

        let registry = make_registry();
        let elements_json = registry.to_cytoscape_elements();

        let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Production Chains</title>
  <script src="https://cdn.jsdelivr.net/npm/cytoscape/dist/cytoscape.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/dagre/dist/dagre.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/cytoscape-dagre/cytoscape-dagre.js"></script>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ background: #1a1a2e; font-family: sans-serif; display: flex; flex-direction: column; height: 100vh; }}
    #header {{ text-align: center; padding: 0.5rem 1rem 0.4rem; flex-shrink: 0; }}
    h1 {{ color: #eee; font-size: 1.3rem; margin-bottom: 0.2rem; }}
    p  {{ color: #aaa; font-size: 0.8rem; }}
    #cy {{ flex: 1; background: #f8f8f8; }}
  </style>
</head>
<body>
  <div id="header">
    <h1>Production Chains</h1>
    <p>Green = raw &nbsp;·&nbsp; Yellow = intermediate &nbsp;·&nbsp; Blue = finished &nbsp;·&nbsp; Diamond = building &nbsp;·&nbsp; Edge labels = quantities</p>
  </div>
  <div id="cy"></div>
  <script>
    var cy = cytoscape({{
      container: document.getElementById('cy'),
      elements: {elements_json},
      style: [
        {{
          selector: 'node[type = "raw"]',
          style: {{
            shape: 'ellipse', width: 90, height: 90,
            'background-color': '#90EE90', 'border-color': '#2d862d', 'border-width': 2,
            label: 'data(label)', 'text-valign': 'center', 'text-halign': 'center',
            'font-size': 15, 'font-weight': 'bold', color: '#1a4a1a',
            'text-wrap': 'wrap', 'text-max-width': 78,
          }}
        }},
        {{
          selector: 'node[type = "intermediate"]',
          style: {{
            shape: 'round-rectangle', width: 115, height: 55,
            'background-color': '#FFD700', 'border-color': '#b8860b', 'border-width': 2,
            label: 'data(label)', 'text-valign': 'center', 'text-halign': 'center',
            'font-size': 15, 'font-weight': 'bold', color: '#3d2a00',
            'text-wrap': 'wrap', 'text-max-width': 105,
          }}
        }},
        {{
          selector: 'node[type = "finished"]',
          style: {{
            shape: 'ellipse', width: 100, height: 100,
            'background-color': '#87CEEB', 'border-color': '#00008b', 'border-width': 2,
            label: 'data(label)', 'text-valign': 'center', 'text-halign': 'center',
            'font-size': 15, 'font-weight': 'bold', color: '#00005a',
            'text-wrap': 'wrap', 'text-max-width': 88,
          }}
        }},
        {{
          selector: 'node[type = "recipe"]',
          style: {{
            shape: 'diamond', width: 95, height: 95,
            'background-color': '#f0f0f0', 'border-color': '#888', 'border-width': 1.5,
            label: 'data(label)', 'text-valign': 'center', 'text-halign': 'center',
            'font-size': 13, color: '#333',
            'text-wrap': 'wrap', 'text-max-width': 65,
          }}
        }},
        {{
          selector: 'edge',
          style: {{
            width: 1.5, 'line-color': '#999',
            'target-arrow-color': '#999', 'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            label: 'data(label)', 'font-size': 12, color: '#555',
            'text-background-color': '#f8f8f8', 'text-background-opacity': 1,
            'text-background-padding': '2px',
          }}
        }}
      ],
      layout: {{
        name: 'dagre',
        rankDir: 'TB',
        ranker: 'network-simplex',
        rankSep: 90,
        nodeSep: 45,
        edgeSep: 15,
        padding: 40,
        animate: false,
      }}
    }});
  </script>
</body>
</html>"#, elements_json = elements_json);

        let out_path = env::current_dir().unwrap().join("target").join("production_chains.html");
        let mut file = fs::File::create(&out_path).expect("could not create output file");
        file.write_all(html.as_bytes()).expect("could not write output file");

        println!("\nGraph written to: {}", out_path.display());

        // Open in default browser (best-effort)
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd").args(["/C", "start", out_path.to_str().unwrap()]).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&out_path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&out_path).spawn();
    }

    #[test]
    fn assign_parcel_resources_respects_constraints() {
        let registry = make_registry();
        let mut rng = RNG::new(42);

        // Parcel 0: only wheat (most constrained — 1 option)
        // Parcel 1: iron_ore or coal
        // Parcel 2: wood or feathers
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["wheat".into()]),
            (1, vec!["iron_ore".into(), "coal".into()]),
            (2, vec!["wood".into(), "feathers".into()]),
        ].into();

        let assignments = registry.assign_parcel_resources(&options, &mut rng);

        assert_eq!(assignments.len(), 3, "all parcels should be assigned");

        // Sole-option parcel always gets wheat with the farm building
        let a0 = assignments.get(&0).unwrap();
        assert_eq!(a0.primary_resource, "wheat");
        assert_eq!(a0.building, "farm");

        // All buildings must be non-empty
        for a in assignments.values() {
            assert!(!a.building.is_empty());
        }
    }

    #[test]
    fn assign_parcel_resources_prefers_diversity() {
        let registry = make_registry();
        let mut rng = RNG::new(0);

        // Two parcels that both could produce wood or iron_ore; they should pick different ones.
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["wood".into(), "iron_ore".into()]),
            (1, vec!["wood".into(), "iron_ore".into()]),
        ].into();

        let assignments = registry.assign_parcel_resources(&options, &mut rng);

        assert_eq!(assignments.len(), 2);
        let r0 = &assignments[&0].primary_resource;
        let r1 = &assignments[&1].primary_resource;
        assert_ne!(r0, r1, "parcels should pick different resources when possible");
    }

    #[test]
    fn assign_parcel_resources_skips_unknown_gather_resources() {
        let registry = make_registry();
        let mut rng = RNG::new(42);

        // gold_ore appears in biome_resources.yaml but has no gather recipe
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["gold_ore".into()]),
            (1, vec!["wood".into()]),
        ].into();

        let assignments = registry.assign_parcel_resources(&options, &mut rng);

        // Parcel 0 has no valid candidates and is skipped
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[&1].primary_resource, "wood");
    }

    /// Regression guard for the resource-chain cap logic: a settlement of all-plains
    /// rural parcels must NOT collapse to a single resource. Plains offers wheat, wool
    /// and cow; the cap loop used to ban over-supplied resources outright, which
    /// cascaded until only wool survived. Now caps limit per-resource parcel *counts*
    /// (never below 1), so every biome-supported resource keeps at least one parcel.
    #[test]
    fn plains_parcels_stay_diverse() {
        use crate::generator::districts::DistrictID;
        use crate::minecraft::Biome;

        let registry = make_registry();

        // Try several seeds so we don't rely on a single lucky RNG stream.
        for seed in [12345, 1, 7, 99, 2024] {
            let mut rng = RNG::new(seed);
            let parcels: HashMap<DistrictID, crate::generator::districts::ParcelAnalysis> = (0..12)
                .map(|i| (
                    DistrictID(i),
                    crate::generator::districts::ParcelAnalysis::from_biome_count(
                        [(Biome::from("minecraft:plains"), 100)].into(),
                    ),
                ))
                .collect();

            let result = registry.resolve_for_parcels(&parcels, &mut rng);

            let mut counts: HashMap<String, u32> = HashMap::new();
            for a in result.parcel_assignments.values() {
                *counts.entry(a.primary_resource.clone()).or_insert(0) += 1;
            }

            assert_eq!(result.parcel_assignments.len(), 12, "all parcels assigned (seed {})", seed);
            // All three plains resources should be represented, each at least once.
            for r in ["wheat", "wool", "cow"] {
                assert!(counts.get(r).copied().unwrap_or(0) >= 1,
                    "expected at least one '{}' parcel on plains, got {:?} (seed {})", r, counts, seed);
            }
        }
    }

    /// Wool color used to paint a district for a given raw resource. Each raw
    /// resource gets a stable, distinct color so adjacent districts read clearly.
    /// Unmapped resources fall back to a deterministic color by name length.
    fn colour_id_for_resource(resource: &str) -> &'static str {
        match resource {
            "wood" => "brown_wool",
            "wheat" => "yellow_wool",
            "iron_ore" => "light_gray_wool",
            "coal" => "black_wool",
            "honey" => "orange_wool",
            "beeswax" => "white_wool",
            "wool" => "pink_wool",
            "sugar_cane" => "lime_wool",
            "cow" => "magenta_wool",
            other => {
                // Deterministic fallback for any resource not explicitly mapped.
                const FALLBACK: [&str; 4] = ["cyan_wool", "purple_wool", "green_wool", "light_blue_wool"];
                FALLBACK[other.len() % FALLBACK.len()]
            }
        }
    }

    /// Wool block painted on a district for a given raw resource.
    fn block_for_resource(resource: &str) -> Block {
        Block { id: colour_id_for_resource(resource).into(), data: None, state: None }
    }

    /// End-to-end visual test (requires a live Minecraft server).
    ///
    /// Runs districting (`generate_parcels`), computes the resource chain over the
    /// rural districts (`resolve_for_parcels`), then paints every district:
    ///   - Rural    → a wool color keyed to its assigned raw resource
    ///   - Urban    → blue wool
    ///   - OffLimits → red wool
    /// District edges are capped with glass so boundaries stay legible.
    ///
    /// Run with:
    ///   cargo test colour_districts_by_resource -- --nocapture
    #[tokio::test]
    async fn colour_districts_by_resource() {
        use crate::editor::World;
        use crate::generator::districts::{generate_parcels, ParcelType};
        use crate::generator::resource_chain::ProductionPainter;
        use crate::generator::terrain::{feathered_flatten, flatten_urban_area};
        use crate::geometry::{Point2D, Point3D};
        use crate::http_mod::GDMCHTTPProvider;
        use crate::noise::Seed;
        use crate::util::init_logger;

        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        // ── Resource chain over rural districts ──────────────────────────────
        // Only rural districts produce raw resources, so feed just those into
        // the resolver (matching `parcel_resource_production_report`).
        let registry = ResourceRegistry::load().expect("Failed to load resource registry");
        let rural_analysis: HashMap<_, _> = editor.world().district_analysis_data.iter()
            .filter(|(id, _)| {
                editor.world().districts.get(id)
                    .map(|d| d.data.parcel_type == ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();

        let result = registry.resolve_for_parcels(&rural_analysis, &mut rng);

        // ── Terraforming (no painters applied) ───────────────────────────────
        // Reproduce only the earthworks the real pipeline performs before any
        // blocks are painted: grade the urban interior, and smooth each rural
        // production area per its assigned painter's flatten_strength. The
        // painters themselves (palettes, irrigation, claims) are intentionally
        // skipped — we just want to see the terraformed surface.

        // Urban grading — same feather / iteration count as the road + placement
        // pipeline (see placement/test.rs, districts/test.rs).
        let urban = editor.world().get_urban_points();
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        // Rural production-area smoothing. Mirrors paint_production_area's feathered
        // smoothing: flatten the field interior + border ring, reaching a couple
        // blocks into neighbouring land, and grade back to natural at the outer
        // edge. No building is placed here, so there are no claims to exclude.
        const EDGE_BUFFER: i32 = 3;
        const NEIGHBOUR_REACH: i32 = 2;
        for (id, assignment) in &result.parcel_assignments {
            let Some(painter_name) = &assignment.production_painter else { continue };
            let flatten_strength = match registry.production_painters.get(painter_name) {
                Some(ProductionPainter::Palettes { flatten_strength, .. }) => *flatten_strength,
                _ => continue,
            };
            let smooth_iters = (flatten_strength.clamp(0.0, 1.0) * 5.0).round() as usize;
            if smooth_iters == 0 {
                continue;
            }

            // Snapshot the geometry so the editor can be borrowed mutably below.
            let Some((edges, points)) = editor.world().districts.get(id)
                .map(|d| (d.data.edges.clone(), d.data.points_2d.clone()))
            else {
                continue;
            };

            let edge_buffer: HashSet<Point2D> = edges.iter()
                .flat_map(|p| {
                    let p2 = p.drop_y();
                    (-EDGE_BUFFER..=EDGE_BUFFER).flat_map(move |dx| {
                        (-EDGE_BUFFER..=EDGE_BUFFER).map(move |dz| Point2D::new(p2.x + dx, p2.y + dz))
                    })
                })
                .collect();

            // The district's own non-water cells (interior + border ring).
            let own_cells: HashSet<Point2D> = points.iter()
                .filter(|&&p| !editor.world().is_water(p))
                .copied()
                .collect();
            // Skip parcels with no interior beyond the edge buffer (matches the
            // painter's `free_cells.is_empty()` early-out).
            if !own_cells.iter().any(|p| !edge_buffer.contains(p)) {
                continue;
            }

            // Reach a couple blocks into neighbouring land for a feathered transition.
            let mut region = own_cells.clone();
            for &p in &own_cells {
                for dx in -NEIGHBOUR_REACH..=NEIGHBOUR_REACH {
                    for dz in -NEIGHBOUR_REACH..=NEIGHBOUR_REACH {
                        let q = Point2D::new(p.x + dx, p.y + dz);
                        if own_cells.contains(&q) {
                            continue;
                        }
                        if editor.world().is_in_bounds_2d(q) && !editor.world().is_water(q) {
                            region.insert(q);
                        }
                    }
                }
            }

            feathered_flatten(&mut editor, &region, EDGE_BUFFER + NEIGHBOUR_REACH, smooth_iters, true).await;
        }

        // Snapshot each district's type and (for rural) its assigned resource so we
        // don't hold a borrow on the world while painting through the editor.
        let district_color: HashMap<_, _> = editor.world().districts.iter()
            .map(|(id, d)| {
                let block = match d.data.parcel_type {
                    ParcelType::Urban => Block { id: "blue_wool".into(), data: None, state: None },
                    ParcelType::OffLimits => Block { id: "red_wool".into(), data: None, state: None },
                    ParcelType::Rural => match result.parcel_assignments.get(id) {
                        Some(a) => block_for_resource(&a.primary_resource),
                        // Rural district with no assignable resource — fall back to gray.
                        None => Block { id: "gray_wool".into(), data: None, state: None },
                    },
                    ParcelType::Unknown => Block { id: "bedrock".into(), data: None, state: None },
                };
                (*id, block)
            })
            .collect();

        let glass = Block { id: "glass".into(), data: None, state: None };

        // Edge cells flattened by 2D coordinate — terraforming may have moved an
        // edge cell's Y, so a 3D `edges.contains` check would miss it. Color comes
        // from `district_map`; this set only decides whether to cap with glass.
        let edge_cells: HashSet<Point2D> = editor.world().districts.values()
            .flat_map(|d| d.data.edges.iter().map(|e| e.drop_y()))
            .collect();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let Some(district_id) = editor.world().district_map[x as usize][z as usize] else {
                    continue;
                };

                let block = district_color.get(&district_id).expect("district color");
                // Post-terraform surface height (local, matching block-write coords).
                let height = editor.world().get_height_at(Point2D::new(x, z));

                let on_edge = edge_cells.contains(&Point2D::new(x, z));

                if on_edge {
                    editor.place_block(&glass, Point3D::new(x, height, z)).await;
                    editor.place_block(block, Point3D::new(x, height - 1, z)).await;
                } else {
                    editor.place_block(block, Point3D::new(x, height, z)).await;
                }
            }
        }

        editor.flush_buffer().await;

        // ── Legend ───────────────────────────────────────────────────────────
        println!("\n╔══ District Color Legend ════════════════════════╗");
        println!("║ Urban     → blue_wool");
        println!("║ OffLimits → red_wool");
        println!("║ Rural     → keyed by raw resource:");
        let mut assigned: Vec<(&String, &'static str)> = result.parcel_assignments.values()
            .map(|a| (&a.primary_resource, colour_id_for_resource(&a.primary_resource)))
            .collect();
        assigned.sort();
        assigned.dedup();
        for (resource, color) in &assigned {
            println!("║   {:<12} → {}", resource, color);
        }
        println!("╚═════════════════════════════════════════════════╝\n");
    }

    /// End-to-end visual test (requires a live Minecraft server).
    ///
    /// Builds the rural production areas for real: city terraforming + wall, then
    /// per-rural-parcel building placement and `paint_production_area` (the actual
    /// painters — crops, logged clearings, etc.). Urban cells are overlaid with
    /// blue wool and OffLimits with red so the city shell and reserved land read
    /// clearly, while rural districts keep their painted production areas visible.
    /// No urban buildings and no roads are placed.
    ///
    /// Run with:
    ///   cargo test colour_districts_with_production_painters -- --nocapture
    #[tokio::test]
    async fn colour_districts_with_production_painters() {
        use crate::editor::World;
        use crate::generator::data::LoadedData;
        use crate::generator::districts::{build_wall, generate_parcels, ParcelType, WallType};
        use crate::generator::materials::{MaterialId, Placer};
        use crate::generator::nbts::StructureType;
        use crate::generator::placement::place_rural_building;
        use crate::generator::resource_chain::paint_production_area;
        use crate::generator::terrain::flatten_urban_area;
        use crate::geometry::{Point2D, Point3D};
        use crate::http_mod::GDMCHTTPProvider;
        use crate::noise::Seed;
        use crate::util::init_logger;

        // ── Test tunables ────────────────────────────────────────────────────
        // When true, every rural parcel is forced to use OVERRIDE_BUILDING (and
        // its gather recipe's painter) instead of its resource-chain assignment.
        // Handy for eyeballing one painter across the whole map — pair with a
        // superflat world to test a painter on perfect terrain.
        const OVERRIDE_ALL_RURAL: bool = true;
        // Gather building forced when OVERRIDE_ALL_RURAL is set. Must be a building
        // declared by a gather recipe (inputs: {}) in recipes.yaml, e.g.
        // "farm" (wheat_fields), "woodcutter_hut" (logging_area),
        // "shepherds_hut" (sheep_pasture), "ranch" (cattle_ranch),
        // "sugar_plantation" (sugar_cane_fields), "iron_mine"/"coal_mine" (mine_terrain).
        const OVERRIDE_BUILDING: &str = "iron_mine";

        init_logger();

        let seed = Seed(12345);
        let mut rng = RNG::new(seed);

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");

        let world = World::new(&provider).await.expect("Failed to create world");
        let mut editor = world.get_editor();

        generate_parcels(seed, &mut editor).await;

        let data = LoadedData::load().expect("Failed to load generator data");

        // ── Resource chain over rural districts ──────────────────────────────
        let rural_analysis: HashMap<_, _> = editor.world().district_analysis_data.iter()
            .filter(|(id, _)| {
                editor.world().districts.get(id)
                    .map(|d| d.data.parcel_type == ParcelType::Rural)
                    .unwrap_or(false)
            })
            .map(|(id, analysis)| (*id, analysis.clone()))
            .collect();
        let result = data.resource_registry.resolve_for_parcels(&rural_analysis, &mut rng);

        // ── City terraforming ────────────────────────────────────────────────
        let urban = editor.world().get_urban_points();
        flatten_urban_area(&mut editor, &urban, 16, 12, true).await;

        // ── City wall + gates ────────────────────────────────────────────────
        // Built before placement so wall cells are claimed first (otherwise a
        // rural building could be sited where the wall later goes).
        let material = MaterialId::new("stone_bricks".to_string());
        let mut wall_rng = rng.derive();
        let mut placer_rng = rng.derive();
        let mut placer = Placer::new(&data.materials, &mut placer_rng);
        build_wall(
            &urban,
            &mut editor,
            &mut wall_rng,
            &mut placer,
            &material,
            &data.structures,
            WallType::StandardWithInner,
        )
        .await;

        // ── Rural buildings + production painters (the real painters) ────────
        // When OVERRIDE_ALL_RURAL is set, resolve the forced building's painter
        // from its gather recipe (inputs: {}) so every parcel uses the same method.
        let override_painter: Option<String> = if OVERRIDE_ALL_RURAL {
            data.resource_registry.recipes().values()
                .find(|r| r.inputs.is_empty() && r.building == OVERRIDE_BUILDING)
                .and_then(|r| r.production_painter.clone())
        } else {
            None
        };

        let mut sd_ids: Vec<_> = result.parcel_assignments.keys().cloned().collect();
        sd_ids.sort_by_key(|id| id.0);
        let mut placed = 0usize;
        for sd_id in &sd_ids {
            let assignment = &result.parcel_assignments[sd_id];
            let (building, painter) = if OVERRIDE_ALL_RURAL {
                (OVERRIDE_BUILDING.to_string(), override_painter.clone())
            } else {
                (assignment.building.clone(), assignment.production_painter.clone())
            };
            let Some(district) = editor.world().districts.get(sd_id).cloned() else { continue };
            let structure_type = StructureType(building.clone());
            let Some(structure) = data.structures.get(&structure_type).cloned() else {
                log::warn!("No structure for building '{}' (parcel {:?})", building, sd_id);
                continue;
            };
            match place_rural_building(&district, &structure, &mut rng, &mut editor, &data).await {
                Ok(()) => {
                    placed += 1;
                    if let Some(painter) = &painter {
                        paint_production_area(&district, painter, &assignment.primary_resource, &data, &mut editor, &mut rng).await;
                    }
                }
                Err(e) => log::warn!("Rural placement failed for '{}': {}", building, e),
            }
        }
        log::info!("Placed {} of {} rural buildings", placed, sd_ids.len());

        // ── Overlay wool on urban (blue) and off-limits (red) only ───────────
        // Rural cells are left as painted so the production areas stay visible.
        // Skip claimed cells (wall, building footprints) so we don't wool over them.
        let blue = Block { id: "blue_wool".into(), data: None, state: None };
        let red = Block { id: "red_wool".into(), data: None, state: None };
        let glass = Block { id: "glass".into(), data: None, state: None };

        // Edge cells (2D) of every district — capped with glass so all borders
        // (rural included) are identifiable, even where rural fields are left
        // as-painted rather than wooled.
        let edge_cells: HashSet<Point2D> = editor.world().districts.values()
            .flat_map(|d| d.data.edges.iter().map(|e| e.drop_y()))
            .collect();

        for x in 0..build_area.size.x {
            for z in 0..build_area.size.z {
                let p = Point2D::new(x, z);
                let Some(district_id) = editor.world().district_map[x as usize][z as usize] else {
                    continue;
                };
                let ptype = {
                    let Some(district) = editor.world().districts.get(&district_id) else { continue };
                    district.data.parcel_type
                };
                // Don't paint over the wall or building footprints.
                if editor.world().is_claimed(p) {
                    continue;
                }
                let on_edge = edge_cells.contains(&p);
                let height = editor.world().get_height_at(p);
                match ptype {
                    ParcelType::Urban | ParcelType::OffLimits => {
                        let block = if matches!(ptype, ParcelType::Urban) { &blue } else { &red };
                        if on_edge {
                            editor.place_block(&glass, Point3D::new(x, height, z)).await;
                            editor.place_block(block, Point3D::new(x, height - 1, z)).await;
                        } else {
                            editor.place_block(block, Point3D::new(x, height, z)).await;
                        }
                    }
                    // Rural / Unknown: keep the painted surface, but cap the border
                    // with glass so district boundaries stay visible.
                    _ => {
                        if on_edge {
                            editor.place_block(&glass, Point3D::new(x, height, z)).await;
                        }
                    }
                }
            }
        }

        editor.flush_buffer().await;

        println!("\n╔══ District Overlay ═════════════════════════════╗");
        println!("║ Urban     → blue_wool (wall left as built)");
        println!("║ OffLimits → red_wool");
        println!("║ Rural     → real production painters + buildings");
        println!("║ Borders   → glass (all district edges)");
        println!("╚═════════════════════════════════════════════════╝\n");
    }

    /// Diagnostic test — passes a `HashMap<DistrictID, ParcelAnalysis>` to
    /// `resolve_for_parcels` and prints the full settlement production report.
    ///
    /// Edit the `parcel_analysis` map below and run:
    ///   cargo test parcel_production_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn parcel_production_report() {
        use crate::generator::districts::{DistrictID, ParcelAnalysis};
        use crate::minecraft::Biome;

        let registry = make_registry();
        let mut rng = RNG::new(42);

        // ====== Edit parcels here ======
        // `from_biome_count` takes biome → block count; biomes at ≥30% count as major.
        let parcel_analysis: HashMap<DistrictID, ParcelAnalysis> = [
            (DistrictID(0), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:forest"),    80), (Biome::from("minecraft:plains"), 20)].into())),
            (DistrictID(1), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:forest"),   100)].into())),
            (DistrictID(2), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:plains"),   100)].into())),
            (DistrictID(3), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:mountains"), 90), (Biome::from("minecraft:plains"), 10)].into())),
            (DistrictID(4), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:river"),    100)].into())),
            (DistrictID(5), ParcelAnalysis::from_biome_count([(Biome::from("minecraft:plains"),   100)].into())),
        ].into();
        // =================================

        let result = registry.resolve_for_parcels(&parcel_analysis, &mut rng);

        let mut parcel_ids: Vec<DistrictID> = parcel_analysis.keys().cloned().collect();
        parcel_ids.sort_by_key(|id| id.0);

        // ── Report ──────────────────────────────────────────────────────────────
        println!("\n╔══ Parcel Production Report ═════════════════════╗");

        println!("║ Parcels:");
        for id in &parcel_ids {
            let analysis = &parcel_analysis[id];
            let biome_names = {
                let mut names: Vec<&str> = analysis.major_biomes().iter()
                    .map(|b| b.as_str().strip_prefix("minecraft:").unwrap_or(b.as_str()))
                    .collect();
                names.sort();
                names.join("+")
            };
            if let Some(a) = result.parcel_assignments.get(id) {
                println!("║   Parcel {:>2} ({:<18}) → {} x2 [{}]",
                    id.0, biome_names, a.primary_resource, a.building);
            } else {
                println!("║   Parcel {:>2} ({:<18}) → (no valid resource)", id.0, biome_names);
            }
        }

        println!("║");
        println!("║ Resource Supply:");
        let mut supply_sorted: Vec<(&String, &u32)> = result.supply.iter().collect();
        supply_sorted.sort_by_key(|(r, _)| r.as_str());
        for (resource, qty) in supply_sorted {
            println!("║   {:<20} x{}", resource, qty);
        }

        println!("║");
        println!("║ Goods Produced:");
        if result.finished_goods.is_empty() && result.leftover_goods.is_empty() {
            println!("║   (none)");
        }
        for (good, qty) in &result.finished_goods {
            println!("║   {:<20} x{}", good, qty);
        }
        for (good, qty) in &result.leftover_goods {
            println!("║   {:<20} x{}  (unused)", good, qty);
        }

        println!("║");
        println!("║ Gathering Buildings:");
        let mut gb_sorted: Vec<(&String, &u32)> = result.gather_buildings.iter().collect();
        gb_sorted.sort_by_key(|(b, _)| b.as_str());
        for (building, count) in gb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("║");
        println!("║ Processing Buildings Required:");
        if result.processing_buildings.is_empty() {
            println!("║   (none)");
        }
        let mut pb_sorted: Vec<(&String, &u32)> = result.processing_buildings.iter().collect();
        pb_sorted.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
        for (building, count) in pb_sorted {
            println!("║   {:<20} x{}", building, count);
        }

        println!("╚═══════════════════════════════════════════════════╝\n");
    }

    /// Diagnostic test — prints a full production report for a given set of raw resources.
    ///
    /// Edit the `inputs` map below to test different scenarios, then run:
    ///   cargo test production_report -- --nocapture
    ///
    /// Chains are worked through in priority order (longest first). Raw resources are
    /// allocated greedily: each chain uses as much as the remaining supply allows.
    #[test]
    fn production_report() {
        let registry = make_registry();
        let mut rng = RNG::new(42);

        // ====== Edit inputs here ======
        let inputs: &[(&str, f32)] = &[
            ("wood",     10.0),
            ("iron_ore", 10.0),
            ("feathers", 15.0),
            ("flint",     8.0),
            ("wheat",    12.0),
            ("wool",      6.0),
        ];
        // ==============================

        let mut remaining: HashMap<String, f32> = inputs.iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();

        let available: HashSet<String> = remaining.keys().cloned().collect();
        let plan = registry.select_production(&available, &mut rng);

        // For each chain (highest priority first), allocate a proportional raw budget
        // and forward-execute its recipes. Each recipe consumes input fractionally but
        // produces FLOORED integer output — anything below 1 unit of an intermediate is
        // lost and can't flow to the next building. Matches `resolve_for_parcels`.
        let mut goods_produced: Vec<(String, u32)> = Vec::new();
        let mut total_buildings: HashMap<String, u32> = HashMap::new();
        let mut intermediate_pool: HashMap<String, u32> = HashMap::new();
        for chain in &plan.chains {
            let cost = match registry.raw_cost(&chain.finished_good) {
                Some(c) => c.clone(),
                None => continue,
            };
            let raw_units_f = cost.iter()
                .map(|(raw, per)| remaining.get(raw).copied().unwrap_or(0.0) / per)
                .fold(f32::INFINITY, f32::min);
            if !raw_units_f.is_finite() || raw_units_f <= 0.0 { continue; }

            let mut local: HashMap<String, f32> = cost.iter()
                .map(|(raw, per)| (raw.clone(), per * raw_units_f))
                .collect();
            for (raw, per) in &cost {
                *remaining.entry(raw.clone()).or_insert(0.0) -= per * raw_units_f;
            }

            for recipe_id in &chain.recipe_ids {
                let recipe = &registry.recipes()[recipe_id];
                if recipe.inputs.is_empty() { continue; }

                let batches = recipe.inputs.iter()
                    .map(|(input, qty)| local.get(input).copied().unwrap_or(0.0) / *qty as f32)
                    .fold(f32::INFINITY, f32::min);
                if !batches.is_finite() || batches <= 0.0 { continue; }

                for (input, qty) in &recipe.inputs {
                    *local.entry(input.clone()).or_insert(0.0) -= batches * *qty as f32;
                }
                for (output, qty) in &recipe.outputs {
                    let produced = (batches * *qty as f32).floor();
                    *local.entry(output.clone()).or_insert(0.0) += produced;
                }
                *total_buildings.entry(recipe.building.clone()).or_insert(0) += batches.ceil() as u32;
            }

            let finished_qty = local.get(&chain.finished_good).copied().unwrap_or(0.0).floor() as u32;
            if finished_qty > 0 {
                goods_produced.push((chain.finished_good.clone(), finished_qty));
            }
            for (id, qty) in &local {
                if id == &chain.finished_good { continue; }
                if registry.resources().get(id).map(|r| r.tier == 1).unwrap_or(false) {
                    let units = qty.floor() as u32;
                    if units > 0 {
                        *intermediate_pool.entry(id.clone()).or_insert(0) += units;
                    }
                }
            }
        }

        // ── Report ──────────────────────────────────────────────────────────────
        println!("\n╔══ Production Report ══════════════════════════════╗");

        println!("║ Raw inputs:");
        let mut sorted_inputs: Vec<_> = inputs.iter().collect();
        sorted_inputs.sort_by_key(|(k, _)| *k);
        for (resource, qty) in &sorted_inputs {
            println!("║   {:<20} {:>6.1}", resource, qty);
        }

        println!("║");
        println!("║ Buildings required:");
        let mut buildings: Vec<_> = total_buildings.iter().collect();
        buildings.sort_by(|(a_name, a_count), (b_name, b_count)| b_count.cmp(a_count).then(a_name.cmp(b_name)));
        for (building, count) in &buildings {
            println!("║   {:<20} x{}", building, count);
        }

        println!("║");
        println!("║ Goods produced:");
        if goods_produced.is_empty() {
            println!("║   (none)");
        }
        for (good, qty) in &goods_produced {
            println!("║   {:<20} x{}", good, qty);
        }

        // Tier-1 intermediates that piled up because downstream recipes couldn't consume
        // them (or fractional output got floored away). These are surfaced as production.
        let mut intermediate_leftovers: Vec<(String, u32)> = intermediate_pool.iter()
            .filter(|(_, q)| **q > 0)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        intermediate_leftovers.sort_by_key(|(r, _)| r.clone());

        println!("║");
        println!("║ Leftover intermediate goods:");
        if intermediate_leftovers.is_empty() {
            println!("║   (none)");
        }
        for (good, qty) in &intermediate_leftovers {
            println!("║   {:<20} x{}", good, qty);
        }

        println!("║");
        println!("║ Leftover raw resources:");
        let mut leftovers: Vec<_> = remaining.iter().collect();
        leftovers.sort_by_key(|(k, _)| k.as_str());
        let any_left = leftovers.iter().any(|(_, v)| **v > 0.001);
        if !any_left {
            println!("║   (none — fully utilised)");
        }
        for (resource, qty) in &leftovers {
            if **qty > 0.001 {
                println!("║   {:<20} {:>6.1}", resource, qty);
            }
        }

        println!("╚═══════════════════════════════════════════════════╝\n");
    }
}
