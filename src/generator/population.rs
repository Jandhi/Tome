//! NPC population: turn a town into a roster of residents, collect placement
//! anchors emitted while the town is built, and distribute the roster across
//! them.
//!
//! The placement unit is an [`AnchorScene`] — a group of [`AnchorSlot`]s filled
//! together. v1 only emits **solo** scenes (one slot), but the model is built
//! scene-ready so multi-person scenes (a conversation, a haggle) drop in later
//! without reshaping the pipeline: a scene's slots carry their own baked facings
//! (so two people face each other), the assignment fills a scene **atomically**
//! (skipping it entirely if a `required` slot can't be staffed), and multi-slot
//! scenes are weighted higher so the town fills with interactions first.
//!
//! NPCs are entities, so the whole pass is a no-op in offline/dry-run mode —
//! only a live `cargo run` actually spawns them.

use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::Editor;
use crate::generator::buildings_v2::Culture;
use crate::geometry::Point3D;
use crate::noise::RNG;

use super::npc::{spawn_villager_npc, DialogueVolume, Profession, VillagerBiome};

/// How much more a multi-person scene weighs than a solo one, per person. A
/// two-slot scene starts at `2 * BOOST`, so conversations and tables are picked
/// well ahead of lone standers in the town-wide draw.
const MULTI_SLOT_WEIGHT_BOOST: f32 = 3.0;

/// Factor a house's remaining anchor weights are multiplied by each time one of
/// its anchors is staffed — halving its pull on the next draw.
const HOUSE_WEIGHT_DECAY: f32 = 0.5;

/// Who may fill an anchor slot. Deserialized from furniture `anchors:` specs
/// (snake_case: `resident`, `worker`, `idle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRole {
    /// Any town resident from the roster.
    Resident,
    /// A worker bound to a workplace (profession matters). Used by industrial
    /// fixtures via [`AnchorScene::worker`], which forces the slot's profession.
    Worker,
    /// An idle bystander in a public space.
    Idle,
}

/// One person's spot within a scene: where they stand, which way they face, and
/// what kind of NPC belongs there.
#[derive(Clone, Debug)]
pub struct AnchorSlot {
    /// Build-area-local feet position — must be a walkable cell.
    pub pos: Point3D,
    /// Yaw in degrees the NPC faces (0 = south, like vanilla). For multi-person
    /// scenes this is baked toward the other slots at emit time.
    pub facing: f32,
    pub role: SlotRole,
    /// If true, the whole scene is skipped unless this slot can be filled.
    pub required: bool,
    /// Force a specific profession on the spawned NPC, overriding the roster's
    /// random one. `None` keeps the roster's profession. Used by workplace
    /// fixtures (the smithy worker is a smith regardless of who the roster
    /// hands us) — name and dialogue still come from the roster.
    pub profession: Option<Profession>,
    /// Context dialogue key (e.g. `tending_furnace`, `conversation`) indexing a
    /// pool in `NpcData::dialogue`. `None` falls back to generic small talk.
    /// Set from the furniture anchor that produced this slot.
    pub dialogue: Option<String>,
    /// How loudly this NPC's bubble reads. Interior/furniture anchors are
    /// `Normal`; plaza criers and stage performers are `Yelled` so their line
    /// carries across the square (see [`super::npc::DialogueVolume`]).
    pub volume: DialogueVolume,
    /// Fractional blocks to raise the spawned NPC's feet — e.g. `0.5` to stand on
    /// a slab top (a tower battlement). `0.0` for normal full-block ground.
    pub y_offset: f32,
}

impl AnchorSlot {
    /// A required slot with no forced profession or dialogue key — the caller
    /// sets `facing` itself, so each slot in a multi-person scene can look
    /// wherever it should (at a partner, a table, a focal point).
    pub fn new(pos: Point3D, facing: f32, role: SlotRole) -> Self {
        AnchorSlot {
            pos,
            facing,
            role,
            required: true,
            profession: None,
            dialogue: None,
            volume: DialogueVolume::Normal,
            y_offset: 0.0,
        }
    }
}

/// What a scene depicts. Deserialized from furniture `anchors:` specs
/// (snake_case: `solo`, `conversation`, `table`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind {
    Solo,
    Conversation,
    Haggle,
    Table,
    Mourning,
    /// A troupe on a plaza stage — a solo act, duet, or trio. The stage rolls the
    /// cast size and `staff_scene` draws a `performing` script with that many
    /// lines (see [`NpcData::exchange_of_len`]).
    Performance,
}

/// A group of slots filled as a unit.
#[derive(Clone, Debug)]
pub struct AnchorScene {
    pub kind: SceneKind,
    pub slots: Vec<AnchorSlot>,
}

impl AnchorScene {
    /// A one-person scene — the only kind v1 emits. The slot carries no forced
    /// profession, so it draws its trade from the roster.
    pub fn solo(pos: Point3D, facing: f32, role: SlotRole) -> Self {
        Self::solo_with(pos, facing, role, None, DialogueVolume::Normal)
    }

    /// A one-person scene with an explicit dialogue key and bubble volume. Used
    /// by plaza fixtures — a market vendor hawking wares (`Yelled`) or an idle
    /// onlooker in the crowd (`Normal`) — where the caller wants to pin the line
    /// pool and how loudly it reads rather than take the generic defaults.
    pub fn solo_with(
        pos: Point3D,
        facing: f32,
        role: SlotRole,
        dialogue: Option<String>,
        volume: DialogueVolume,
    ) -> Self {
        AnchorScene {
            kind: SceneKind::Solo,
            slots: vec![AnchorSlot {
                pos,
                facing,
                role,
                required: true,
                profession: None,
                dialogue,
                volume,
                y_offset: 0.0,
            }],
        }
    }

    /// A one-person workplace fixture: a single required [`SlotRole::Worker`]
    /// with a profession bound to the workplace (smith at a smithy, etc.). Name
    /// and dialogue still come from the roster; only the outfit is forced.
    pub fn worker(pos: Point3D, facing: f32, profession: Profession) -> Self {
        AnchorScene {
            kind: SceneKind::Solo,
            slots: vec![AnchorSlot {
                pos,
                facing,
                role: SlotRole::Worker,
                required: true,
                profession: Some(profession),
                dialogue: None,
                volume: DialogueVolume::Normal,
                y_offset: 0.0,
            }],
        }
    }

    /// A multi-person scene assembled from pre-built slots. Each slot already
    /// carries its own position, rotation, and role, so the caller decides where
    /// everyone looks (e.g. two conversers facing each other, a table facing
    /// inward). Slots default to `required`, so the scene is skipped unless all
    /// of them can be staffed.
    pub fn group(kind: SceneKind, slots: Vec<AnchorSlot>) -> Self {
        AnchorScene { kind, slots }
    }

    /// Base selection weight for the town-wide draw. Multi-person scenes are
    /// boosted so a town fills with interactions before lone standers. This is
    /// the *starting* weight; [`populate_town`] halves a house's live weights
    /// each time one of its anchors is staffed, so the crowd spreads off filled
    /// houses onto emptier ones.
    pub fn base_weight(&self) -> f32 {
        match self.slots.len() {
            0 | 1 => 1.0,
            n => n as f32 * MULTI_SLOT_WEIGHT_BOOST,
        }
    }

    /// Total person-slots in the scene (used to size the roster/budget so every
    /// slot — not just every scene — can be staffed).
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// How many slots must be staffed for the scene to be placed at all.
    fn required_count(&self) -> usize {
        self.slots.iter().filter(|s| s.required).count()
    }
}

/// A resolved NPC identity, ready to spawn. Dialogue isn't baked here — it's
/// chosen per placement from the slot's context key (see [`NpcData::line`]).
#[derive(Clone, Debug)]
pub struct Npc {
    pub name: String,
    pub biome: VillagerBiome,
    pub profession: Profession,
    /// A baby villager. Children keep a roster profession for naming/dialogue but
    /// spawn as a baby (see [`spawn_villager_npc`]).
    pub is_child: bool,
}

/// The villager skin variant that matches a town's culture.
fn villager_biome_for(culture: Culture) -> VillagerBiome {
    match culture {
        Culture::Desert => VillagerBiome::Desert,
        Culture::Japanese => VillagerBiome::Taiga,
        Culture::Medieval => VillagerBiome::Plains,
    }
}

/// Name + dialogue pools, loaded from `data/npcs.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcData {
    pub first_names: Vec<String>,
    pub epithets: Vec<String>,
    /// Generic fallback lines, used when a slot has no dialogue key (or the key
    /// isn't in `dialogue`).
    pub small_talk: Vec<String>,
    /// Context-keyed dialogue pools (e.g. `tending_furnace`, `conversation`,
    /// `crafting`). A key can be shared by several furniture items. Missing
    /// keys fall back to `small_talk`.
    #[serde(default)]
    pub dialogue: HashMap<String, Vec<String>>,
    /// Multi-person exchanges, keyed like `dialogue` but each entry is a whole
    /// back-and-forth: its lines are handed out positionally to a scene's slots
    /// (line 0 → first speaker, line 1 → their reply, …) so the pair reads as
    /// one conversation instead of two random lines. The first line of each
    /// exchange should stand alone, in case the scene is reduced to one person.
    #[serde(default)]
    pub exchanges: HashMap<String, Vec<Vec<String>>>,
}

impl NpcData {
    /// Load the pools from `data/npcs.yaml`.
    pub fn load() -> anyhow::Result<Self> {
        load_yaml("npcs.yaml")
    }

    /// Pick a dialogue line for a slot. Uses the context pool named by `key` if
    /// it exists and is non-empty; otherwise falls back to generic `small_talk`
    /// (and to "..." only if every pool is empty).
    pub fn line(&self, key: Option<&str>, rng: &mut RNG) -> String {
        let pool = key
            .and_then(|k| self.dialogue.get(k))
            .filter(|lines| !lines.is_empty())
            .unwrap_or(&self.small_talk);
        if pool.is_empty() {
            "...".to_string()
        } else {
            rng.choose(pool).clone()
        }
    }

    /// Pick one whole exchange (an ordered list of lines) for `key` whose cast
    /// size matches `n` — an entry with exactly `n` lines, one per speaker — or
    /// `None` if the pool has no `n`-line entry. The caller hands the lines out to
    /// a scene's slots in order so the group reads as one exchange. Selecting by
    /// length lets a single key (e.g. `performing`) hold solos, duets, and trios
    /// side by side, and a fixed-size scene draw the script that fits its cast.
    pub fn exchange_of_len(&self, key: &str, n: usize, rng: &mut RNG) -> Option<Vec<String>> {
        let pool = self.exchanges.get(key)?;
        let matching: Vec<&Vec<String>> = pool.iter().filter(|e| e.len() == n).collect();
        if matching.is_empty() {
            return None;
        }
        Some(rng.choose(&matching).to_vec())
    }
}

/// Professions a random resident may take (v1 — no workplace binding yet). These
/// stay in code rather than YAML since they're enum-bound; the duplicate
/// `Farmer` skews the roll toward common trades.
const PROFESSIONS: [Profession; 10] = [
    Profession::Farmer,
    Profession::Farmer,
    Profession::None,
    Profession::Librarian,
    Profession::Cleric,
    Profession::Fisherman,
    Profession::Shepherd,
    Profession::Mason,
    Profession::Butcher,
    Profession::Nitwit,
];

/// Generate a roster of `count` residents for a town of `culture`, drawing names
/// and dialogue from `data`.
pub fn build_roster(count: usize, culture: Culture, data: &NpcData, rng: &mut RNG) -> Vec<Npc> {
    let biome = villager_biome_for(culture);
    (0..count)
        .map(|_| {
            let mut name = rng.choose(&data.first_names).clone();
            if !data.epithets.is_empty() && rng.percent(35) {
                name.push(' ');
                name.push_str(rng.choose(&data.epithets).as_str());
            }
            let profession = *rng.choose(&PROFESSIONS);
            // Children aren't decided here yet — residents default to adults; the
            // caller flips `is_child` (and the spawner's `child`) when it wants one.
            Npc { name, biome, profession, is_child: false }
        })
        .collect()
}

/// One house's harvested anchor scenes plus the population budget its beds
/// imply. `population` is the count of anchors the town-wide draw aims to staff
/// for this house — derived by the caller from sleeping capacity (see
/// `POPULATION_PER_BED` in `settlement`), so a double bed counts for two.
pub struct HouseAnchors {
    pub scenes: Vec<AnchorScene>,
    pub population: usize,
}

/// One candidate scene in the town-wide draw, tagged with the house it belongs
/// to so a placement can damp the rest of that house's anchors.
struct Entry {
    house: usize,
    scene: AnchorScene,
    weight: f32,
    used: bool,
}

/// Spawn the NPCs for one scene from `pool`, returning how many were placed.
///
/// Atomic: if `pool` can't cover the scene's `required` slots, nothing is
/// spawned and `0` is returned. Multi-person scenes draw one shared exchange
/// (keyed by the first slot's dialogue key) and hand its lines out in slot
/// order, so a pair reads as one back-and-forth; solo scenes fall back to a
/// per-slot line. A slot may force a profession (workplace fixtures); otherwise
/// the NPC keeps the roster's random trade. `home`, when set, tags every NPC in
/// the scene with the house they belong to (see [`spawn_villager_npc`]).
async fn staff_scene(
    editor: &Editor,
    scene: &AnchorScene,
    pool: &mut Vec<Npc>,
    data: &NpcData,
    rng: &mut RNG,
    home: Option<usize>,
) -> anyhow::Result<usize> {
    let need = scene.required_count();
    if need == 0 || pool.len() < need {
        return Ok(0);
    }
    // Draw one script sized to this scene's cast: an `n`-line exchange for `n`
    // slots, so a duet gets two lines and a trio three (see `exchange_of_len`).
    // `need >= 1` here (the early return covers all-optional scenes), so slot 0
    // exists. Keys with no matching-length entry fall back to per-slot `line`s.
    let exchange = scene.slots[0]
        .dialogue
        .as_deref()
        .and_then(|k| data.exchange_of_len(k, scene.slots.len(), rng));
    let mut placed = 0usize;
    for (i, slot) in scene.slots.iter().enumerate() {
        if !slot.required && pool.is_empty() {
            continue; // optional slots fill only if roster remains
        }
        let Some(npc) = pool.pop() else { break };
        let profession = slot.profession.unwrap_or(npc.profession);
        let dialogue = exchange
            .as_ref()
            .and_then(|lines| lines.get(i).cloned())
            .unwrap_or_else(|| data.line(slot.dialogue.as_deref(), rng));
        spawn_villager_npc(
            editor, slot.pos, slot.facing, &npc.name, &dialogue, npc.biome, profession, slot.volume,
            home, npc.is_child, slot.y_offset,
        )
        .await?;
        placed += 1;
    }
    Ok(placed)
}

/// Pick the index (into `entries`) of one of `live` weighted by its current
/// `weight`, or `None` if `live` is empty. Tolerates an all-zero total (returns
/// the first live entry) so repeated halving can't trip an empty-weight panic.
fn weighted_pick(entries: &[Entry], live: &[usize], rng: &mut RNG) -> Option<usize> {
    if live.is_empty() {
        return None;
    }
    let total: f32 = live.iter().map(|&i| entries[i].weight).sum();
    if total <= 0.0 {
        return live.first().copied();
    }
    let mut r = rng.rand_i32(100_000) as f32 / 100_000.0 * total;
    for &i in live {
        if r < entries[i].weight {
            return Some(i);
        }
        r -= entries[i].weight;
    }
    live.last().copied()
}

/// Halve the live (unused) anchor weights of one house, so its pull on the next
/// town-wide draw drops after each resident placed there.
fn decay_house(entries: &mut [Entry], house: usize) {
    for e in entries.iter_mut() {
        if e.house == house && !e.used {
            e.weight *= HOUSE_WEIGHT_DECAY;
        }
    }
}

/// Populate a whole town from per-house anchors, sizing the crowd to beds.
///
/// The budget is `Σ max(1, beds)` over all houses. A seed pass staffs one anchor
/// in every house that has any (so each populated house gets at least one
/// resident), then a town-wide weighted draw fills the rest: anchors are picked
/// in proportion to their current weight (multi-person scenes weigh more), and
/// each placement halves the rest of that house's weights so the crowd flows off
/// filled houses onto emptier ones. Returns the number of NPCs spawned. No-op
/// (returns 0) in offline mode.
pub async fn populate_town(
    editor: &Editor,
    houses: Vec<HouseAnchors>,
    culture: Culture,
    data: &NpcData,
    rng: &mut RNG,
) -> anyhow::Result<usize> {
    if editor.is_offline() {
        return Ok(0);
    }

    let budget: usize = houses.iter().map(|h| h.population).sum();
    if budget == 0 {
        return Ok(0);
    }

    // Flatten houses into weighted entries, remembering each scene's house.
    let mut entries: Vec<Entry> = Vec::new();
    for (house, h) in houses.into_iter().enumerate() {
        for scene in h.scenes {
            let weight = scene.base_weight();
            entries.push(Entry { house, scene, weight, used: false });
        }
    }
    if entries.is_empty() {
        return Ok(0);
    }
    let num_houses = entries.iter().map(|e| e.house).max().map_or(0, |m| m + 1);

    // Roster: names/professions for up to two NPCs per staffed anchor (v1's max
    // slot count). Drained from the back as scenes are staffed.
    let mut pool = build_roster((budget * 2).max(1), culture, data, &mut rng.derive());

    let mut placed_anchors = 0usize;
    let mut placed_npcs = 0usize;

    // Seed pass: one anchor per house that has any, before the town-wide draw.
    for house in 0..num_houses {
        if pool.is_empty() || placed_anchors >= budget {
            break;
        }
        let live: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.house == house && !e.used)
            .map(|(i, _)| i)
            .collect();
        let Some(idx) = weighted_pick(&entries, &live, rng) else { continue };
        let n = staff_scene(editor, &entries[idx].scene, &mut pool, data, rng, Some(house)).await?;
        entries[idx].used = true;
        if n > 0 {
            placed_npcs += n;
            placed_anchors += 1;
            decay_house(&mut entries, house);
        }
    }

    // Town-wide draw for the remaining budget.
    while placed_anchors < budget && !pool.is_empty() {
        let live: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.used)
            .map(|(i, _)| i)
            .collect();
        let Some(idx) = weighted_pick(&entries, &live, rng) else { break };
        let house = entries[idx].house;
        let n = staff_scene(editor, &entries[idx].scene, &mut pool, data, rng, Some(house)).await?;
        entries[idx].used = true;
        if n > 0 {
            placed_npcs += n;
            placed_anchors += 1;
            decay_house(&mut entries, house);
        }
    }

    Ok(placed_npcs)
}

/// Distribute `roster` across a flat list of `scenes`, spawning up to `budget`
/// NPCs. Higher-weight (multi-person) scenes are tried first. Used for fixtures
/// like industrial workers, where every scene should be staffed. Returns the
/// number of NPCs spawned. No-op (returns 0) in offline mode.
pub async fn populate_npcs(
    editor: &Editor,
    mut scenes: Vec<AnchorScene>,
    mut roster: Vec<Npc>,
    budget: usize,
    data: &NpcData,
    rng: &mut RNG,
) -> anyhow::Result<usize> {
    if editor.is_offline() {
        return Ok(0);
    }

    // Shuffle for "somewhat random" placement, then prefer multi-person scenes
    // (stable sort keeps the shuffle as a tiebreak within equal weights).
    rng.shuffle(&mut scenes);
    rng.shuffle(&mut roster);
    scenes.sort_by(|a, b| {
        b.base_weight().partial_cmp(&a.base_weight()).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut pool = roster; // drained from the back as we place
    let mut placed = 0usize;
    for scene in &scenes {
        if placed >= budget || pool.is_empty() {
            break;
        }
        // Workplace/plaza fixtures aren't house residents, so they carry no home.
        placed += staff_scene(editor, scene, &mut pool, data, rng, None).await?;
    }
    Ok(placed)
}

/// Minecraft yaw (degrees) for an NPC at `from` looking toward `to`. Use this
/// when emitting anchors so the NPC faces something meaningful (a door, a
/// counter, the other half of a conversation). 0 = south, 90 = west.
pub fn yaw_toward(from: Point3D, to: Point3D) -> f32 {
    let dx = (to.x - from.x) as f32;
    let dz = (to.z - from.z) as f32;
    if dx == 0.0 && dz == 0.0 {
        return 0.0;
    }
    (-dx).atan2(dz).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Editor, World};
    use crate::geometry::{Point2D, Point3D};
    use crate::http_mod::GDMCHTTPProvider;
    use crate::noise::{Seed, RNG};
    use crate::util::init_logger;

    /// `npcs.yaml` parses and the context dialogue pools resolve. No server.
    #[test]
    fn npc_dialogue_pools_resolve() {
        let data = NpcData::load().expect("load npcs.yaml");
        let mut rng = RNG::new(Seed(1));
        // A known context key returns a line from its own pool, not small_talk.
        let cooking = &data.dialogue["cooking"];
        assert!(!cooking.is_empty(), "cooking pool should have lines");
        let line = data.line(Some("cooking"), &mut rng);
        assert!(cooking.contains(&line), "context line must come from the keyed pool");
        // An unknown key falls back to generic small talk.
        let fallback = data.line(Some("no_such_key"), &mut rng);
        assert!(data.small_talk.contains(&fallback), "missing key falls back to small_talk");
        // No key at all also falls back.
        let none = data.line(None, &mut rng);
        assert!(data.small_talk.contains(&none));
    }

    /// `exchange_of_len` returns only entries with the requested line count, so a
    /// fixed-size stage scene draws a script that fits its cast. No server.
    #[test]
    fn exchange_matches_cast_size() {
        let data = NpcData::load().expect("load npcs.yaml");
        let mut rng = RNG::new(Seed(7));
        // The `performing` pool has solos, duets, and trios; each draw has exactly
        // the requested number of lines.
        for n in 1..=3 {
            let script = data
                .exchange_of_len("performing", n, &mut rng)
                .unwrap_or_else(|| panic!("performing pool should have a {n}-line entry"));
            assert_eq!(script.len(), n, "drew a {}-line script for cast {n}", script.len());
        }
        // No 9-line performance exists, and an unknown key has no exchanges at all.
        assert!(data.exchange_of_len("performing", 9, &mut rng).is_none());
        assert!(data.exchange_of_len("no_such_key", 2, &mut rng).is_none());
    }

    /// Build a small roster and place it along a row of solo anchors across the
    /// middle of the build area, so the variety (names, professions, dialogue)
    /// can be eyeballed in-game. Needs a live server.
    /// Run with: `cargo test populate_demo -- --nocapture`.
    #[tokio::test]
    async fn populate_demo() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let world = World::new(&provider).await.expect("Failed to create world");
        let editor = Editor::new(build_area, world);
        let mut rng = RNG::new(Seed(42));

        let size = editor.world().world_rect_2d().size;
        let cz = size.y / 2;
        let start_x = size.x / 2 - 6;

        // A line of 6 solo anchors, each facing south (toward an approaching
        // player), spaced 3 apart.
        let count = 6;
        let scenes: Vec<AnchorScene> = (0..count)
            .map(|i| {
                let c = Point2D::new(start_x + i * 3, cz);
                let y = editor.world().get_ocean_floor_height_at(c);
                AnchorScene::solo(Point3D::new(c.x, y, c.y), 0.0, SlotRole::Resident)
            })
            .collect();

        let data = NpcData::load().expect("load npcs.yaml");
        let roster = build_roster(count as usize, Culture::Desert, &data, &mut rng);
        let placed = populate_npcs(&editor, scenes, roster, count as usize, &data, &mut rng)
            .await
            .expect("populate failed");

        println!("Placed {} NPCs in a demo row at z={}", placed, cz);
        assert!(placed > 0);
    }
}
