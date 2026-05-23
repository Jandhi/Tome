#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::env;

    use crate::minecraft::Biome;
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
    fn assign_district_resources_respects_constraints() {
        let registry = make_registry();
        let mut rng = RNG::new(42);

        // District 0: only wheat (most constrained — 1 option)
        // District 1: iron_ore or coal
        // District 2: wood or feathers
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["wheat".into()]),
            (1, vec!["iron_ore".into(), "coal".into()]),
            (2, vec!["wood".into(), "feathers".into()]),
        ].into();

        let assignments = registry.assign_district_resources(&options, &mut rng);

        assert_eq!(assignments.len(), 3, "all districts should be assigned");

        // Sole-option district always gets wheat with the farm building
        let a0 = assignments.get(&0).unwrap();
        assert_eq!(a0.resource, "wheat");
        assert_eq!(a0.building, "farm");

        // All buildings must be non-empty
        for a in assignments.values() {
            assert!(!a.building.is_empty());
        }
    }

    #[test]
    fn assign_district_resources_prefers_diversity() {
        let registry = make_registry();
        let mut rng = RNG::new(0);

        // Two districts that both could produce wood or iron_ore; they should pick different ones.
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["wood".into(), "iron_ore".into()]),
            (1, vec!["wood".into(), "iron_ore".into()]),
        ].into();

        let assignments = registry.assign_district_resources(&options, &mut rng);

        assert_eq!(assignments.len(), 2);
        let r0 = &assignments[&0].resource;
        let r1 = &assignments[&1].resource;
        assert_ne!(r0, r1, "districts should pick different resources when possible");
    }

    #[test]
    fn assign_district_resources_skips_unknown_gather_resources() {
        let registry = make_registry();
        let mut rng = RNG::new(42);

        // gold_ore appears in biome_resources.yaml but has no gather recipe
        let options: HashMap<u32, Vec<String>> = [
            (0, vec!["gold_ore".into()]),
            (1, vec!["wood".into()]),
        ].into();

        let assignments = registry.assign_district_resources(&options, &mut rng);

        // District 0 has no valid candidates and is skipped
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[&1].resource, "wood");
    }

    /// Diagnostic test — passes a `HashMap<SuperDistrictID, DistrictAnalysis>` to
    /// `resolve_for_districts` and prints the full settlement production report.
    ///
    /// Edit the `district_analysis` map below and run:
    ///   cargo test district_production_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn district_production_report() {
        use crate::generator::districts::{SuperDistrictID, DistrictAnalysis};
        use crate::minecraft::Biome;

        let registry = make_registry();
        let mut rng = RNG::new(42);

        // ====== Edit districts here ======
        // `from_biome_count` takes biome → block count; biomes at ≥30% count as major.
        let district_analysis: HashMap<SuperDistrictID, DistrictAnalysis> = [
            (SuperDistrictID(0), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:forest"),    80), (Biome::from("minecraft:plains"), 20)].into())),
            (SuperDistrictID(1), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:forest"),   100)].into())),
            (SuperDistrictID(2), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:plains"),   100)].into())),
            (SuperDistrictID(3), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:mountains"), 90), (Biome::from("minecraft:plains"), 10)].into())),
            (SuperDistrictID(4), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:river"),    100)].into())),
            (SuperDistrictID(5), DistrictAnalysis::from_biome_count([(Biome::from("minecraft:plains"),   100)].into())),
        ].into();
        // =================================

        let result = registry.resolve_for_districts(&district_analysis, &mut rng);

        let mut district_ids: Vec<SuperDistrictID> = district_analysis.keys().cloned().collect();
        district_ids.sort_by_key(|id| id.0);

        // ── Report ──────────────────────────────────────────────────────────────
        println!("\n╔══ District Production Report ═════════════════════╗");

        println!("║ Districts:");
        for id in &district_ids {
            let analysis = &district_analysis[id];
            let biome_names = {
                let mut names: Vec<&str> = analysis.major_biomes().iter()
                    .map(|b| b.as_str().strip_prefix("minecraft:").unwrap_or(b.as_str()))
                    .collect();
                names.sort();
                names.join("+")
            };
            if let Some(a) = result.district_assignments.get(id) {
                println!("║   District {:>2} ({:<18}) → {} x2 [{}]",
                    id.0, biome_names, a.resource, a.building);
            } else {
                println!("║   District {:>2} ({:<18}) → (no valid resource)", id.0, biome_names);
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
        // lost and can't flow to the next building. Matches `resolve_for_districts`.
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
