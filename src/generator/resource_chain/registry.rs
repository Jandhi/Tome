use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::env;
use std::fs::File;
use anyhow::{bail, Context};

use crate::generator::districts::{DistrictAnalysis, SuperDistrictID};
use crate::generator::nbts::{Structure, StructureType};
use crate::minecraft::Biome;
use crate::noise::RNG;

use super::types::{BiomeResourcesFile, DistrictResourceAssignment, RecipeDef, RecipesFile, ResourceDef, ResourcesFile};

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

/// Full production result for a settlement derived from a set of districts.
pub struct SettlementProductionResult {
    /// The resource each district will gather and the building it needs to do so.
    pub district_assignments: HashMap<SuperDistrictID, DistrictResourceAssignment>,
    /// Total raw resource supply available (resource → quantity, 2 per producing district).
    pub supply: HashMap<String, u32>,
    /// Finished goods produced after allocating supply across chains, in priority order.
    pub finished_goods: Vec<(String, u32)>,
    /// Raw and intermediate goods left over after chain allocation.
    pub leftover_goods: Vec<(String, u32)>,
    /// Gathering buildings required across all districts (building type → count).
    pub gather_buildings: HashMap<String, u32>,
    /// Processing buildings required, scaled by units of finished goods produced.
    pub processing_buildings: HashMap<String, u32>,
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
                if recipe.inputs.is_empty() { continue; } // gather recipes don't propagate availability
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
            .filter(|(_, recipe)| !recipe.inputs.is_empty()) // gather recipes are never near-misses
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

    /// Verifies that every `recipe.building` value resolves to a loaded
    /// `Structure` whose NBT lives under `data/structures/resource_buildings/`.
    /// Surfaces all missing or misplaced entries in a single error so the
    /// operator can fix them in one pass rather than rerunning per fix.
    pub fn validate_buildings(&self, structures: &HashMap<StructureType, Structure>) -> anyhow::Result<()> {
        let mut missing: Vec<String> = Vec::new();
        let mut misplaced: Vec<(String, String)> = Vec::new();

        let mut seen: HashSet<&str> = HashSet::new();
        for recipe in self.recipes.values() {
            if !seen.insert(recipe.building.as_str()) {
                continue;
            }
            let key = StructureType(recipe.building.clone());
            match structures.get(&key) {
                None => missing.push(recipe.building.clone()),
                Some(structure) => {
                    if !structure.meta.path.contains("resource_buildings") {
                        misplaced.push((recipe.building.clone(), structure.meta.path.clone()));
                    }
                }
            }
        }

        if missing.is_empty() && misplaced.is_empty() {
            return Ok(());
        }

        let mut msg = String::from("Resource chain building validation failed:");
        if !missing.is_empty() {
            missing.sort();
            msg.push_str("\n  Missing structures (no .json under data/structures/resource_buildings/): ");
            msg.push_str(&missing.join(", "));
        }
        if !misplaced.is_empty() {
            misplaced.sort();
            msg.push_str("\n  Misplaced structures (not under resource_buildings/):");
            for (id, path) in &misplaced {
                msg.push_str(&format!("\n    - {} -> {}", id, path));
            }
        }
        bail!(msg)
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

    /// Given a map of district analyses, resolves the full production picture for the settlement:
    /// selects one raw resource per district (based on major biomes), allocates supply across
    /// production chains, and returns per-district building assignments alongside the complete
    /// building and goods summary.
    pub fn resolve_for_districts(
        &self,
        district_analysis: &HashMap<SuperDistrictID, DistrictAnalysis>,
        rng: &mut RNG,
    ) -> SettlementProductionResult {
        // caps to prevent extreme overproduction of certain goods that would skew the settlement's production profile
        const RAW_SURPLUS_CAP: u32 = 5;
        const INTERMEDIATE_CAP: u32 = 10;
        const FINISHED_GOOD_CAP: u32 = 15;

        // Build candidate resource lists from each district's major biomes (≥30%).
        let base_options: HashMap<SuperDistrictID, Vec<String>> = district_analysis.iter()
            .map(|(id, analysis)| {
                let mut candidates: Vec<String> = analysis.major_biomes().iter()
                    .flat_map(|biome| self.resources_for_biome(biome))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                candidates.sort();
                (*id, candidates)
            })
            .collect();

        // Iteratively cap over-supplied resources: run a trial assignment, simulate the full
        // production pipeline, and remove resources that would generate excess goods.
        //   - raw resource surplus  ≥ RAW_SURPLUS_CAP  → cap that raw resource directly
        //   - finished good output  ≥ FINISHED_GOOD_CAP → cap all raw inputs feeding that good
        //   - intermediate leftover ≥ INTERMEDIATE_CAP  → cap all raw inputs feeding that good
        // Repeat until stable (up to 10 iterations).
        let mut capped: HashSet<String> = HashSet::new();
        for _ in 0..10 {
            let filtered: HashMap<SuperDistrictID, Vec<String>> = base_options.iter()
                .map(|(id, candidates)| {
                    let filtered: Vec<String> = candidates.iter()
                        .filter(|r| !capped.contains(*r))
                        .cloned()
                        .collect();
                    // Fall back to all candidates if filtering leaves nothing.
                    let chosen = if filtered.is_empty() { candidates.clone() } else { filtered };
                    (*id, chosen)
                })
                .collect();

            let trial_assignments = self.assign_district_resources(&filtered, &mut rng.derive());

            // Compute trial supply.
            let mut trial_supply: HashMap<String, u32> = HashMap::new();
            for (_, a) in &trial_assignments {
                *trial_supply.entry(a.resource.clone()).or_insert(0) += 2;
            }

            // Run chain allocation on trial supply.
            let trial_available: HashSet<String> = trial_supply.keys().cloned().collect();
            let trial_resolved = self.resolve(&trial_available);
            let trial_plan = self.select_production(&trial_available, &mut rng.derive());
            let mut trial_remaining: HashMap<String, f32> = trial_supply.iter()
                .map(|(k, &v)| (k.clone(), v as f32))
                .collect();
            let mut trial_finished: Vec<(String, u32)> = Vec::new();

            for chain in &trial_plan.chains {
                let cost = match self.raw_cost(&chain.finished_good) {
                    Some(c) => c.clone(),
                    None => continue,
                };
                let units = cost.iter()
                    .map(|(raw, per)| (trial_remaining.get(raw).copied().unwrap_or(0.0) / per).floor() as u32)
                    .min()
                    .unwrap_or(0);
                if units == 0 { continue; }
                for (raw, per) in &cost {
                    *trial_remaining.entry(raw.clone()).or_insert(0.0) -= per * units as f32;
                }
                trial_finished.push((chain.finished_good.clone(), units));
            }

            // Compute intermediate leftovers producible from remaining raw supply.
            let trial_intermediates: Vec<(String, u32)> = trial_resolved.producible.iter()
                .filter(|id| self.resources.get(*id).map(|r| r.tier == 1).unwrap_or(false))
                .filter_map(|id| {
                    let cost = self.raw_cost(id)?;
                    if cost.is_empty() { return None; }
                    let units = cost.iter()
                        .map(|(raw, per)| (trial_remaining.get(raw).copied().unwrap_or(0.0) / per).floor() as u32)
                        .min()
                        .unwrap_or(0);
                    if units == 0 { None } else { Some((id.clone(), units)) }
                })
                .collect();

            let mut newly_capped: Vec<String> = Vec::new();

            // Raw surplus ≥ RAW_SURPLUS_CAP → cap that raw resource directly.
            for (r, &qty) in &trial_remaining {
                if qty >= RAW_SURPLUS_CAP as f32 && !capped.contains(r) {
                    newly_capped.push(r.clone());
                }
            }

            // Finished good ≥ FINISHED_GOOD_CAP → cap all raw inputs feeding it.
            for (good, units) in &trial_finished {
                if *units >= FINISHED_GOOD_CAP {
                    if let Some(cost) = self.raw_cost(good) {
                        for raw in cost.keys() {
                            if !capped.contains(raw) {
                                newly_capped.push(raw.clone());
                            }
                        }
                    }
                }
            }

            // Intermediate leftover ≥ INTERMEDIATE_CAP → cap all raw inputs feeding it.
            for (intermediate, units) in &trial_intermediates {
                if *units >= INTERMEDIATE_CAP {
                    if let Some(cost) = self.raw_cost(intermediate) {
                        for raw in cost.keys() {
                            if !capped.contains(raw) {
                                newly_capped.push(raw.clone());
                            }
                        }
                    }
                }
            }

            newly_capped.sort();
            newly_capped.dedup();

            if newly_capped.is_empty() {
                break;
            }
            capped.extend(newly_capped);
        }

        // Final filtered options and real assignment.
        let options: HashMap<SuperDistrictID, Vec<String>> = base_options.iter()
            .map(|(id, candidates)| {
                let filtered: Vec<String> = candidates.iter()
                    .filter(|r| !capped.contains(*r))
                    .cloned()
                    .collect();
                let chosen = if filtered.is_empty() { candidates.clone() } else { filtered };
                (*id, chosen)
            })
            .collect();

        // Each super-district is assigned exactly one resource producing exactly 2 units —
        // fixed regardless of how many constituent districts the super-district contains.
        let district_assignments = self.assign_district_resources(&options, rng);

        let mut supply: HashMap<String, u32> = HashMap::new();
        let mut gather_buildings: HashMap<String, u32> = HashMap::new();
        for (_, a) in &district_assignments {
            *supply.entry(a.resource.clone()).or_insert(0) += 2;
            *gather_buildings.entry(a.building.clone()).or_insert(0) += 1;
        }

        let available: HashSet<String> = supply.keys().cloned().collect();
        let resolved = self.resolve(&available);
        let plan = self.select_production(&available, rng);

        // Allocate supply across chains (highest priority first).
        let mut remaining: HashMap<String, f32> = supply.iter()
            .map(|(k, &v)| (k.clone(), v as f32))
            .collect();
        let mut finished_goods: Vec<(String, u32)> = Vec::new();
        let mut processing_buildings: HashMap<String, u32> = HashMap::new();

        for chain in &plan.chains {
            let cost = match self.raw_cost(&chain.finished_good) {
                Some(c) => c.clone(),
                None => continue,
            };
            let units = cost.iter()
                .map(|(raw, per)| (remaining.get(raw).copied().unwrap_or(0.0) / per).floor() as u32)
                .min()
                .unwrap_or(0);
            if units == 0 { continue; }

            for (raw, per) in &cost {
                *remaining.entry(raw.clone()).or_insert(0.0) -= per * units as f32;
            }
            finished_goods.push((chain.finished_good.clone(), units));

            for (building, runs_per_unit) in &chain.building_run_cost {
                if gather_buildings.contains_key(building) { continue; }
                let count = (runs_per_unit * units as f32).ceil() as u32;
                *processing_buildings.entry(building.clone()).or_insert(0) += count;
            }
        }

        // Leftover raw goods.
        let mut leftover_goods: Vec<(String, u32)> = remaining.iter()
            .filter(|(_, &qty)| qty.floor() >= 1.0)
            .map(|(r, &qty)| (r.clone(), qty.floor() as u32))
            .collect();
        leftover_goods.sort_by_key(|(r, _)| r.clone());

        // Leftover intermediate goods producible from remaining raw supply.
        let mut intermediate_leftovers: Vec<(String, u32)> = resolved.producible.iter()
            .filter(|id| self.resources.get(*id).map(|r| r.tier == 1).unwrap_or(false))
            .filter_map(|id| {
                let cost = self.raw_cost(id)?;
                if cost.is_empty() { return None; }
                let units = cost.iter()
                    .map(|(raw, per)| (remaining.get(raw).copied().unwrap_or(0.0) / per).floor() as u32)
                    .min()
                    .unwrap_or(0);
                if units == 0 { None } else { Some((id.clone(), units)) }
            })
            .collect();
        intermediate_leftovers.sort_by_key(|(r, _)| r.clone());
        leftover_goods.extend(intermediate_leftovers);

        SettlementProductionResult {
            district_assignments,
            supply,
            finished_goods,
            leftover_goods,
            gather_buildings,
            processing_buildings,
        }
    }

    /// For each entry in `options` (a district ID paired with its candidate raw resources from
    /// its biome), selects exactly one raw resource per district and returns the corresponding
    /// gathering building.
    ///
    /// **Selection strategy** — most-constrained districts (fewest valid candidates) are
    /// resolved first. For each district, candidates are scored by:
    ///   1. Novelty — +1000 if the resource has not yet been assigned to any other district,
    ///      encouraging diversity across the settlement.
    ///   2. Value — the number of tier-2 finished goods that require this resource somewhere
    ///      in their production chain (higher = feeds more goods).
    ///   3. Jitter — a small random nudge so repeated calls with the same biome mix produce
    ///      varied towns.
    ///
    /// Districts whose candidates have no known gather recipe are skipped.
    /// Input is a `HashMap` so each ID is structurally guaranteed to appear at most once,
    /// ensuring every district receives exactly one resource assignment.
    pub fn assign_district_resources<ID: Eq + Hash + Clone>(
        &self,
        options: &HashMap<ID, Vec<String>>,
        rng: &mut RNG,
    ) -> HashMap<ID, DistrictResourceAssignment> {
        // Filter each district's candidates to those with a known gather building,
        // then sort most-constrained first.
        let mut districts: Vec<(ID, Vec<String>)> = options.iter()
            .map(|(id, candidates)| {
                let valid: Vec<String> = candidates.iter()
                    .filter(|r| self.gather_building(r).is_some())
                    .cloned()
                    .collect();
                (id.clone(), valid)
            })
            .collect();
        districts.sort_by_key(|(_, candidates)| candidates.len());

        let mut assigned_set: HashSet<String> = HashSet::new();
        let mut result: HashMap<ID, DistrictResourceAssignment> = HashMap::new();

        for (id, candidates) in districts {
            if candidates.is_empty() {
                continue;
            }

            // Pre-score all candidates so rng is called once per candidate.
            let scored: Vec<(String, i64)> = candidates.iter()
                .map(|r| {
                    let value = self.resource_value_score(r) as i64;
                    let novelty: i64 = if !assigned_set.contains(r) { 1000 } else { 0 };
                    let jitter = rng.rand_i32(100) as i64;
                    (r.clone(), novelty + value + jitter)
                })
                .collect();

            let (resource, _) = scored.into_iter().max_by_key(|(_, s)| *s).unwrap();
            let building = self.gather_building(&resource).unwrap().to_string();
            assigned_set.insert(resource.clone());
            result.insert(id, DistrictResourceAssignment { resource, building });
        }

        result
    }

    /// Returns the gathering building for a raw resource — the recipe that produces
    /// it with no inputs. Returns `None` if no such recipe exists.
    fn gather_building(&self, resource_id: &str) -> Option<&str> {
        self.produced_by.get(resource_id)?
            .iter()
            .find(|recipe_id| self.recipes[*recipe_id].inputs.is_empty())
            .map(|recipe_id| self.recipes[recipe_id].building.as_str())
    }

    /// Scores a raw resource by how many tier-2 finished goods require it anywhere
    /// in their production chain. Higher = feeds more goods.
    fn resource_value_score(&self, resource_id: &str) -> usize {
        self.resources.iter()
            .filter(|(_, def)| def.tier == 2)
            .filter(|(good_id, _)| {
                self.raw_cost.get(good_id.as_str())
                    .map(|costs| costs.contains_key(resource_id))
                    .unwrap_or(false)
            })
            .count()
    }

    /// Generates a Mermaid flowchart string representing the full production graph.
    /// Resources are grouped by tier; recipe nodes show the required building.
    ///
    /// Paste the output into https://mermaid.live, or use `generate_production_graph`
    /// in tests to write a self-contained HTML file.
    pub fn to_mermaid_graph(&self) -> String {
        let mut lines: Vec<String> = vec![
            "flowchart TB".into(),
            "  classDef raw          fill:#90EE90,stroke:#2d862d,color:#000".into(),
            "  classDef intermediate fill:#FFD700,stroke:#b8860b,color:#000".into(),
            "  classDef finished     fill:#87CEEB,stroke:#00008b,color:#000".into(),
            "  classDef recipe       fill:#f5f5f5,stroke:#888,color:#333".into(),
            "".into(),
        ];

        // Collect resources sorted by category then id so related nodes cluster together
        let mut raw: Vec<_>          = self.resources.iter().filter(|(_, d)| d.tier == 0).collect();
        let mut intermediate: Vec<_> = self.resources.iter().filter(|(_, d)| d.tier == 1).collect();
        let mut finished: Vec<_>     = self.resources.iter().filter(|(_, d)| d.tier == 2).collect();
        raw.sort_by(|(a_id, a_def), (b_id, b_def)| a_def.category.cmp(&b_def.category).then(a_id.cmp(b_id)));
        intermediate.sort_by(|(a_id, a_def), (b_id, b_def)| a_def.category.cmp(&b_def.category).then(a_id.cmp(b_id)));
        finished.sort_by(|(a_id, a_def), (b_id, b_def)| a_def.category.cmp(&b_def.category).then(a_id.cmp(b_id)));

        lines.push("  subgraph Raw [\"Raw Resources\"]".into());
        lines.push("    direction LR".into());
        for (id, def) in &raw {
            lines.push(format!("    {}([{}]):::raw", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        lines.push("  subgraph Intermediate [\"Intermediate Goods\"]".into());
        lines.push("    direction LR".into());
        for (id, def) in &intermediate {
            lines.push(format!("    {}[{}]:::intermediate", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        lines.push("  subgraph Finished [\"Finished Goods\"]".into());
        lines.push("    direction LR".into());
        for (id, def) in &finished {
            lines.push(format!("    {}(({})):::finished", id, def.name));
        }
        lines.push("  end".into());
        lines.push("".into());

        // Recipe nodes and edges — skip gather recipes (empty inputs; raw resources shown already)
        let mut recipes: Vec<_> = self.recipes.iter()
            .filter(|(_, r)| !r.inputs.is_empty())
            .collect();
        recipes.sort_by_key(|(id, _)| id.as_str());

        for (recipe_id, recipe) in &recipes {
            let node_id = format!("rec_{}", recipe_id);
            lines.push(format!("  {}{{{}}}:::recipe", node_id, recipe.building));

            let mut inputs: Vec<_> = recipe.inputs.iter().collect();
            inputs.sort_by_key(|(k, _)| k.as_str());
            for (input, qty) in inputs {
                lines.push(format!("  {} -->|{}| {}", input, qty, node_id));
            }

            let mut outputs: Vec<_> = recipe.outputs.iter().collect();
            outputs.sort_by_key(|(k, _)| k.as_str());
            for (output, qty) in outputs {
                lines.push(format!("  {} -->|{}| {}", node_id, qty, output));
            }
            lines.push("".into());
        }

        lines.join("\n")
    }

    /// Returns a JavaScript array of Cytoscape.js elements (nodes + edges).
    /// Used by the `generate_production_graph` test to render an interactive graph.
    pub fn to_cytoscape_elements(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Resource nodes, sorted for determinism
        let mut resources: Vec<_> = self.resources.iter().collect();
        resources.sort_by_key(|(id, _)| id.as_str());
        for (id, def) in &resources {
            let node_type = match def.tier {
                0 => "raw",
                1 => "intermediate",
                _ => "finished",
            };
            parts.push(format!(
                r#"  {{"data":{{"id":"{id}","label":"{label}","type":"{node_type}"}}}}"#,
                id = id, label = def.name, node_type = node_type,
            ));
        }

        // Recipe nodes + edges — skip gather recipes (empty inputs)
        let mut recipes: Vec<_> = self.recipes.iter()
            .filter(|(_, r)| !r.inputs.is_empty())
            .collect();
        recipes.sort_by_key(|(id, _)| id.as_str());

        let mut eid = 0u32;
        for (recipe_id, recipe) in &recipes {
            let nid = format!("rec_{}", recipe_id);
            parts.push(format!(
                r#"  {{"data":{{"id":"{nid}","label":"{label}","type":"recipe"}}}}"#,
                nid = nid, label = recipe.building,
            ));

            let mut inputs: Vec<_> = recipe.inputs.iter().collect();
            inputs.sort_by_key(|(k, _)| k.as_str());
            for (src, qty) in inputs {
                eid += 1;
                parts.push(format!(
                    r#"  {{"data":{{"id":"e{eid}","source":"{src}","target":"{tgt}","label":"{qty}"}}}}"#,
                    eid = eid, src = src, tgt = nid, qty = qty,
                ));
            }

            let mut outputs: Vec<_> = recipe.outputs.iter().collect();
            outputs.sort_by_key(|(k, _)| k.as_str());
            for (tgt, qty) in outputs {
                eid += 1;
                parts.push(format!(
                    r#"  {{"data":{{"id":"e{eid}","source":"{src}","target":"{tgt}","label":"{qty}"}}}}"#,
                    eid = eid, src = nid, tgt = tgt, qty = qty,
                ));
            }
        }

        format!("[\n{}\n]", parts.join(",\n"))
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
                let recipe = &self.recipes[recipe_id];
                if recipe.inputs.is_empty() {
                    // Gather recipe: resource is still a raw input, but record the recipe
                    // so its building is included in chain costs.
                    raw_inputs.insert(resource_id.to_string());
                    if !recipe_ids.contains(recipe_id) {
                        recipe_ids.push(recipe_id.clone());
                    }
                } else if !recipe_ids.contains(recipe_id) {
                    recipe_ids.push(recipe_id.clone());
                    let inputs: Vec<String> = recipe.inputs.keys().cloned().collect();
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
            if recipe.inputs.is_empty() {
                // Gather recipe: the resource is its own raw cost
                let mut m = HashMap::new();
                m.insert(resource_id.to_string(), 1.0);
                m
            } else {
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
        }
    };

    memo.insert(resource_id.to_string(), cost.clone());
    cost
}
