use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::env;
use std::fs::File;
use anyhow::{bail, Context};

use crate::generator::districts::{ParcelAnalysis, DistrictID};
use crate::generator::nbts::{Structure, StructureType};
use crate::minecraft::Biome;
use crate::noise::RNG;

use super::production_painter::{AnimalNamesFile, ProductionPainter, ProductionPaintersFile};
use super::types::{BiomeResourcesFile, ParcelResourceAssignment, RecipeDef, RecipesFile, ResourceDef, ResourcesFile};

/// A flat-terrain resource is dropped from a parcel's candidate set when
/// `ruggedness * flat_terrain_weight` exceeds this limit. It gates *validity* (whether
/// wheat is even an option on this parcel), not score — rougher terrain simply offers
/// fewer crop/pasture options, and a parcel left with no valid option is skipped.
/// At this limit (0.5): wheat (weight 1.0) drops once ruggedness passes 0.5; a more
/// tolerant pasture (weight 0.7) survives up to ruggedness ≈ 0.71.
const FLAT_TERRAIN_RUGGEDNESS_LIMIT: f32 = 0.5;

/// Roughness/gradient at or above which a parcel is treated as maximally rugged
/// (ruggedness saturates to 1.0). Roughness is aligned with the districts' off-limits
/// threshold (roughness 6.0) so only genuinely rough ground saturates; gradient stays
/// below its off-limits cap (1.0) because slope drives terraforming cost faster than
/// local roughness does.
const FLAT_TERRAIN_MAX_ROUGHNESS: f32 = 6.0;
const FLAT_TERRAIN_MAX_GRADIENT: f32 = 0.6;

/// Maximum fraction of a district's cells that may be water before it is denied a
/// resource assignment entirely. Water cells are excluded from a production area's
/// buildable cells (see `paint_production_area`), so a mostly-water district would
/// strand its gather building on a scrap of land beside the lake — the kind of
/// placement that looks like a building sitting on water. This is a touch more
/// lenient than the urban cutoff (0.33): a little shoreline water is fine for a
/// rural field, but a district that's mostly water gets no resource at all.
const MAX_RESOURCE_WATER: f32 = 0.40;

/// TEMPORARY (competition entry): hard caps on production variety, applied as a
/// post-processing step over the normal resolver output. Flip to `false` to restore
/// the full balancing system unchanged. While enabled:
///   - each gather resource is assigned to at most `MAX_RURAL_PER_RESOURCE` districts
///     (so e.g. at most two apiaries / two woodcutters across the map), and
///   - each city processing building appears at most `MAX_PROCESSING_PER_BUILDING`
///     times (one brewery, one chandlery, …), intermediate or finished alike.
/// See `resolve_for_parcels` for the two gated blocks that enforce these.
pub(crate) const COMPETITION_HARD_CAPS: bool = true;
pub(crate) const MAX_RURAL_PER_RESOURCE: usize = 2;
const MAX_PROCESSING_PER_BUILDING: u32 = 1;

/// Score penalty per prior assignment a candidate already has *beyond the least-used
/// option available to the same parcel*. Discourages over-representing one resource —
/// e.g. introducing a 3rd wheat when wool and cow each have only one — while never
/// penalising a parcel whose sole option is already common (a lone candidate is its own
/// minimum, so it takes zero balance penalty). Sized to dominate the jitter (0..99) and
/// value spread so a one-unit imbalance reliably tips the choice toward the rarer option.
const RESOURCE_BALANCE_PENALTY: i64 = 300;

/// Normalised ruggedness in `[0,1]` used to decide whether flat-terrain resources remain
/// valid options for a parcel. Combines roughness and gradient, taking whichever is worse,
/// each scaled against the point past which flattening a field becomes prohibitive.
pub fn parcel_ruggedness(analysis: &ParcelAnalysis) -> f32 {
    let roughness = analysis.roughness() / FLAT_TERRAIN_MAX_ROUGHNESS;
    let gradient = analysis.gradient() / FLAT_TERRAIN_MAX_GRADIENT;
    roughness.max(gradient).clamp(0.0, 1.0)
}

pub struct ChainSelection {
    /// The tier-2 good at the end of this chain.
    pub finished_good: String,
    /// Recipe IDs in topological order (raw inputs first, finished good last) — safe
    /// to iterate forward when executing recipe-by-recipe.
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
    /// Raw input cost to produce 1 unit of the finished good, resolved against the
    /// supply available when the chain was selected — so it matches the recipe variants
    /// actually chosen (e.g. the coal smelting path when no wood was gathered). May
    /// differ from the registry's canonical precomputed `raw_cost`.
    pub raw_cost: HashMap<String, f32>,
}

pub struct ProductionPlan {
    /// Chains ranked by depth DESC, raw_inputs.len() DESC, intermediates.len() DESC, then random.
    pub chains: Vec<ChainSelection>,
    /// `building_run_cost` aggregated across all chains — useful for checking which building
    /// types the plan requires. Multiply by units produced and ceil to get physical buildings.
    pub building_run_cost: HashMap<String, f32>,
}

/// Full production result for a settlement derived from a set of parcels.
pub struct SettlementProductionResult {
    /// The resource each parcel will gather and the building it needs to do so.
    pub parcel_assignments: HashMap<DistrictID, ParcelResourceAssignment>,
    /// Total raw resource supply available (resource → quantity, 2 per producing parcel).
    pub supply: HashMap<String, u32>,
    /// Finished goods produced after allocating supply across chains, in priority order.
    pub finished_goods: Vec<(String, u32)>,
    /// Raw and intermediate goods left over after chain allocation.
    pub leftover_goods: Vec<(String, u32)>,
    /// Gathering buildings required across all parcels (building type → count).
    pub gather_buildings: HashMap<String, u32>,
    /// Processing buildings required, scaled by units of finished goods produced.
    pub processing_buildings: HashMap<String, u32>,
    /// Districts dropped by the per-resource competition cap, kept as an ordered
    /// fallback list per gather resource (flattest/most-seatable first). When a
    /// primary `parcel_assignments` entry fails to physically place, the caller can
    /// promote the best dropped same-resource candidate instead of losing the
    /// building outright — preserving the planned economy without exceeding the cap.
    pub fallback_assignments: HashMap<String, Vec<(DistrictID, ParcelResourceAssignment)>>,
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
    /// painter name -> production painter definition
    pub production_painters: HashMap<String, ProductionPainter>,
    /// Names randomly assigned to animals spawned by the pasture/ranch painter.
    pub animal_names: Vec<String>,
    /// Optional decorative prefixes (e.g. "Ol'") prepended to a name ~10% of the time.
    pub animal_name_prefixes: Vec<String>,
    /// Optional decorative suffixes (e.g. "the Great") appended to a name ~10% of the time.
    pub animal_name_suffixes: Vec<String>,
    /// Funny names for bees placed inside beehives by the bee_area painter.
    pub bee_names: Vec<String>,
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
        let painters_file: ProductionPaintersFile = load_yaml(base.join("production_painters.yaml"))?;
        let animal_names: AnimalNamesFile = load_yaml(base.join("animal_names.yaml"))?;

        let mut registry = Self::from_parts(
            resources.resources,
            recipes.recipes,
            biome_resources.biome_resources,
        )?;
        registry.production_painters = painters_file.production_painters;
        registry.animal_names = animal_names.animal_names;
        registry.animal_name_prefixes = animal_names.name_prefixes;
        registry.animal_name_suffixes = animal_names.name_suffixes;
        registry.bee_names = animal_names.bee_names;
        registry.validate_painters()?;
        Ok(registry)
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
            production_painters: HashMap::new(),
            animal_names: Vec::new(),
            animal_name_prefixes: Vec::new(),
            animal_name_suffixes: Vec::new(),
            bee_names: Vec::new(),
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

    /// Verifies that every gather recipe's `production_painter` name exists in the
    /// loaded painters map. Called from `load()` after painters are populated.
    pub fn validate_painters(&self) -> anyhow::Result<()> {
        let mut missing: Vec<String> = Vec::new();
        for recipe in self.recipes.values() {
            if let Some(painter_name) = &recipe.production_painter {
                if !self.production_painters.contains_key(painter_name.as_str()) {
                    missing.push(painter_name.clone());
                }
            }
        }
        missing.sort();
        missing.dedup();
        if !missing.is_empty() {
            bail!(
                "Resource chain painter validation failed. Missing painters in production_painters.yaml: {}",
                missing.join(", ")
            );
        }
        Ok(())
    }

    /// Returns the full gather recipe for a raw resource — the recipe that produces
    /// it with no inputs. Returns `None` if no such recipe exists.
    fn gather_recipe(&self, resource_id: &str) -> Option<&RecipeDef> {
        self.produced_by.get(resource_id)?
            .iter()
            .find(|recipe_id| self.recipes[*recipe_id].inputs.is_empty())
            .map(|recipe_id| &self.recipes[recipe_id])
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
                let chain = self.trace_chain(id, &resolved.producible);
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

    /// Traces the full dependency chain for a finished good, collecting all recipe IDs
    /// and raw inputs recursively. Recipe variants are chosen against `producible` (the
    /// supply-reachable set), so a good resolves through whichever recipe its inputs can
    /// actually satisfy — e.g. the coal smelting path when no wood was gathered.
    fn trace_chain(&self, finished_good: &str, producible: &HashSet<String>) -> ChainSelection {
        let mut recipe_ids: Vec<String> = Vec::new();
        let mut raw_inputs: HashSet<String> = HashSet::new();

        self.trace_resource(finished_good, producible, &mut recipe_ids, &mut raw_inputs);

        // Compute buildings needed per 1 unit of finished good, accounting for
        // recipe batch sizes (e.g. 1 arrow run → 8 arrows = 0.125 buildings per arrow).
        let mut recipe_runs: HashMap<String, f32> = HashMap::new();
        self.compute_recipe_runs(finished_good, 1.0, producible, &mut recipe_runs);

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

        // Cost resolved through the same producible-aware recipe choices as the chain
        // above, so `execute_chain`'s budget matches the recipes it will actually fire.
        let raw_cost = self.chain_raw_cost(finished_good, producible);

        ChainSelection {
            finished_good: finished_good.to_string(),
            recipe_ids,
            building_run_cost,
            raw_inputs,
            intermediates,
            depth,
            raw_cost,
        }
    }

    /// Recursively computes how many times each recipe must run to produce
    /// `needed_per_unit` of `resource_id` (relative to 1 unit of the original finished good).
    fn compute_recipe_runs(
        &self,
        resource_id: &str,
        needed_per_unit: f32,
        producible: &HashSet<String>,
        recipe_runs: &mut HashMap<String, f32>,
    ) {
        let Some(recipe_id) = self.choose_producer(resource_id, producible) else {
            return; // raw input — no recipe
        };
        let recipe = &self.recipes[recipe_id];
        let output_qty = recipe.outputs[resource_id] as f32;
        let runs_per_unit = needed_per_unit / output_qty;
        *recipe_runs.entry(recipe_id.clone()).or_insert(0.0) += runs_per_unit;
        for (input_id, &input_qty) in &recipe.inputs {
            self.compute_recipe_runs(input_id, runs_per_unit * input_qty as f32, producible, recipe_runs);
        }
    }

    /// Given a map of parcel analyses, resolves the full production picture for the settlement:
    /// selects one raw resource per parcel (based on major biomes), allocates supply across
    /// production chains, and returns per-parcel building assignments alongside the complete
    /// building and goods summary.
    pub fn resolve_for_parcels(
        &self,
        parcel_analysis: &HashMap<DistrictID, ParcelAnalysis>,
        rng: &mut RNG,
    ) -> SettlementProductionResult {
        self.resolve_for_parcels_seated(parcel_analysis, None, rng)
    }

    /// As [`resolve_for_parcels`], but additionally honours a per-parcel
    /// *seatability* constraint: `seatable[id]` is the set of resources whose
    /// gather building footprint can physically fit in district `id` under the
    /// placement slope cap (computed by `placement::district_seatable_footprints`).
    /// A resource absent from a parcel's set is excluded from that parcel exactly
    /// like a terrain exclusion, so the planned economy never advertises a gather
    /// building that placement would later drop for want of a flat pad. `None`
    /// disables the constraint (all resources seatable everywhere).
    pub fn resolve_for_parcels_seated(
        &self,
        parcel_analysis: &HashMap<DistrictID, ParcelAnalysis>,
        seatable: Option<&HashMap<DistrictID, HashSet<String>>>,
        rng: &mut RNG,
    ) -> SettlementProductionResult {
        // caps to prevent extreme overproduction of certain goods that would skew the settlement's production profile
        const RAW_SURPLUS_CAP: u32 = 5;
        const INTERMEDIATE_CAP: u32 = 10;
        const FINISHED_GOOD_CAP: u32 = 15;

        // Build candidate resource lists from each parcel's major biomes (≥30%).
        // Districts that are mostly water are given no candidates at all: their
        // buildable land is too sparse/fragmented to host a gather building and its
        // production area without stranding it on the water's edge.
        let base_options: HashMap<DistrictID, Vec<String>> = parcel_analysis.iter()
            .map(|(id, analysis)| {
                if analysis.water_percentage() > MAX_RESOURCE_WATER {
                    log::info!(
                        "[resource-chain]   parcel {:>3} -> (none) — too much water ({:.0}% > {:.0}%)",
                        id.0, analysis.water_percentage() * 100.0, MAX_RESOURCE_WATER * 100.0,
                    );
                    return (*id, Vec::new());
                }
                let mut candidates: Vec<String> = analysis.major_biomes().iter()
                    .flat_map(|biome| self.resources_for_biome(biome))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                candidates.sort();
                (*id, candidates)
            })
            .collect();

        // Per-parcel ruggedness, used to steer flat-terrain resources (crops/pastures)
        // away from rough or steep parcels we'd otherwise have to terraform heavily.
        let ruggedness: HashMap<DistrictID, f32> = parcel_analysis.iter()
            .map(|(id, analysis)| (*id, parcel_ruggedness(analysis)))
            .collect();

        // Log the terrain inputs to the flat-terrain penalty so a test run can measure its
        // impact. Sorted by district id for stable diffing across runs.
        let mut rugged_rows: Vec<(&DistrictID, &ParcelAnalysis, f32)> = parcel_analysis.iter()
            .map(|(id, a)| (id, a, ruggedness[id]))
            .collect();
        rugged_rows.sort_by_key(|(id, _, _)| id.0);
        for (id, a, rugged) in &rugged_rows {
            log::info!(
                "[resource-chain]   ruggedness parcel {:>3} -> {:.3} (roughness {:.2} / {:.1}, gradient {:.2} / {:.1})",
                id.0, rugged, a.roughness(), FLAT_TERRAIN_MAX_ROUGHNESS, a.gradient(), FLAT_TERRAIN_MAX_GRADIENT,
            );
        }

        // Iteratively cap over-supplied resources: run a trial assignment, simulate the full
        // production pipeline, and remove resources that would generate excess goods.
        //   - raw resource surplus  ≥ RAW_SURPLUS_CAP  → cap that raw resource directly
        //   - finished good output  ≥ FINISHED_GOOD_CAP → cap all raw inputs feeding that good
        //   - intermediate leftover ≥ INTERMEDIATE_CAP  → cap all raw inputs feeding that good
        // Repeat until stable (up to 10 iterations).
        // Iteratively limit how many parcels may gather each over-supplied resource.
        // Banning a resource outright cascades: the survivors absorb its freed parcels,
        // overshoot in turn, and get banned too, until only the least-efficient chain
        // survives (e.g. plains collapsing entirely to wool). Instead we cap the *number
        // of parcels* assigned to each resource, never below 1, so every biome-supported
        // resource keeps at least one parcel while overproduction is still reined in.
        let mut quota: HashMap<String, usize> = HashMap::new();
        for _ in 0..10 {
            let trial_assignments = self.assign_parcel_resources_quota(&base_options, &quota, &ruggedness, seatable, false, &mut rng.derive());

            // Compute trial supply, crediting all gather recipe outputs (handles multi-output
            // recipes like gather_bees which produces both honey and beeswax).
            let mut trial_supply: HashMap<String, u32> = HashMap::new();
            for (_, a) in &trial_assignments {
                self.credit_gather_supply(&a.primary_resource, &mut trial_supply);
            }

            // How many parcels each resource currently occupies — the basis for scaling quotas.
            let mut trial_counts: HashMap<String, usize> = HashMap::new();
            for a in trial_assignments.values() {
                *trial_counts.entry(a.primary_resource.clone()).or_insert(0) += 1;
            }

            // Run chain allocation on trial supply via the same forward-execution model
            // used for the real pass, so cap decisions reflect actual production output.
            let trial_available: HashSet<String> = trial_supply.keys().cloned().collect();
            let trial_plan = self.select_production(&trial_available, &mut rng.derive());
            let mut trial_remaining: HashMap<String, f32> = trial_supply.iter()
                .map(|(k, &v)| (k.clone(), v as f32))
                .collect();
            let mut trial_finished: Vec<(String, u32)> = Vec::new();
            let mut trial_intermediate_pool: HashMap<String, u32> = HashMap::new();

            for chain in &trial_plan.chains {
                let (finished_qty, intermediates, _) = self.execute_chain(chain, &mut trial_remaining);
                if finished_qty > 0 {
                    trial_finished.push((chain.finished_good.clone(), finished_qty));
                }
                for (id, q) in intermediates {
                    *trial_intermediate_pool.entry(id).or_insert(0) += q;
                }
            }

            let trial_intermediates: Vec<(String, u32)> = trial_intermediate_pool.into_iter()
                .filter(|(_, q)| *q > 0)
                .collect();

            let mut changed = false;

            // Finished good ≥ cap or intermediate leftover ≥ cap → scale down the parcel
            // count of each raw input feeding it so output lands near the cap (floored at 1).
            let mut over_outputs: Vec<(String, u32, u32)> = Vec::new();
            for (good, units) in &trial_finished {
                if *units >= FINISHED_GOOD_CAP { over_outputs.push((good.clone(), *units, FINISHED_GOOD_CAP)); }
            }
            for (interm, units) in &trial_intermediates {
                if *units >= INTERMEDIATE_CAP { over_outputs.push((interm.clone(), *units, INTERMEDIATE_CAP)); }
            }
            for (good, units, cap) in over_outputs {
                let Some(cost) = self.raw_cost(&good) else { continue };
                for raw in cost.keys() {
                    let parcels_now = trial_counts.get(raw).copied().unwrap_or(0);
                    if parcels_now == 0 { continue; }
                    let target = (((parcels_now as f32) * (cap as f32) / (units as f32)).floor() as usize).max(1);
                    let existing = quota.get(raw).copied().unwrap_or(usize::MAX);
                    if target < existing {
                        quota.insert(raw.clone(), target);
                        changed = true;
                    }
                }
            }

            // Raw surplus ≥ cap → trim that resource's parcel count by one (floored at 1).
            for (r, &qty) in &trial_remaining {
                if qty < RAW_SURPLUS_CAP as f32 { continue; }
                let parcels_now = trial_counts.get(r).copied().unwrap_or(0);
                if parcels_now <= 1 { continue; }
                let target = parcels_now - 1;
                let existing = quota.get(r).copied().unwrap_or(usize::MAX);
                if target < existing {
                    quota.insert(r.clone(), target);
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        // Each super-parcel is assigned exactly one gather recipe (identified by its
        // primary resource), respecting the per-resource parcel quotas computed above.
        // The primary output is credited at 2 units; co-products from multi-output recipes
        // (e.g. gather_bees → honey + beeswax) are scaled proportionally.
        let mut parcel_assignments = self.assign_parcel_resources_quota(&base_options, &quota, &ruggedness, seatable, true, rng);

        // Competition cap: keep at most MAX_RURAL_PER_RESOURCE districts per gather
        // resource. Applied here, before supply/buildings/plan are derived, so the rest
        // of the pipeline stays consistent with the trimmed rural set. Deterministic:
        // for each over-represented resource we keep the lowest-ID districts and drop
        // the excess (those districts simply go unproduced). This is a *hard* cap the
        // quota system alone can't guarantee, since its diversity floor may overshoot.
        let mut fallback_assignments: HashMap<String, Vec<(DistrictID, ParcelResourceAssignment)>> = HashMap::new();
        if COMPETITION_HARD_CAPS {
            let mut by_resource: HashMap<String, Vec<DistrictID>> = HashMap::new();
            for (id, a) in &parcel_assignments {
                by_resource.entry(a.primary_resource.clone()).or_default().push(*id);
            }
            for (resource, mut ids) in by_resource {
                if ids.len() <= MAX_RURAL_PER_RESOURCE {
                    continue;
                }
                ids.sort_by_key(|id| id.0);
                for drop_id in ids.into_iter().skip(MAX_RURAL_PER_RESOURCE) {
                    // Remove from the active plan, but retain it as a placement fallback
                    // for this resource rather than discarding it entirely.
                    if let Some(assignment) = parcel_assignments.remove(&drop_id) {
                        fallback_assignments.entry(resource.clone()).or_default().push((drop_id, assignment));
                    }
                    log::info!(
                        "[resource-chain]   competition cap: dropped {:?} (resource {}) — over {} per resource (kept as fallback)",
                        drop_id, resource, MAX_RURAL_PER_RESOURCE,
                    );
                }
            }
            // Order each resource's fallbacks flattest-first, so the caller promotes the
            // most placeable dropped parcel when a primary fails to seat.
            for fallbacks in fallback_assignments.values_mut() {
                fallbacks.sort_by(|(a, _), (b, _)| {
                    let ra = ruggedness.get(a).copied().unwrap_or(1.0);
                    let rb = ruggedness.get(b).copied().unwrap_or(1.0);
                    ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
                });
            }
        }

        let mut supply: HashMap<String, u32> = HashMap::new();
        let mut gather_buildings: HashMap<String, u32> = HashMap::new();
        for (_, a) in &parcel_assignments {
            self.credit_gather_supply(&a.primary_resource, &mut supply);
            *gather_buildings.entry(a.building.clone()).or_insert(0) += 1;
        }

        let available: HashSet<String> = supply.keys().cloned().collect();
        let resolved = self.resolve(&available);
        let plan = self.select_production(&available, rng);

        // Allocate supply across chains (highest priority first). Each chain gets a
        // proportional raw budget based on `raw_cost`, then forward-executes its recipes:
        // every building consumes input as floats but produces FLOORED integer output, so
        // any fractional output (e.g. 2.5 of a resource) is truncated to 2 — the rest is lost
        // and cannot flow to the next building.
        let mut remaining: HashMap<String, f32> = supply.iter()
            .map(|(k, &v)| (k.clone(), v as f32))
            .collect();
        let mut finished_goods: Vec<(String, u32)> = Vec::new();
        let mut processing_buildings: HashMap<String, u32> = HashMap::new();
        let mut intermediate_pool: HashMap<String, u32> = HashMap::new();

        for chain in &plan.chains {
            let (finished_qty, intermediates, buildings) = self.execute_chain(chain, &mut remaining);
            if finished_qty > 0 {
                finished_goods.push((chain.finished_good.clone(), finished_qty));
            }
            for (b, c) in buildings {
                if gather_buildings.contains_key(&b) { continue; }
                *processing_buildings.entry(b).or_insert(0) += c;
            }
            for (id, q) in intermediates {
                *intermediate_pool.entry(id).or_insert(0) += q;
            }
        }
        let _ = resolved; // intermediate leftovers now come from forward execution, not the raw-cost projection

        // Competition cap: at most MAX_PROCESSING_PER_BUILDING of each city processing
        // building (intermediate or finished alike — each building type is its own key).
        // Clamps the placement-driving counts only; the finished/leftover good totals
        // above still reflect the full chain math, which is fine for reporting.
        if COMPETITION_HARD_CAPS {
            for count in processing_buildings.values_mut() {
                *count = (*count).min(MAX_PROCESSING_PER_BUILDING);
            }
        }

        // Leftover raw goods (tier 0).
        let mut leftover_goods: Vec<(String, u32)> = self.resources.iter()
            .filter(|(_, def)| def.tier == 0)
            .filter_map(|(id, _)| {
                let qty = remaining.get(id).copied().unwrap_or(0.0).floor() as u32;
                if qty == 0 { None } else { Some((id.clone(), qty)) }
            })
            .collect();
        leftover_goods.sort_by_key(|(r, _)| r.clone());

        // Intermediate goods (tier 1) that piled up because downstream recipes couldn't
        // consume them — surfaced as leftover production.
        let mut intermediate_leftovers: Vec<(String, u32)> = intermediate_pool.into_iter()
            .filter(|(_, q)| *q > 0)
            .collect();
        intermediate_leftovers.sort_by_key(|(r, _)| r.clone());
        leftover_goods.extend(intermediate_leftovers);

        let result = SettlementProductionResult {
            parcel_assignments,
            supply,
            finished_goods,
            leftover_goods,
            gather_buildings,
            processing_buildings,
            fallback_assignments,
        };

        // Log the full chain so multiple generation runs can be compared via the logs.
        // Each section is prefixed with `[resource-chain]` for easy grepping/filtering.
        log_settlement_production(&result, parcel_analysis.len());

        result
    }

    /// For each entry in `options` (a parcel ID paired with its candidate raw resources from
    /// its biome), selects exactly one raw resource per parcel and returns the corresponding
    /// gathering building.
    ///
    /// **Selection strategy** — a parcel's candidates are first restricted to the ones its
    /// terrain can host (flat-terrain crops/pastures drop out on rough ground), then the
    /// most-constrained parcels (fewest valid candidates) are resolved first. For each
    /// parcel the surviving candidates are scored by:
    ///   1. Novelty — +1000 if the resource has not yet been assigned to any other parcel,
    ///      encouraging diversity across the settlement.
    ///   2. Value — the number of tier-2 finished goods that require this resource somewhere
    ///      in their production chain (higher = feeds more goods).
    ///   3. Jitter — a small random nudge so repeated calls with the same biome mix produce
    ///      varied towns.
    ///   4. Balance penalty — a candidate is docked for each prior assignment it has beyond
    ///      the rarest option available to this parcel, discouraging over-representation
    ///      (see [`assign_parcel_resources_quota`]).
    ///
    /// Parcels whose candidates have no known gather recipe — or whose options are all ruled
    /// out by terrain — are skipped, so a parcel receives *at most* one assignment, not
    /// always one. Input is a `HashMap` so each ID is guaranteed to appear at most once.
    pub fn assign_parcel_resources<ID: Eq + Hash + Clone + std::fmt::Debug>(
        &self,
        options: &HashMap<ID, Vec<String>>,
        rng: &mut RNG,
    ) -> HashMap<ID, ParcelResourceAssignment> {
        self.assign_parcel_resources_quota(options, &HashMap::new(), &HashMap::new(), None, false, rng)
    }

    /// Like [`assign_parcel_resources`], but caps how many parcels may be assigned each
    /// resource. `max_parcels` maps resource → maximum parcel count (absent = unlimited).
    ///
    /// When every candidate a parcel could take has already hit its quota, the quota is
    /// ignored *for that parcel* so it still receives an assignment — this is the
    /// diversity floor that prevents an over-supplied resource from being eliminated
    /// entirely (see the cap loop in `resolve_for_parcels`).
    ///
    /// `ruggedness` maps each parcel to its normalised terrain ruggedness in `[0,1]`
    /// (absent = 0.0). It gates *which resources are valid* for a parcel rather than scoring
    /// them: flat-terrain resources (crops/pastures) are removed from the candidate set once
    /// the parcel is too rough for them (see `terrain_allows`). A parcel left with no valid
    /// option — e.g. an all-flat-terrain biome on very rough ground — is skipped, so some
    /// rural districts may end up with no resource at all.
    ///
    /// When `log_decisions` is set, each parcel's outcome — the resources terrain ruled out,
    /// the chosen resource, and any over-representation balance penalties — is logged at
    /// `info` so a test run can measure the impact. The trial passes in `resolve_for_parcels`
    /// leave it off to avoid 10× duplicate noise.
    fn assign_parcel_resources_quota<ID: Eq + Hash + Clone + std::fmt::Debug>(
        &self,
        options: &HashMap<ID, Vec<String>>,
        max_parcels: &HashMap<String, usize>,
        ruggedness: &HashMap<ID, f32>,
        seatable: Option<&HashMap<ID, HashSet<String>>>,
        log_decisions: bool,
        rng: &mut RNG,
    ) -> HashMap<ID, ParcelResourceAssignment> {
        // Filter each parcel's candidates to those with a known gather building, whose
        // flat-terrain requirement the parcel's terrain can satisfy (crops/pastures drop
        // out on rough ground — see `terrain_allows`), AND whose building footprint can
        // actually be seated in the parcel (`seatable`, when supplied — so we never plan a
        // building placement can't physically fit). We keep the rejected-for-terrain and
        // rejected-for-footprint lists per parcel only so the decision log can show what
        // each filter ruled out. Then sort most-constrained first.
        let mut parcels: Vec<(ID, Vec<String>, Vec<String>, Vec<String>)> = options.iter()
            .map(|(id, candidates)| {
                let rugged = ruggedness.get(id).copied().unwrap_or(0.0);
                let gatherable = candidates.iter()
                    .filter(|r| self.gather_building(r).is_some());
                let (terrain_ok, excluded_terrain): (Vec<String>, Vec<String>) = gatherable
                    .cloned()
                    .partition(|r| self.terrain_allows(r, rugged));
                // A missing parcel entry means "unknown — allow"; only an explicit set
                // that omits the resource counts as not-seatable. With `seatable = None`
                // every resource passes, preserving the unconstrained behaviour.
                let seatable_here = seatable.and_then(|m| m.get(id));
                let (valid, excluded_footprint): (Vec<String>, Vec<String>) = terrain_ok
                    .into_iter()
                    .partition(|r| seatable_here.map_or(true, |s| s.contains(r)));
                (id.clone(), valid, excluded_terrain, excluded_footprint)
            })
            .collect();
        parcels.sort_by_key(|(_, candidates, _, _)| candidates.len());

        let mut assigned_set: HashSet<String> = HashSet::new();
        let mut assigned_count: HashMap<String, usize> = HashMap::new();
        let mut result: HashMap<ID, ParcelResourceAssignment> = HashMap::new();

        for (id, candidates, excluded_terrain, excluded_footprint) in parcels {
            let parcel_rugged = ruggedness.get(&id).copied().unwrap_or(0.0);
            let excluded_str = if excluded_terrain.is_empty() { "none".to_string() } else { excluded_terrain.join(", ") };
            let footprint_str = if excluded_footprint.is_empty() { "none".to_string() } else { excluded_footprint.join(", ") };

            if candidates.is_empty() {
                // No option survives (no gather building, or terrain/footprint ruled them
                // all out) — leave this rural district unproduced.
                if log_decisions {
                    log::info!(
                        "[resource-chain]   pick parcel {:?} -> (none) (ruggedness {:.3}); terrain-excluded: {}; footprint-excluded: {}",
                        id, parcel_rugged, excluded_str, footprint_str,
                    );
                }
                continue;
            }

            // Prefer candidates still under their quota; only if all are exhausted do we
            // fall back to the full candidate list, so the quota alone never starves a
            // parcel of its (terrain-valid) options.
            let under_quota: Vec<&String> = candidates.iter()
                .filter(|r| {
                    let used = assigned_count.get(*r).copied().unwrap_or(0);
                    let cap = max_parcels.get(*r).copied().unwrap_or(usize::MAX);
                    used < cap
                })
                .collect();
            let pool: Vec<&String> = if under_quota.is_empty() {
                candidates.iter().collect()
            } else {
                under_quota
            };

            // Least-used count among this parcel's options — the baseline the balance
            // penalty is measured against, so a candidate is only docked for being *more*
            // common than the rarest alternative available here.
            let min_count = pool.iter()
                .map(|r| assigned_count.get(*r).copied().unwrap_or(0))
                .min()
                .unwrap_or(0);

            // Pre-score all candidates so rng is called once per candidate.
            // Each tuple is (resource, total_score, balance_penalty) — the penalty is kept
            // separately so it can be surfaced in the decision log.
            let scored: Vec<(String, i64, i64)> = pool.iter()
                .map(|r| {
                    let value = self.resource_value_score(r) as i64;
                    let novelty: i64 = if !assigned_set.contains(*r) { 1000 } else { 0 };
                    let jitter = rng.rand_i32(100) as i64;
                    let count = assigned_count.get(*r).copied().unwrap_or(0);
                    let balance_penalty = -((count.saturating_sub(min_count) as i64) * RESOURCE_BALANCE_PENALTY);
                    ((*r).clone(), novelty + value + jitter + balance_penalty, balance_penalty)
                })
                .collect();

            let (resource, winning_score, _) = scored.iter()
                .max_by_key(|(_, score, _)| *score)
                .cloned()
                .unwrap();

            // Defensive: a sub-zero best score means nothing here is worth gathering — skip.
            let skipped = winning_score < 0;

            if log_decisions {
                // Candidates docked for over-representation, so the balance pressure is visible.
                let mut penalised: Vec<String> = scored.iter()
                    .filter(|(_, _, penalty)| *penalty != 0)
                    .map(|(r, _, penalty)| format!("{} ({})", r, penalty))
                    .collect();
                penalised.sort();
                let penalised = if penalised.is_empty() { "none".to_string() } else { penalised.join(", ") };
                let outcome = if skipped { format!("(none — best was {} at {})", resource, winning_score) }
                              else { format!("{} (score {})", resource, winning_score) };
                log::info!(
                    "[resource-chain]   pick parcel {:?} -> {} (ruggedness {:.3}); terrain-excluded: {}; footprint-excluded: {}; balance penalties: {}",
                    id, outcome, parcel_rugged, excluded_str, footprint_str, penalised,
                );
            }

            if skipped {
                continue;
            }

            let recipe = self.gather_recipe(&resource).unwrap();
            let building = recipe.building.clone();
            let production_painter = recipe.production_painter.clone();
            assigned_set.insert(resource.clone());
            *assigned_count.entry(resource.clone()).or_insert(0) += 1;
            result.insert(id, ParcelResourceAssignment { primary_resource: resource, building, production_painter });
        }

        result
    }

    /// Credits all outputs of the gather recipe for `primary_resource` into `supply`.
    /// The primary output is credited at 2 units (the fixed per-parcel convention).
    /// Co-products are scaled proportionally: `(co_qty / primary_qty) * 2`, rounded.
    /// Falls back to crediting only the primary at +2 if the recipe is not found.
    fn credit_gather_supply(&self, primary_resource: &str, supply: &mut HashMap<String, u32>) {
        let Some(recipe) = self.gather_recipe(primary_resource) else {
            *supply.entry(primary_resource.to_string()).or_insert(0) += 2;
            return;
        };
        let primary_qty = recipe.outputs.get(primary_resource).copied().unwrap_or(1) as f32;
        for (output, &qty) in &recipe.outputs {
            let scaled = ((qty as f32 / primary_qty) * 2.0).round() as u32;
            if scaled > 0 {
                *supply.entry(output.clone()).or_insert(0) += scaled;
            }
        }
    }

    /// Returns the gathering building for a raw resource — the recipe that produces
    /// it with no inputs. Returns `None` if no such recipe exists.
    fn gather_building(&self, resource_id: &str) -> Option<&str> {
        self.gather_recipe(resource_id).map(|r| r.building.as_str())
    }

    /// How strongly a raw resource wants flat terrain, in `[0,1]` (see
    /// `ResourceDef::flat_terrain`). Higher values mean crops/pastures we'd rather not
    /// place on rough ground. Unknown resources are treated as terrain-agnostic (0.0).
    fn flat_terrain_weight(&self, resource_id: &str) -> f32 {
        self.resources.get(resource_id).map(|r| r.flat_terrain).unwrap_or(0.0)
    }

    /// Whether a parcel of the given `ruggedness` (`[0,1]`) can host this resource.
    /// Terrain-agnostic resources (weight 0) are always allowed; flat-terrain resources
    /// drop out once `ruggedness * weight` passes `FLAT_TERRAIN_RUGGEDNESS_LIMIT`, so
    /// rougher ground simply offers fewer crop/pasture options.
    fn terrain_allows(&self, resource_id: &str, ruggedness: f32) -> bool {
        ruggedness * self.flat_terrain_weight(resource_id) <= FLAT_TERRAIN_RUGGEDNESS_LIMIT
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

    /// Runs a single chain forward, allocating a proportional slice of `remaining` raw
    /// supply to this chain and then firing each recipe in topological order.
    /// Output is FLOORED at every recipe so fractional outputs (e.g. 2.5 → 2) cannot flow
    /// downstream — the discarded fraction is lost. Returns the finished-good quantity,
    /// any tier-1 intermediates left over after the chain ran, and the building counts
    /// (ceil of fractional batches per recipe).
    fn execute_chain(
        &self,
        chain: &ChainSelection,
        remaining: &mut HashMap<String, f32>,
    ) -> (u32, HashMap<String, u32>, HashMap<String, u32>) {
        // Use the chain's own availability-aware cost (matching the recipe variants it
        // will fire below), not the registry's canonical first-recipe cost.
        if chain.raw_cost.is_empty() {
            return (0, HashMap::new(), HashMap::new());
        }
        let cost = chain.raw_cost.clone();

        // Fractional finished-good units the chain's raw inputs support.
        let raw_units_f = cost.iter()
            .map(|(raw, per)| remaining.get(raw).copied().unwrap_or(0.0) / per)
            .fold(f32::INFINITY, f32::min);
        if !raw_units_f.is_finite() || raw_units_f <= 0.0 {
            return (0, HashMap::new(), HashMap::new());
        }

        // Carve the chain's raw allocation out of the shared `remaining` pool.
        let mut local: HashMap<String, f32> = cost.iter()
            .map(|(raw, per)| (raw.clone(), per * raw_units_f))
            .collect();
        for (raw, per) in &cost {
            *remaining.entry(raw.clone()).or_insert(0.0) -= per * raw_units_f;
        }

        // Fire each recipe in topological order. Inputs deplete fractionally,
        // outputs are floored before being added to local stock.
        let mut buildings: HashMap<String, u32> = HashMap::new();
        for recipe_id in &chain.recipe_ids {
            let recipe = &self.recipes[recipe_id];
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

            *buildings.entry(recipe.building.clone()).or_insert(0) += batches.ceil() as u32;
        }

        let finished_qty = local.get(&chain.finished_good).copied().unwrap_or(0.0).floor() as u32;

        // Tier-1 stock that survived the chain run = intermediate leftover.
        let mut intermediates: HashMap<String, u32> = HashMap::new();
        for (id, qty) in &local {
            if id == &chain.finished_good { continue; }
            if self.resources.get(id).map(|r| r.tier == 1).unwrap_or(false) {
                let units = qty.floor() as u32;
                if units > 0 {
                    intermediates.insert(id.clone(), units);
                }
            }
        }

        (finished_qty, intermediates, buildings)
    }

    /// Post-order DFS walk backwards from a resource, accumulating recipe IDs in
    /// topological order (raw inputs first, finished good last) and recording the
    /// raw inputs touched. Inputs are visited in sorted order so the resulting
    /// order is deterministic across HashMap iterations.
    fn trace_resource(
        &self,
        resource_id: &str,
        producible: &HashSet<String>,
        recipe_ids: &mut Vec<String>,
        raw_inputs: &mut HashSet<String>,
    ) {
        // Pick the producer whose inputs are actually reachable from the current supply.
        match self.choose_producer(resource_id, producible) {
            None => {
                raw_inputs.insert(resource_id.to_string());
            }
            Some(recipe_id) => {
                if recipe_ids.contains(recipe_id) {
                    return;
                }
                let recipe = &self.recipes[recipe_id];
                if recipe.inputs.is_empty() {
                    raw_inputs.insert(resource_id.to_string());
                    recipe_ids.push(recipe_id.clone());
                } else {
                    let mut inputs: Vec<String> = recipe.inputs.keys().cloned().collect();
                    inputs.sort();
                    for input in inputs {
                        self.trace_resource(&input, producible, recipe_ids, raw_inputs);
                    }
                    recipe_ids.push(recipe_id.clone());
                }
            }
        }
    }

    /// Among the recipes that produce `resource_id`, returns the first (in deterministic
    /// sorted order) whose inputs are *all* in `producible`, falling back to the first
    /// producer if none qualify, and `None` if the resource has no producer (a raw input).
    ///
    /// This is what makes chain construction availability-aware: `iron_ingot` resolves
    /// through the coal recipe when wood — and therefore charcoal — was never gathered,
    /// instead of rigidly taking the alphabetically-first charcoal recipe and stranding
    /// the mined iron and coal. Gather recipes have no inputs, so they always qualify.
    fn choose_producer<'a>(&'a self, resource_id: &str, producible: &HashSet<String>) -> Option<&'a String> {
        let producers = self.produced_by.get(resource_id)?;
        producers.iter()
            .find(|rid| self.recipes[*rid].inputs.keys().all(|i| producible.contains(i)))
            .or_else(|| producers.first())
    }

    /// Availability-aware raw cost: like the registry's precomputed `raw_cost`, but
    /// resolves recipe variants against `producible` (via [`choose_producer`]) so the
    /// cost matches the chain `trace_chain` actually builds. Used for budget allocation
    /// in `execute_chain`.
    fn chain_raw_cost(&self, resource_id: &str, producible: &HashSet<String>) -> HashMap<String, f32> {
        match self.choose_producer(resource_id, producible) {
            // Raw input (no producer) or gather recipe (no inputs): cost is itself.
            None => HashMap::from([(resource_id.to_string(), 1.0)]),
            Some(recipe_id) if self.recipes[recipe_id].inputs.is_empty() => {
                HashMap::from([(resource_id.to_string(), 1.0)])
            }
            Some(recipe_id) => {
                let recipe = &self.recipes[recipe_id];
                let output_qty = recipe.outputs[resource_id] as f32;
                let mut total: HashMap<String, f32> = HashMap::new();
                for (input_id, &input_qty) in &recipe.inputs {
                    let scale = input_qty as f32 / output_qty;
                    for (raw, qty) in self.chain_raw_cost(input_id, producible) {
                        *total.entry(raw).or_insert(0.0) += qty * scale;
                    }
                }
                total
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

/// Emit a full, sorted dump of a settlement's production result to the log so that
/// multiple generation runs can be compared after the fact. Every line is prefixed
/// with `[resource-chain]` for easy `grep`/log-filter analysis. Logged at `info`.
fn log_settlement_production(result: &SettlementProductionResult, total_parcels: usize) {
    log::info!(
        "[resource-chain] settlement production: {} producing parcels of {} total",
        result.parcel_assignments.len(), total_parcels,
    );

    // Per-parcel assignments, sorted by district id for stable diffing across runs.
    let mut assignments: Vec<(&DistrictID, &ParcelResourceAssignment)> =
        result.parcel_assignments.iter().collect();
    assignments.sort_by_key(|(id, _)| id.0);
    for (id, a) in assignments {
        log::info!(
            "[resource-chain]   parcel {:>3} -> {} [{}]",
            id.0, a.primary_resource, a.building,
        );
    }

    let log_counts = |label: &str, map: &HashMap<String, u32>| {
        let mut items: Vec<(&String, &u32)> = map.iter().collect();
        items.sort_by(|(an, ac), (bn, bc)| bc.cmp(ac).then(an.cmp(bn)));
        if items.is_empty() {
            log::info!("[resource-chain]   {}: (none)", label);
        }
        for (name, count) in items {
            log::info!("[resource-chain]   {}: {} x{}", label, name, count);
        }
    };
    let log_pairs = |label: &str, pairs: &[(String, u32)]| {
        let mut items: Vec<&(String, u32)> = pairs.iter().collect();
        items.sort_by(|(an, ac), (bn, bc)| bc.cmp(ac).then(an.cmp(bn)));
        if items.is_empty() {
            log::info!("[resource-chain]   {}: (none)", label);
        }
        for (name, count) in items {
            log::info!("[resource-chain]   {}: {} x{}", label, name, count);
        }
    };

    log_counts("supply", &result.supply);
    log_pairs("finished_good", &result.finished_goods);
    log_pairs("leftover", &result.leftover_goods);
    log_counts("gather_building", &result.gather_buildings);
    log_counts("processing_building", &result.processing_buildings);
}
