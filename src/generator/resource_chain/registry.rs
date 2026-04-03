use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use anyhow::{bail, Context};

use crate::minecraft::Biome;
use crate::noise::RNG;

use super::types::{BiomeResourcesFile, RecipeDef, RecipesFile, ResourceDef, ResourcesFile};

pub struct ChainSelection {
    /// The tier-2 good at the end of this chain.
    pub finished_good: String,
    /// Recipe IDs in dependency traversal order (finished good first, raw inputs last).
    pub recipe_ids: Vec<String>,
    /// Buildings needed per 1 unit of finished good produced.
    /// Accounts for recipe batch sizes: a recipe that outputs 8 arrows from 1 run
    /// contributes 1/8 runs (and thus 1/8 buildings) per arrow.
    /// Two recipe steps using the same building are summed.
    pub building_run_cost: HashMap<String, f32>,
    /// Raw resources (tier 0) consumed by this chain.
    pub raw_inputs: HashSet<String>,
    /// Intermediate goods produced and consumed within the chain.
    pub intermediates: HashSet<String>,
    /// Number of distinct recipes in the chain.
    pub depth: usize,
}

pub struct ProductionPlan {
    /// Chains ranked by depth DESC, raw_inputs.len() DESC, intermediates.len() DESC, then random.
    pub chains: Vec<ChainSelection>,
    /// `building_run_cost` aggregated across all chains — useful for checking which building
    /// types the plan requires. Multiply by units produced and ceil to get physical buildings.
    pub building_run_cost: HashMap<String, f32>,
}

#[derive(Debug)]
pub struct ResourceRegistry {
    resources: HashMap<String, ResourceDef>,
    recipes: HashMap<String, RecipeDef>,
    /// resource_id -> all recipe_ids that produce it, sorted for determinism (charcoal before coal)
    produced_by: HashMap<String, Vec<String>>,
    /// resource_id -> recipe_ids that consume it
    consumed_by: HashMap<String, Vec<String>>,
    /// resource_id -> raw inputs required per 1 unit of output
    raw_cost: HashMap<String, HashMap<String, f32>>,
    /// biome id (e.g. "minecraft:forest") -> raw resource ids
    biome_resources: HashMap<String, Vec<String>>,
}

pub struct ResolvedChains {
    /// All resource IDs producible from the given raw inputs (includes the raw inputs themselves)
    pub producible: HashSet<String>,
    /// Recipe IDs used to produce them
    pub recipes_used: Vec<String>,
    /// Recipes that are one missing ingredient away from being unlocked
    pub nearly_unlocked: Vec<NearMiss>,
}

pub struct NearMiss {
    pub recipe_id: String,
    pub missing: Vec<String>,
}

impl ResourceRegistry {
    pub fn load() -> anyhow::Result<Self> {
        let base = env::current_dir()?.join("data").join("resource_chains");

        let resources: ResourcesFile = load_yaml(base.join("resources.yaml"))?;
        let recipes: RecipesFile = load_yaml(base.join("recipes.yaml"))?;
        let biome_resources: BiomeResourcesFile = load_yaml(base.join("biome_resources.yaml"))?;

        Self::from_parts(
            resources.resources,
            recipes.recipes,
            biome_resources.biome_resources,
        )
    }

    pub(super) fn from_parts(
        resources: HashMap<String, ResourceDef>,
        recipes: HashMap<String, RecipeDef>,
        biome_resources: HashMap<String, Vec<String>>,
    ) -> anyhow::Result<Self> {
        validate_recipe_references(&resources, &recipes)?;

        let produced_by: HashMap<String, Vec<String>> = build_produced_by(&recipes)?;
        let consumed_by = build_consumed_by(&recipes);

        detect_cycles(&resources, &produced_by, &recipes)?;

        let raw_cost = compute_all_raw_costs(&resources, &produced_by, &recipes);

        Ok(Self {
            resources,
            recipes,
            produced_by,
            consumed_by,
            raw_cost,
            biome_resources,
        })
    }

    /// Given a set of available raw resource IDs, return all producible
    /// resources and the recipes needed to reach them.
    pub fn resolve(&self, available: &HashSet<String>) -> ResolvedChains {
        let mut have = available.clone();
        let mut used_recipes: Vec<String> = vec![];
        let mut changed = true;

        while changed {
            changed = false;
            for (recipe_id, recipe) in &self.recipes {
                let all_inputs = recipe.inputs.keys().all(|r| have.contains(r));
                let any_new_output = recipe.outputs.keys().any(|r| !have.contains(r));
                if all_inputs && any_new_output {
                    for output in recipe.outputs.keys() {
                        have.insert(output.clone());
                    }
                    used_recipes.push(recipe_id.clone());
                    changed = true;
                }
            }
        }

        let nearly_unlocked = self.recipes.iter()
            .filter(|(id, _)| !used_recipes.contains(id))
            .filter_map(|(id, recipe)| {
                let missing: Vec<String> = recipe.inputs.keys()
                    .filter(|r| !have.contains(*r))
                    .cloned()
                    .collect();
                if missing.len() == 1 {
                    Some(NearMiss { recipe_id: id.clone(), missing })
                } else {
                    None
                }
            })
            .collect();

        ResolvedChains { producible: have, recipes_used: used_recipes, nearly_unlocked }
    }

    /// Returns the raw resource IDs provided by a given biome.
    pub fn resources_for_biome(&self, biome: &Biome) -> HashSet<String> {
        self.biome_resources
            .get(biome.as_str())
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns the pre-computed raw input cost to produce 1 unit of a resource.
    pub fn raw_cost(&self, resource_id: &str) -> Option<&HashMap<String, f32>> {
        self.raw_cost.get(resource_id)
    }

    pub fn resources(&self) -> &HashMap<String, ResourceDef> {
        &self.resources
    }

    pub fn recipes(&self) -> &HashMap<String, RecipeDef> {
        &self.recipes
    }

    /// Given available raw resources, returns a ranked production plan:
    /// which tier-2 goods to produce and which buildings are needed.
    ///
    /// Ranking priority:
    ///   1. Chain depth (longer chains first — more value-add steps)
    ///   2. Number of distinct raw inputs used (utilise more raw goods first)
    ///   3. Number of intermediate goods produced (fuller chain first)
    ///   4. Random tiebreaking via `rng`
    pub fn select_production(&self, available: &HashSet<String>, rng: &mut RNG) -> ProductionPlan {
        let resolved = self.resolve(available);

        let mut chains_with_jitter: Vec<(ChainSelection, i64)> = resolved.producible.iter()
            .filter(|id| self.resources.get(*id).map(|r| r.tier == 2).unwrap_or(false))
            .map(|id| {
                let chain = self.trace_chain(id);
                let jitter = rng.next_i64();
                (chain, jitter)
            })
            .collect();

        chains_with_jitter.sort_by(|(a, ja), (b, jb)| {
            b.depth.cmp(&a.depth)
                .then(b.raw_inputs.len().cmp(&a.raw_inputs.len()))
                .then(b.intermediates.len().cmp(&a.intermediates.len()))
                .then(ja.cmp(jb))
        });

        let chains: Vec<ChainSelection> = chains_with_jitter.into_iter().map(|(c, _)| c).collect();

        let mut building_run_cost: HashMap<String, f32> = HashMap::new();
        for chain in &chains {
            for (building, runs) in &chain.building_run_cost {
                *building_run_cost.entry(building.clone()).or_insert(0.0) += runs;
            }
        }

        ProductionPlan { chains, building_run_cost }
    }

    /// Traces the full dependency chain for a finished good, collecting
    /// all recipe IDs and raw inputs recursively.
    fn trace_chain(&self, finished_good: &str) -> ChainSelection {
        let mut recipe_ids: Vec<String> = Vec::new();
        let mut raw_inputs: HashSet<String> = HashSet::new();

        self.trace_resource(finished_good, &mut recipe_ids, &mut raw_inputs);

        // Compute buildings needed per 1 unit of finished good, accounting for
        // recipe batch sizes (e.g. 1 arrow run → 8 arrows = 0.125 buildings per arrow).
        let mut recipe_runs: HashMap<String, f32> = HashMap::new();
        self.compute_recipe_runs(finished_good, 1.0, &mut recipe_runs);

        let mut building_run_cost: HashMap<String, f32> = HashMap::new();
        for (recipe_id, runs) in &recipe_runs {
            let building = &self.recipes[recipe_id].building;
            *building_run_cost.entry(building.clone()).or_insert(0.0) += runs;
        }

        let intermediates: HashSet<String> = recipe_ids.iter()
            .flat_map(|rid| self.recipes[rid].outputs.keys().cloned())
            .filter(|id| id.as_str() != finished_good)
            .collect();

        let depth = recipe_ids.len();

        ChainSelection {
            finished_good: finished_good.to_string(),
            recipe_ids,
            building_run_cost,
            raw_inputs,
            intermediates,
            depth,
        }
    }

    /// Recursively computes how many times each recipe must run to produce
    /// `needed_per_unit` of `resource_id` (relative to 1 unit of the original finished good).
    fn compute_recipe_runs(
        &self,
        resource_id: &str,
        needed_per_unit: f32,
        recipe_runs: &mut HashMap<String, f32>,
    ) {
        let Some(recipe_id) = self.produced_by.get(resource_id).and_then(|v| v.first()) else {
            return; // raw input — no recipe
        };
        let recipe = &self.recipes[recipe_id];
        let output_qty = recipe.outputs[resource_id] as f32;
        let runs_per_unit = needed_per_unit / output_qty;
        *recipe_runs.entry(recipe_id.clone()).or_insert(0.0) += runs_per_unit;
        for (input_id, &input_qty) in &recipe.inputs {
            self.compute_recipe_runs(input_id, runs_per_unit * input_qty as f32, recipe_runs);
        }
    }

    /// Generates a Mermaid flowchart string representing the full production graph.
    /// Resources are grouped by tier; recipe nodes show the required building.
    ///
    /// Paste the output into https://mermaid.live, or use `generate_production_graph`
    /// in tests to write a self-contained HTML file.
    pub fn to_mermaid_graph(&self) -> String {
        let mut lines: Vec<String> = vec![
            "flowchart LR".into(),
            "  classDef raw          fill:#90EE90,stroke:#2d862d,color:#000".into(),
            "  classDef intermediate fill:#FFD700,stroke:#b8860b,color:#000".into(),
            "  classDef finished     fill:#87CEEB,stroke:#00008b,color:#000".into(),
            "  classDef recipe       fill:#f5f5f5,stroke:#aaa,color:#333".into(),
            "".into(),
        ];

        // Collect resources sorted by tier then id for deterministic output
        let mut raw: Vec<_>          = self.resources.iter().filter(|(_, d)| d.tier == 0).collect();
        let mut intermediate: Vec<_> = self.resources.iter().filter(|(_, d)| d.tier == 1).collect();
        let mut finished: Vec<_>     = self.resources.iter().filter(|(_, d)| d.tier == 2).collect();
        raw.sort_by_key(|(id, _)| id.as_str());
        intermediate.sort_by_key(|(id, _)| id.as_str());
        finished.sort_by_key(|(id, _)| id.as_str());

        lines.push("  subgraph Raw [\"Raw Resources\"]".into());
        for (id, def) in &raw {
            lines.push(format!("    {}([{}]):::raw", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        lines.push("  subgraph Intermediate [\"Intermediate Goods\"]".into());
        for (id, def) in &intermediate {
            lines.push(format!("    {}[{}]:::intermediate", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        lines.push("  subgraph Finished [\"Finished Goods\"]".into());
        for (id, def) in &finished {
            lines.push(format!("    {}(({})):::finished", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        // Recipe nodes and edges, sorted for determinism
        let mut recipes: Vec<_> = self.recipes.iter().collect();
        recipes.sort_by_key(|(id, _)| id.as_str());

        for (recipe_id, recipe) in &recipes {
            let node_id = format!("rec_{}", recipe_id);
            lines.push(format!("  {}{{{}}}:::recipe", node_id, recipe.building));

            let mut inputs: Vec<_> = recipe.inputs.keys().collect();
            inputs.sort();
            for input in inputs {
                lines.push(format!("  {} --> {}", input, node_id));
            }

            let mut outputs: Vec<_> = recipe.outputs.keys().collect();
            outputs.sort();
            for output in outputs {
                lines.push(format!("  {} --> {}", node_id, output));
            }
            lines.push("".into());
        }

        lines.join("\n")
    }

    /// DFS walk backwards from a resource, accumulating recipe IDs (deduplicated)
    /// and raw inputs.
    fn trace_resource(
        &self,
        resource_id: &str,
        recipe_ids: &mut Vec<String>,
        raw_inputs: &mut HashSet<String>,
    ) {
        // Use the first (canonical) producer for chain tracing
        match self.produced_by.get(resource_id).and_then(|v| v.first()) {
            None => {
                raw_inputs.insert(resource_id.to_string());
            }
            Some(recipe_id) => {
                if !recipe_ids.contains(recipe_id) {
                    recipe_ids.push(recipe_id.clone());
                    let inputs: Vec<String> = self.recipes[recipe_id].inputs.keys().cloned().collect();
                    for input in inputs {
                        self.trace_resource(&input, recipe_ids, raw_inputs);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) fn load_yaml<T: serde::de::DeserializeOwned>(path: impl AsRef<std::path::Path>) -> anyhow::Result<T> {
    let path = path.as_ref();
    let file = File::open(path)
        .with_context(|| format!("Could not open {:?}", path))?;
    serde_yaml::from_reader(file)
        .with_context(|| format!("Could not parse {:?}", path))
}

fn validate_recipe_references(
    resources: &HashMap<String, ResourceDef>,
    recipes: &HashMap<String, RecipeDef>,
) -> anyhow::Result<()> {
    for (recipe_id, recipe) in recipes {
        for resource_id in recipe.inputs.keys().chain(recipe.outputs.keys()) {
            if !resources.contains_key(resource_id.as_str()) {
                bail!(
                    "Recipe '{}' references unknown resource '{}'",
                    recipe_id, resource_id
                );
            }
        }
        if recipe.outputs.is_empty() {
            bail!("Recipe '{}' has no outputs", recipe_id);
        }
    }
    Ok(())
}

/// Maps each output resource to all recipe IDs that produce it, sorted for determinism.
/// Multiple producers are allowed (e.g. charcoal and coal variants of the same recipe).
fn build_produced_by(recipes: &HashMap<String, RecipeDef>) -> anyhow::Result<HashMap<String, Vec<String>>> {
    let mut produced_by: HashMap<String, Vec<String>> = HashMap::new();
    for (recipe_id, recipe) in recipes {
        for output_id in recipe.outputs.keys() {
            produced_by.entry(output_id.clone()).or_default().push(recipe_id.clone());
        }
    }
    for producers in produced_by.values_mut() {
        producers.sort();
    }
    Ok(produced_by)
}

fn build_consumed_by(recipes: &HashMap<String, RecipeDef>) -> HashMap<String, Vec<String>> {
    let mut consumed_by: HashMap<String, Vec<String>> = HashMap::new();
    for (recipe_id, recipe) in recipes {
        for input_id in recipe.inputs.keys() {
            consumed_by.entry(input_id.clone()).or_default().push(recipe_id.clone());
        }
    }
    consumed_by
}

fn detect_cycles(
    resources: &HashMap<String, ResourceDef>,
    produced_by: &HashMap<String, Vec<String>>,
    recipes: &HashMap<String, RecipeDef>,
) -> anyhow::Result<()> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = vec![];

    for resource_id in resources.keys() {
        if !visited.contains(resource_id.as_str()) {
            if let Some(cycle_at) = dfs_find_cycle(resource_id, produced_by, recipes, &mut visited, &mut path) {
                bail!("Cycle detected in resource chain at '{}'", cycle_at);
            }
        }
    }
    Ok(())
}

fn dfs_find_cycle(
    resource_id: &str,
    produced_by: &HashMap<String, Vec<String>>,
    recipes: &HashMap<String, RecipeDef>,
    visited: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<String> {
    if path.contains(&resource_id.to_string()) {
        return Some(resource_id.to_string());
    }
    if visited.contains(resource_id) {
        return None;
    }

    path.push(resource_id.to_string());

    // Check all producers — a cycle via any one path is still a cycle
    if let Some(recipe_ids) = produced_by.get(resource_id) {
        for recipe_id in recipe_ids {
            let recipe = &recipes[recipe_id];
            for input_id in recipe.inputs.keys() {
                if let Some(cycle) = dfs_find_cycle(input_id, produced_by, recipes, visited, path) {
                    return Some(cycle);
                }
            }
        }
    }

    path.pop();
    visited.insert(resource_id.to_string());
    None
}

fn compute_all_raw_costs(
    resources: &HashMap<String, ResourceDef>,
    produced_by: &HashMap<String, Vec<String>>,
    recipes: &HashMap<String, RecipeDef>,
) -> HashMap<String, HashMap<String, f32>> {
    let mut memo: HashMap<String, HashMap<String, f32>> = HashMap::new();
    for resource_id in resources.keys() {
        compute_raw_cost_for(resource_id, produced_by, recipes, &mut memo);
    }
    memo
}

fn compute_raw_cost_for(
    resource_id: &str,
    produced_by: &HashMap<String, Vec<String>>,
    recipes: &HashMap<String, RecipeDef>,
    memo: &mut HashMap<String, HashMap<String, f32>>,
) -> HashMap<String, f32> {
    if let Some(cached) = memo.get(resource_id) {
        return cached.clone();
    }

    // Use the first (alphabetically earliest) producer as the canonical cost path.
    // For charcoal/coal pairs the charcoal recipe sorts first, so charcoal is the default.
    let cost = match produced_by.get(resource_id).and_then(|v| v.first()) {
        None => {
            let mut m = HashMap::new();
            m.insert(resource_id.to_string(), 1.0);
            m
        }
        Some(recipe_id) => {
            let recipe = &recipes[recipe_id];
            let output_qty = recipe.outputs[resource_id] as f32;
            let mut total: HashMap<String, f32> = HashMap::new();
            for (input_id, &input_qty) in &recipe.inputs {
                let input_cost = compute_raw_cost_for(input_id, produced_by, recipes, memo);
                let scale = input_qty as f32 / output_qty;
                for (raw_id, raw_qty) in input_cost {
                    *total.entry(raw_id).or_insert(0.0) += raw_qty * scale;
                }
            }
            total
        }
    };

    memo.insert(resource_id.to_string(), cost.clone());
    cost
}
