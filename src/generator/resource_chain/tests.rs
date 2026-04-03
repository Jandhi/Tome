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

        assert!(tools_pos < furniture_pos, "tools (depth 4) should rank above furniture (depth 2)");
        assert!(furniture_pos < arrows_pos, "furniture (depth 2) should rank above arrows (depth 1)");
    }

    #[test]
    fn select_production_buildings_are_complete() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["wood"].iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);

        // Furniture chain requires sawmill (planks) and carpentry (furniture)
        assert!(plan.building_run_cost.contains_key("sawmill"), "sawmill needed for planks");
        assert!(plan.building_run_cost.contains_key("carpentry"), "carpentry needed for furniture");
        // Charcoal chain requires kiln — but charcoal is tier 1, not tier 2,
        // so it only appears if it feeds a tier-2 chain. With only wood, no tier-2 chain
        // needs charcoal, so kiln should NOT appear.
        assert!(!plan.building_run_cost.contains_key("kiln"), "kiln not needed without a charcoal consumer");
    }

    #[test]
    fn select_production_chain_depth_matches_recipe_count() {
        let registry = make_registry();
        let mut rng = RNG::new(42);
        let available: HashSet<String> = ["wood", "iron_ore"]
            .iter().map(|s| s.to_string()).collect();

        let plan = registry.select_production(&available, &mut rng);

        let tools_chain = plan.chains.iter().find(|c| c.finished_good == "tools").expect("tools should be in plan");
        // Recipes: wood_to_charcoal, iron_ore_charcoal_to_ingot, iron_ingot_to_tools
        assert_eq!(tools_chain.depth, 3, "tools chain should have 3 recipes");
        assert_eq!(tools_chain.raw_inputs.len(), 2, "tools chain uses wood and iron_ore");
    }

    /// Generates a standalone HTML file containing an interactive Mermaid graph of all
    /// production chains and opens it in the default browser.
    ///
    /// Run with:
    ///   cargo test generate_production_graph -- --nocapture
    #[test]
    fn generate_production_graph() {
        use std::fs;
        use std::io::Write;

        let registry = make_registry();
        let mermaid = registry.to_mermaid_graph();

        let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Production Chains</title>
  <script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
  <style>
    body {{ margin: 0; background: #1a1a2e; display: flex; flex-direction: column; align-items: center; font-family: sans-serif; }}
    h1   {{ color: #eee; margin: 1rem 0 0.25rem; }}
    p    {{ color: #aaa; margin: 0 0 1rem; font-size: 0.85rem; }}
    .mermaid {{ background: #fff; border-radius: 8px; padding: 1.5rem; max-width: 98vw; overflow: auto; }}
  </style>
</head>
<body>
  <h1>Production Chains</h1>
  <p>Green = raw &nbsp;·&nbsp; Yellow = intermediate &nbsp;·&nbsp; Blue = finished &nbsp;·&nbsp; Diamonds = buildings</p>
  <div class="mermaid">
{}
  </div>
  <script>mermaid.initialize({{ startOnLoad: true, theme: 'default', flowchart: {{ rankSpacing: 80, nodeSpacing: 30 }} }});</script>
</body>
</html>"#, mermaid);

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

        // For each chain (highest priority first), allocate as many units as remaining
        // raw resources allow, then deduct those resources.
        let mut goods_produced: Vec<(String, u32)> = Vec::new();
        for chain in &plan.chains {
            let cost = match registry.raw_cost(&chain.finished_good) {
                Some(c) => c.clone(),
                None => continue,
            };

            let units = cost.iter()
                .map(|(raw, cost_per)| {
                    let have = remaining.get(raw).copied().unwrap_or(0.0);
                    (have / cost_per).floor() as u32
                })
                .min()
                .unwrap_or(0);

            if units == 0 {
                continue;
            }

            for (raw, cost_per) in &cost {
                *remaining.entry(raw.clone()).or_insert(0.0) -= cost_per * units as f32;
            }

            goods_produced.push((chain.finished_good.clone(), units));
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
        // Scale each chain's building run cost by units produced, then ceil.
        // building_run_cost is buildings-per-1-unit, so multiply by units and round up.
        let mut total_buildings: HashMap<String, u32> = HashMap::new();
        for chain in &plan.chains {
            let units = goods_produced.iter()
                .find(|(g, _)| g == &chain.finished_good)
                .map(|(_, u)| *u)
                .unwrap_or(0);
            if units == 0 { continue; }
            for (building, runs_per_unit) in &chain.building_run_cost {
                let count = (runs_per_unit * units as f32).ceil() as u32;
                *total_buildings.entry(building.clone()).or_insert(0) += count;
            }
        }

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
