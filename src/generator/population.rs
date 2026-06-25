//! NPC population: turn a town into households of NPCs with kinship, then
//! distribute them across placement anchors emitted while the town was built.
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
//!
//! ## Household model
//!
//! Residents are generated as [`Household`]s (per house, sized to bed budget),
//! not as a flat roster. Each [`Npc`] carries: a town-wide unique [`NpcId`],
//! first name + surname (always stored), optional epithet, three-bucket
//! [`LifeStage`] (Child/Adult/Elder), and a `Vec<Relationship>` whose targets
//! reference any NPC in town by [`NpcId`] — relationships freely cross
//! household boundaries (in-laws, siblings who moved across town, an adult
//! child who inherited a neighbour's plot).
//!
//! Generation runs in four passes:
//!   1. [`build_households`] — open-shape households, sized to bed budget,
//!      with intra-household kin (parent↔child, spouse↔spouse, sibling↔sibling)
//!      wired reciprocally. Everyone starts a plain, unemployed villager.
//!   2. [`link_cross_household`] — in-laws, married-out siblings, adult
//!      children whose parents live elsewhere. All edges reciprocal.
//!   3. [`assign_employment`] — sets each adult's `look` (villager outfit) and
//!      `employment` (the job it implies); children stay plain villagers, elders
//!      mostly retire. v1 rolls random trades; future passes will consume a
//!      workplace jobs board.
//!   4. [`populate_town`] — the existing seeded + weighted anchor draw, but
//!      the per-house pool now reads from `population.households[h].members`.

use std::collections::HashMap;

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::Editor;
use crate::generator::buildings_v2::footprint::SizeClass;
use crate::generator::buildings_v2::Culture;
use crate::generator::nbts::{Structure, StructureType};
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::Color;
use crate::noise::RNG;

use super::npc::{
    spawn_mob_npc, spawn_villager_npc, DialogueVolume, NpcLook, Profession, Staffing, VillagerBiome,
};

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

/// Which age of NPC may fill a slot. Deserialized from furniture `anchors:`
/// specs (snake_case: `adult_only`, `any_age`, `child_only`). Defaults to
/// `AdultOnly`: most posts are adult work (anvils, stalls, guard posts), so a
/// slot opts *into* children. Mark domestic and social slots `any_age`, and
/// child-led scenes (playing in the yard) `child_only`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Occupant {
    AdultOnly,
    AnyAge,
    ChildOnly,
}

impl Default for Occupant {
    fn default() -> Self {
        Occupant::AdultOnly
    }
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
    /// Which age of NPC may stand here. Most slots are `AdultOnly`; domestic and
    /// social slots are marked `AnyAge` and play scenes `ChildOnly`, so a child
    /// only ever spawns where a furniture/scene author allowed one.
    pub occupant: Occupant,
    /// If true, the whole scene is skipped unless this slot can be filled.
    pub required: bool,
    /// Force how the spawned NPC looks — a villager with a specific profession,
    /// or a non-villager mob (e.g. a pillager guard) — overriding the default.
    /// `None` derives the look from the roster NPC (a villager wearing its own
    /// rolled profession). Either way, name and dialogue come from the roster.
    /// Used by workplace fixtures (the smithy worker is a smith regardless of who
    /// the roster hands us) and guard posts. See [`NpcLook`].
    pub look: Option<NpcLook>,
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
            occupant: Occupant::AdultOnly,
            required: true,
            look: None,
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
                occupant: Occupant::AdultOnly,
                required: true,
                look: None,
                dialogue,
                volume,
                y_offset: 0.0,
            }],
        }
    }

    /// A one-person workplace fixture: a single required [`SlotRole::Worker`]
    /// with a skin bound to the workplace (a smith look at a smithy, etc.). Name
    /// and dialogue still come from the roster; only the look is forced.
    pub fn worker(pos: Point3D, facing: f32, look: NpcLook) -> Self {
        AnchorScene {
            kind: SceneKind::Solo,
            slots: vec![AnchorSlot {
                pos,
                facing,
                role: SlotRole::Worker,
                occupant: Occupant::AdultOnly,
                required: true,
                look: Some(look),
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

// ============================================================================
// NPC data model
// ============================================================================

/// Town-wide unique identifier for an NPC. Relationships reference this so an
/// edge can freely cross household boundaries — a sibling who moved across
/// town, in-laws from another family, the smith's adult son living next door.
/// Allocated by [`IdAllocator`].
pub type NpcId = u32;

/// Hands out fresh [`NpcId`]s. One allocator threads through the whole town's
/// generation (residents, workplace fixtures, guards) so every NPC has a
/// unique id regardless of which subsystem spawned it.
#[derive(Debug)]
pub struct IdAllocator {
    next: u32,
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    pub fn new() -> Self {
        // Start at 1 so id 0 can stay a sentinel if anything needs one.
        IdAllocator { next: 1 }
    }

    pub fn next_id(&mut self) -> NpcId {
        let id = self.next;
        self.next += 1;
        id
    }
}

/// Three-bucket age model. Minecraft only renders adult-vs-baby visually, so
/// `Elder` is flavour (epithets, retired trade) but still spawns as an adult
/// villager — `Child` is the only stage that maps to the baby model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeStage {
    Child,
    Adult,
    Elder,
}

/// A household's wealth tier, derived from the building's [`SizeClass`] at
/// placement time. Drives downstream skew in employment (wealthy households
/// favour prestige trades; poor favour subsistence) and household shape
/// (wealthy households more often house servants/lodgers and multigen
/// elders; poor lean solo / sibling / lodger). Ordered Poor < … < Elite so
/// callers can compare with `>=`/`<` when expressing thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wealth {
    /// Cottage tier — rural / outskirts / subsistence.
    Poor,
    /// Standard town house — the majority of the population.
    Modest,
    /// Hall tier — a craftsman or notable family with means.
    Wealthy,
    /// Manor tier — the elite, capped at 1–2 per town.
    Elite,
}

impl Wealth {
    /// Map a [`SizeClass`] to the wealth tier of the household that lives
    /// there. The only wealth signal currently flowing into population
    /// generation, so the mapping is 1:1.
    pub fn from_size_class(sc: SizeClass) -> Self {
        match sc {
            SizeClass::Cottage => Wealth::Poor,
            SizeClass::House => Wealth::Modest,
            SizeClass::Hall => Wealth::Wealthy,
            SizeClass::Manor => Wealth::Elite,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Wealth::Poor => "Poor",
            Wealth::Modest => "Modest",
            Wealth::Wealthy => "Wealthy",
            Wealth::Elite => "Elite",
        }
    }
}

/// Kinship between two NPCs. Designed open: new variants (grandparent, in-law,
/// lodger) can land without reshaping the data model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipKind {
    Spouse,
    Parent,
    Child,
    Sibling,
}

/// One directed edge in the kinship graph. Both directions are stored on the
/// respective NPCs as a reciprocal pair (Parent on Alice, Child on Bob).
#[derive(Clone, Debug)]
pub struct Relationship {
    pub kind: RelationshipKind,
    /// Whom the edge points at — by town-wide id, so the target may live in any
    /// household (including none — fixtures aren't registered in
    /// [`Population::by_id`], so kin edges only target residents).
    pub to: NpcId,
}

/// A resolved NPC identity, ready to spawn. Dialogue isn't baked here — it's
/// chosen per placement from the slot's context key (see [`NpcData::line`]).
#[derive(Clone, Debug)]
pub struct Npc {
    pub id: NpcId,
    pub first_name: String,
    pub surname: String,
    /// Flavour add-on ("the Quiet", "Stonefoot"). When present, the displayed
    /// name tag uses `first epithet` instead of `first surname` — but the
    /// surname is *still stored* so kin/household lookups stay intact.
    /// Rolled heavily toward elders; rare on adults; almost never on children.
    pub epithet: Option<String>,
    pub life_stage: LifeStage,
    pub biome: VillagerBiome,
    /// How the NPC *looks* — a villager in some Minecraft profession outfit, or a
    /// mob (a witch, a pillager). This is the costume, not the job: "is it a
    /// villager, a witch, a weaponsmith." Residents default to a plain villager
    /// and are dressed by [`assign_employment`]; the mob half is allowed (those
    /// are just NPCs that look like a mob). Distinct from `employment`.
    pub look: NpcLook,
    /// What the NPC *does* — its literal town job ("guard", "woodcutter",
    /// "farmer", …), or `None` when unemployed (children, retired elders,
    /// nitwits, plain villagers). Residents get a trade label from
    /// [`assign_employment`]; fixtures get the label their data entry declares.
    /// Distinct from `look`: a weaponsmith-outfit villager can work as a guard.
    pub employment: Option<String>,
    /// Set once the NPC has been committed to a fixed spot in the world — today
    /// only by [`bind_workers`] when a resident is drafted to a workplace — so
    /// [`populate_town`] won't also seat them at home. Keeps the "each resident
    /// appears exactly once" invariant without disturbing [`Population::by_id`].
    pub placed: bool,
    pub relationships: Vec<Relationship>,
}

impl Npc {
    /// The name tag shown above the NPC in-game. Epithets replace the surname
    /// in the displayed form; the surname is still stored on the struct.
    pub fn display_name(&self) -> String {
        match &self.epithet {
            Some(e) => format!("{} {}", self.first_name, e),
            None => format!("{} {}", self.first_name, self.surname),
        }
    }

    pub fn is_child(&self) -> bool {
        matches!(self.life_stage, LifeStage::Child)
    }
}

/// One family unit living in a single house. Members usually share `surname`,
/// but a married-in spouse who kept their name or an unrelated lodger keeps
/// their own — `Npc.surname` is the source of truth per-member.
#[derive(Clone, Debug)]
pub struct Household {
    /// The household's primary surname (usually the head's). Members may carry
    /// a different one.
    pub surname: String,
    /// Which house this household lives in — the index used for the
    /// `home_<id>` entity tag on spawned residents.
    pub home: usize,
    /// Footprint centre (X/Z) of this household's house — copied from its
    /// [`HouseAnchors`] so proximity passes (work-binding, friendship) don't
    /// need to thread the house list around.
    pub pos: Point2D,
    /// Wealth tier derived from the building's size class at placement time.
    /// Biases [`assign_employment`] and [`pick_household_shape`].
    pub wealth: Wealth,
    pub members: Vec<Npc>,
}

/// The whole town's NPC population: households plus a flat id→location index
/// so cross-household passes (and the employment pass) can resolve an
/// [`NpcId`] in O(1).
#[derive(Debug, Default)]
pub struct Population {
    pub households: Vec<Household>,
    /// `id → (household_idx, member_idx)`. Only household members are
    /// registered here; anonymous fixtures (workplace workers, guards) get an
    /// id from the same allocator but aren't kin-resolvable.
    pub by_id: HashMap<NpcId, (usize, usize)>,
}

impl Population {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a member by id. `None` for fixture ids (not registered).
    pub fn get(&self, id: NpcId) -> Option<&Npc> {
        let &(h, m) = self.by_id.get(&id)?;
        Some(&self.households[h].members[m])
    }
}

/// The villager skin variant that matches a town's culture.
fn villager_biome_for(culture: Culture) -> VillagerBiome {
    match culture {
        Culture::Desert => VillagerBiome::Desert,
        Culture::Japanese => VillagerBiome::Taiga,
        Culture::Medieval => VillagerBiome::Plains,
    }
}

/// A staffed post defined in data: the job it fills and the skins it can wear.
/// `employment` is the job label baked onto the NPC (and tallied in the jobs
/// summary); `looks` is rolled per spawn, so a post can mix villager and mob
/// skins. Used for guard posts. Workplaces use [`Staffing`] (this plus a crew
/// size), declared on each building's structure JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct Fixture {
    pub employment: String,
    pub looks: Vec<NpcLook>,
}


/// Per-culture name pools (currently just first names). Loaded from the
/// `cultures` map in `data/npcs.yaml`, keyed by [`culture_key`].
#[derive(Debug, Clone, Deserialize)]
pub struct CultureNames {
    /// Localized first names, picked verbatim by [`roll_first_name`]. Mixes
    /// masculine and feminine names; one is assigned per NPC at random.
    #[serde(default)]
    pub first_names: Vec<String>,
}

/// The `data/npcs.yaml` key for a culture's name pools.
fn culture_key(culture: Culture) -> &'static str {
    match culture {
        Culture::Desert => "desert",
        Culture::Japanese => "japanese",
        Culture::Medieval => "medieval",
    }
}

/// Name + dialogue pools, loaded from `data/npcs.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcData {
    /// Per-culture name pools, keyed by [`culture_key`] (`medieval`, `japanese`,
    /// `desert`). A culture's `first_names` list, when non-empty, supersedes the
    /// legacy `first_name_prefixes`/`first_name_suffixes` composition.
    #[serde(default)]
    pub cultures: HashMap<String, CultureNames>,
    /// Consonant-ending stems that open a generated first name (Ael, Cor,
    /// Hild, Theod, …). Combined with [`Self::first_name_suffixes`] in
    /// [`roll_first_name`] to mint first names — Aelric, Coran, Hilda,
    /// Theodwyn. Keeping prefixes consonant-terminal avoids awkward double
    /// vowels when paired with a vowel-initial suffix.
    pub first_name_prefixes: Vec<String>,
    /// Endings appended to a [`Self::first_name_prefixes`] stem (-a, -ric,
    /// -wyn, …). See [`roll_first_name`].
    pub first_name_suffixes: Vec<String>,
    /// Capitalized noun-like roots that lead a generated surname (Ash, Oak,
    /// Stone, Hawk, Under, …). Combined with [`Self::surname_suffixes`] in
    /// [`roll_surname`] to mint family names — Ashwood, Stonebrook, Hawkridge.
    /// The two pools together give ~prefix*suffix possible surnames, so
    /// collisions across a town of ~100 residents stay rare.
    pub surname_prefixes: Vec<String>,
    /// Lowercase place-name endings that close a generated surname (-wood,
    /// -ford, -ridge, -ham, …). See [`Self::surname_prefixes`].
    pub surname_suffixes: Vec<String>,
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
    /// Gate and tower guard fixture: its `employment` job label and the `looks`
    /// pool rolled per guard (mix villager and mob skins; repeat to weight).
    /// `looks` must be non-empty (enforced by [`NpcData::validate`]).
    pub guards: Fixture,
    /// Market-stall vendor fixture — the skin pool and job label for the trader
    /// hawking wares behind a plaza stall. `looks` must be non-empty.
    pub vendors: Fixture,
    /// Stage performer fixture — the skin pool and job label for a troupe member
    /// up on a plaza stage. `looks` must be non-empty.
    pub performers: Fixture,
    /// Fallback worker [`Staffing`] for buildings whose structure JSON declares
    /// no `staffing` block of its own. Workplace staffing now lives on each
    /// building's structure sidecar; this only covers the unstated ones.
    pub default_staffing: Staffing,
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

    /// Like [`line`](Self::line), but for a child it first tries a `{key}_child`
    /// pool — so a kid at a `conversation` slot draws from `conversation_child`
    /// when that pool exists, and otherwise falls back to the normal line. Adults
    /// (and missing child pools) behave exactly like [`line`](Self::line).
    pub fn line_aged(&self, key: Option<&str>, is_child: bool, rng: &mut RNG) -> String {
        if is_child {
            if let Some(k) = key {
                let child_key = format!("{k}_child");
                if let Some(pool) = self
                    .dialogue
                    .get(&child_key)
                    .filter(|lines| !lines.is_empty())
                {
                    return rng.choose(pool).clone();
                }
            }
        }
        self.line(key, rng)
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

    /// The worker [`Staffing`] for a building of `kind`: its structure JSON's own
    /// `staffing` block when present, else the town-wide `default_staffing`. The
    /// caller rolls a skin per worker from `staffing.looks` so a multi-worker
    /// shop isn't uniform.
    pub fn staffing_for<'a>(
        &'a self,
        kind: &str,
        structures: &'a HashMap<StructureType, Structure>,
    ) -> &'a Staffing {
        structures
            .get(&StructureType(kind.to_string()))
            .and_then(|s| s.staffing.as_ref())
            .unwrap_or(&self.default_staffing)
    }

    /// Validate the NPC data against the loaded structures: the guard and default
    /// staffing pools must be non-empty, and every building that declares its own
    /// `staffing` block must have a non-empty `looks` pool and at least one
    /// worker (so a malformed sidecar fails at startup, not silently).
    pub fn validate(&self, structures: &HashMap<StructureType, Structure>) -> anyhow::Result<()> {
        if self.guards.looks.is_empty() {
            anyhow::bail!("npcs.yaml: `guards.looks` must list at least one entry");
        }
        if self.vendors.looks.is_empty() {
            anyhow::bail!("npcs.yaml: `vendors.looks` must list at least one entry");
        }
        if self.performers.looks.is_empty() {
            anyhow::bail!("npcs.yaml: `performers.looks` must list at least one entry");
        }
        if self.default_staffing.looks.is_empty() {
            anyhow::bail!("npcs.yaml: `default_staffing.looks` must list at least one entry");
        }
        if self.default_staffing.workers == 0 {
            anyhow::bail!("npcs.yaml: `default_staffing.workers` must be > 0");
        }
        for (ty, s) in structures {
            if let Some(staffing) = &s.staffing {
                if staffing.looks.is_empty() {
                    anyhow::bail!("structure {}: `staffing.looks` is empty", ty.0);
                }
                if staffing.workers == 0 {
                    anyhow::bail!("structure {}: `staffing.workers` is 0", ty.0);
                }
            }
        }
        Ok(())
    }
}

/// Professions an adult resident may take. Stays in code (not YAML) since it's
/// enum-bound. Used by [`build_roster`] for anonymous fixture NPCs (workplace
/// workers, plaza vendors) where wealth has no meaning; residents draw from
/// the wealth-tiered pools below instead. Duplicates skew toward common trades.
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

/// Wealth-tiered profession pools, used by [`assign_employment`] so each
/// household's adults read as plausible for their station. Same enum-bound
/// rationale as [`PROFESSIONS`]; duplicates skew the roll within each tier.
const PROFESSIONS_POOR: &[Profession] = &[
    Profession::Farmer,
    Profession::Farmer,
    Profession::Farmer,
    Profession::Shepherd,
    Profession::Shepherd,
    Profession::Fisherman,
    Profession::None,
    Profession::None,
    Profession::Nitwit,
    Profession::Nitwit,
];

const PROFESSIONS_MODEST: &[Profession] = &[
    Profession::Farmer,
    Profession::Farmer,
    Profession::Shepherd,
    Profession::Fisherman,
    Profession::Mason,
    Profession::Butcher,
    Profession::Fletcher,
    Profession::Leatherworker,
    Profession::None,
    Profession::Nitwit,
];

const PROFESSIONS_WEALTHY: &[Profession] = &[
    Profession::Mason,
    Profession::Mason,
    Profession::Butcher,
    Profession::Leatherworker,
    Profession::Toolsmith,
    Profession::Weaponsmith,
    Profession::Armorer,
    Profession::Cleric,
    Profession::Librarian,
    Profession::Cartographer,
];

const PROFESSIONS_ELITE: &[Profession] = &[
    Profession::Cleric,
    Profession::Cleric,
    Profession::Librarian,
    Profession::Librarian,
    Profession::Cartographer,
    Profession::Cartographer,
    Profession::Armorer,
    Profession::Toolsmith,
];

fn professions_for(wealth: Wealth) -> &'static [Profession] {
    match wealth {
        Wealth::Poor => PROFESSIONS_POOR,
        Wealth::Modest => PROFESSIONS_MODEST,
        Wealth::Wealthy => PROFESSIONS_WEALTHY,
        Wealth::Elite => PROFESSIONS_ELITE,
    }
}

// ============================================================================
// Name + epithet rolls
// ============================================================================

/// Pick a first name localized to `culture`. Prefers the culture's explicit
/// `first_names` list from `data/npcs.yaml` (Anglo-Saxon for medieval, Japanese,
/// Arabic for desert). Falls back to the legacy prefix×suffix composition —
/// "Cor" + "an" → "Coran" — when the culture lists no names, and to
/// `"Stranger"` only if every pool is empty (so a partial yaml doesn't panic).
fn roll_first_name(culture: Culture, data: &NpcData, rng: &mut RNG) -> String {
    if let Some(names) = data.cultures.get(culture_key(culture)) {
        if !names.first_names.is_empty() {
            return rng.choose(&names.first_names).clone();
        }
    }
    if data.first_name_prefixes.is_empty() || data.first_name_suffixes.is_empty() {
        return "Stranger".to_string();
    }
    let prefix = rng.choose(&data.first_name_prefixes);
    let suffix = rng.choose(&data.first_name_suffixes);
    format!("{}{}", prefix, suffix)
}

/// Mint a family name by combining a random prefix with a random suffix —
/// "Ash" + "wood" → "Ashwood", "Under" + "hill" → "Underhill". Falls back to
/// `"Townsfolk"` if either pool is empty (so a partial yaml doesn't panic).
fn roll_surname(data: &NpcData, rng: &mut RNG) -> String {
    if data.surname_prefixes.is_empty() || data.surname_suffixes.is_empty() {
        return "Townsfolk".to_string();
    }
    let prefix = rng.choose(&data.surname_prefixes);
    let suffix = rng.choose(&data.surname_suffixes);
    format!("{}{}", prefix, suffix)
}

/// Roll an epithet for a member of `stage`, weighted toward elders. Elders
/// almost always pick one up; adults rarely; children effectively never.
fn roll_epithet(stage: LifeStage, data: &NpcData, rng: &mut RNG) -> Option<String> {
    if data.epithets.is_empty() {
        return None;
    }
    let chance = match stage {
        LifeStage::Elder => 60,
        LifeStage::Adult => 8,
        LifeStage::Child => 0,
    };
    if chance > 0 && rng.percent(chance) {
        Some(rng.choose(&data.epithets).clone())
    } else {
        None
    }
}

// ============================================================================
// Household construction (intra-household kin)
// ============================================================================

/// Shapes a household may take, sized by bed budget. Picked once per house and
/// expanded into members + intra-household relationships by [`expand_shape`].
#[derive(Debug, Clone, Copy)]
enum HouseholdShape {
    /// Lone adult.
    SoloAdult,
    /// Lone elder ("the old hermit on the hill").
    SoloElder,
    /// Two adults, married.
    Couple,
    /// Single parent + N children.
    SingleParent(u8),
    /// Couple + N children.
    CoupleWithKids(u8),
    /// Couple + one elder (typically a parent of one spouse).
    CoupleWithElder,
    /// Couple + N children + one elder.
    CoupleWithKidsAndElder(u8),
    /// N adult siblings sharing a roof (no parents present).
    AdultSiblings(u8),
    /// N unrelated adults sharing a roof.
    Lodgers(u8),
    /// An elder living with an adult child.
    ElderWithAdultChild,
}

/// Roll a household shape sized to `budget`, biased by `wealth`:
/// * **Poor** — more solo elders, sibling clusters, and lodgers; fewer
///   intact couples (precarious life expectancy / migration).
/// * **Modest** — the baseline distribution (nuclear-heavy).
/// * **Wealthy** — couples stay intact; more multigen (elders cared for in
///   the household); more lodgers (semantically, live-in servants).
/// * **Elite** — heavy lodgers (household staff) on top of a strong couple +
///   multigen core.
fn pick_household_shape(budget: usize, wealth: Wealth, rng: &mut RNG) -> HouseholdShape {
    use HouseholdShape::*;
    match budget {
        0 | 1 => {
            // Solo. Poor see more lonely elders; wealthier are likelier to be
            // an adult living alone (a junior heir, an unmarried scholar).
            let elder_chance = match wealth {
                Wealth::Poor => 55,
                Wealth::Modest => 35,
                Wealth::Wealthy | Wealth::Elite => 25,
            };
            if rng.percent(elder_chance) { SoloElder } else { SoloAdult }
        }
        2 => {
            let r = rng.rand_i32(100);
            match wealth {
                Wealth::Poor => {
                    // Couples fragile; single-parent and sibling/lodger shares lift.
                    if r < 35 { Couple }
                    else if r < 60 { SingleParent(1) }
                    else if r < 75 { AdultSiblings(2) }
                    else if r < 92 { Lodgers(2) }
                    else { ElderWithAdultChild }
                }
                Wealth::Modest => {
                    if r < 55 { Couple }
                    else if r < 75 { SingleParent(1) }
                    else if r < 85 { AdultSiblings(2) }
                    else if r < 95 { Lodgers(2) }
                    else { ElderWithAdultChild }
                }
                Wealth::Wealthy => {
                    // Strong couples; some lodgers (a maid + cook).
                    if r < 60 { Couple }
                    else if r < 70 { SingleParent(1) }
                    else if r < 75 { AdultSiblings(2) }
                    else if r < 95 { Lodgers(2) }
                    else { ElderWithAdultChild }
                }
                Wealth::Elite => {
                    // Many lodgers (household staff) even at small head counts.
                    if r < 50 { Couple }
                    else if r < 55 { SingleParent(1) }
                    else if r < 58 { AdultSiblings(2) }
                    else if r < 95 { Lodgers(2) }
                    else { ElderWithAdultChild }
                }
            }
        }
        3 => {
            let r = rng.rand_i32(100);
            match wealth {
                Wealth::Poor => {
                    if r < 35 { CoupleWithKids(1) }
                    else if r < 60 { SingleParent(2) }
                    else if r < 70 { CoupleWithElder }
                    else if r < 85 { AdultSiblings(3) }
                    else { Lodgers(3) }
                }
                Wealth::Modest => {
                    if r < 50 { CoupleWithKids(1) }
                    else if r < 70 { SingleParent(2) }
                    else if r < 82 { CoupleWithElder }
                    else if r < 92 { AdultSiblings(3) }
                    else { Lodgers(3) }
                }
                Wealth::Wealthy => {
                    // More multigen (CoupleWithElder), some staff (Lodgers).
                    if r < 45 { CoupleWithKids(1) }
                    else if r < 55 { SingleParent(2) }
                    else if r < 80 { CoupleWithElder }
                    else if r < 85 { AdultSiblings(3) }
                    else { Lodgers(3) }
                }
                Wealth::Elite => {
                    if r < 35 { CoupleWithKids(1) }
                    else if r < 40 { SingleParent(2) }
                    else if r < 65 { CoupleWithElder }
                    else if r < 68 { AdultSiblings(3) }
                    else { Lodgers(3) } // ~32% staff
                }
            }
        }
        n => {
            // 4+ beds. Big houses are where wealth most differentiates:
            // wealthier households much more likely to host elders alongside kids.
            let kids = (n.saturating_sub(2)) as u8;
            let multigen_chance = match wealth {
                Wealth::Poor => 15,
                Wealth::Modest => 30,
                Wealth::Wealthy => 55,
                Wealth::Elite => 65,
            };
            if rng.percent(multigen_chance) && kids >= 1 {
                CoupleWithKidsAndElder(kids.saturating_sub(1).max(1))
            } else {
                CoupleWithKids(kids.max(1))
            }
        }
    }
}

/// Append a member to `members` and register it in `by_id`, returning its
/// in-household index. Id is freshly allocated; relationships start empty.
#[allow(clippy::too_many_arguments)]
fn push_member(
    h_idx: usize,
    members: &mut Vec<Npc>,
    by_id: &mut HashMap<NpcId, (usize, usize)>,
    alloc: &mut IdAllocator,
    first_name: String,
    surname: String,
    epithet: Option<String>,
    stage: LifeStage,
    biome: VillagerBiome,
) -> usize {
    let m_idx = members.len();
    let id = alloc.next_id();
    by_id.insert(id, (h_idx, m_idx));
    members.push(Npc {
        id,
        first_name,
        surname,
        epithet,
        life_stage: stage,
        biome,
        // Plain villager until `assign_employment` dresses and employs them.
        look: NpcLook::Villager(Profession::None),
        employment: None,
        placed: false,
        relationships: Vec::new(),
    });
    m_idx
}

/// Wire a reciprocal relationship pair: `a` gets a `forward` edge to `b`, and
/// `b` gets a `back` edge to `a`. (Spouse↔Spouse, Parent→Child + Child→Parent,
/// Sibling↔Sibling.) Both members must already be in `members`.
fn link_pair(
    members: &mut [Npc],
    a: usize,
    b: usize,
    forward: RelationshipKind,
    back: RelationshipKind,
) {
    let id_a = members[a].id;
    let id_b = members[b].id;
    members[a].relationships.push(Relationship { kind: forward, to: id_b });
    members[b].relationships.push(Relationship { kind: back, to: id_a });
}

/// Realize one shape into `members`, returning when done. Surname goes to the
/// "primary line"; lodgers and a couple's married-in spouse pick a different
/// one from the pool so the household isn't uniformly one name.
#[allow(clippy::too_many_arguments)]
fn expand_shape(
    shape: HouseholdShape,
    h_idx: usize,
    members: &mut Vec<Npc>,
    by_id: &mut HashMap<NpcId, (usize, usize)>,
    alloc: &mut IdAllocator,
    primary_surname: &str,
    data: &NpcData,
    culture: Culture,
    biome: VillagerBiome,
    rng: &mut RNG,
) {
    // Pick a surname distinct from `primary_surname` if the pool allows; falls
    // back to the primary if every roll matches (small surname pool).
    let alt_surname = |rng: &mut RNG| -> String {
        for _ in 0..6 {
            let s = roll_surname(data, rng);
            if s != primary_surname {
                return s;
            }
        }
        primary_surname.to_string()
    };

    match shape {
        HouseholdShape::SoloAdult => {
            let stage = LifeStage::Adult;
            push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(stage, data, rng), stage, biome);
        }
        HouseholdShape::SoloElder => {
            let stage = LifeStage::Elder;
            push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(stage, data, rng), stage, biome);
        }
        HouseholdShape::Couple => {
            // Spouse A keeps the household surname; spouse B has a chance to
            // have married in from another family (different surname).
            let married_in = rng.percent(40);
            let sn_b = if married_in { alt_surname(rng) } else { primary_surname.to_string() };
            let stage = LifeStage::Adult;
            let a = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(stage, data, rng), stage, biome);
            let b = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), sn_b,
                roll_epithet(stage, data, rng), stage, biome);
            link_pair(members, a, b, RelationshipKind::Spouse, RelationshipKind::Spouse);
        }
        HouseholdShape::SingleParent(n_kids) => {
            let parent = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            let mut kid_idxs = Vec::with_capacity(n_kids as usize);
            for _ in 0..n_kids {
                let k = push_member(h_idx, members, by_id, alloc,
                    roll_first_name(culture, data, rng), primary_surname.to_string(),
                    None, LifeStage::Child, biome);
                link_pair(members, parent, k, RelationshipKind::Child, RelationshipKind::Parent);
                kid_idxs.push(k);
            }
            link_siblings(members, &kid_idxs);
        }
        HouseholdShape::CoupleWithKids(n_kids) => {
            let married_in = rng.percent(40);
            let sn_b = if married_in { alt_surname(rng) } else { primary_surname.to_string() };
            let a = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            let b = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), sn_b,
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            link_pair(members, a, b, RelationshipKind::Spouse, RelationshipKind::Spouse);
            // Kids take the primary surname (the family name).
            let mut kid_idxs = Vec::with_capacity(n_kids as usize);
            for _ in 0..n_kids {
                let k = push_member(h_idx, members, by_id, alloc,
                    roll_first_name(culture, data, rng), primary_surname.to_string(),
                    None, LifeStage::Child, biome);
                link_pair(members, a, k, RelationshipKind::Child, RelationshipKind::Parent);
                link_pair(members, b, k, RelationshipKind::Child, RelationshipKind::Parent);
                kid_idxs.push(k);
            }
            link_siblings(members, &kid_idxs);
        }
        HouseholdShape::CoupleWithElder => {
            let married_in = rng.percent(40);
            let sn_b = if married_in { alt_surname(rng) } else { primary_surname.to_string() };
            let a = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            let b = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), sn_b,
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            link_pair(members, a, b, RelationshipKind::Spouse, RelationshipKind::Spouse);
            // Elder is a parent of whichever spouse shares the family name.
            let elder = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Elder, data, rng), LifeStage::Elder, biome);
            link_pair(members, elder, a, RelationshipKind::Child, RelationshipKind::Parent);
        }
        HouseholdShape::CoupleWithKidsAndElder(n_kids) => {
            let married_in = rng.percent(40);
            let sn_b = if married_in { alt_surname(rng) } else { primary_surname.to_string() };
            let a = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            let b = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), sn_b,
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            link_pair(members, a, b, RelationshipKind::Spouse, RelationshipKind::Spouse);
            let elder = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Elder, data, rng), LifeStage::Elder, biome);
            link_pair(members, elder, a, RelationshipKind::Child, RelationshipKind::Parent);
            let mut kid_idxs = Vec::with_capacity(n_kids as usize);
            for _ in 0..n_kids {
                let k = push_member(h_idx, members, by_id, alloc,
                    roll_first_name(culture, data, rng), primary_surname.to_string(),
                    None, LifeStage::Child, biome);
                link_pair(members, a, k, RelationshipKind::Child, RelationshipKind::Parent);
                link_pair(members, b, k, RelationshipKind::Child, RelationshipKind::Parent);
                // Elder is also a grandparent — not modeled with its own
                // RelationshipKind yet, but the Parent→Spouse→Child chain
                // already encodes it via traversal.
                kid_idxs.push(k);
            }
            link_siblings(members, &kid_idxs);
        }
        HouseholdShape::AdultSiblings(n) => {
            let mut idxs = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let i = push_member(h_idx, members, by_id, alloc,
                    roll_first_name(culture, data, rng), primary_surname.to_string(),
                    roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
                idxs.push(i);
            }
            link_siblings(members, &idxs);
        }
        HouseholdShape::Lodgers(n) => {
            // Unrelated adults sharing rent. First keeps primary surname; rest
            // get distinct ones so the household reads as "the Hollins house,
            // plus two boarders".
            for k in 0..n {
                let sn = if k == 0 {
                    primary_surname.to_string()
                } else {
                    alt_surname(rng)
                };
                push_member(h_idx, members, by_id, alloc,
                    roll_first_name(culture, data, rng), sn,
                    roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            }
        }
        HouseholdShape::ElderWithAdultChild => {
            let elder = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Elder, data, rng), LifeStage::Elder, biome);
            let child = push_member(h_idx, members, by_id, alloc,
                roll_first_name(culture, data, rng), primary_surname.to_string(),
                roll_epithet(LifeStage::Adult, data, rng), LifeStage::Adult, biome);
            link_pair(members, elder, child, RelationshipKind::Child, RelationshipKind::Parent);
        }
    }
}

/// Link every pair in `idxs` as siblings (reciprocal). Idempotent on an empty
/// or single-element slice.
fn link_siblings(members: &mut [Npc], idxs: &[usize]) {
    for i in 0..idxs.len() {
        for j in (i + 1)..idxs.len() {
            link_pair(members, idxs[i], idxs[j], RelationshipKind::Sibling, RelationshipKind::Sibling);
        }
    }
}

/// Build a [`Population`] for the whole town: one [`Household`] per house,
/// sized to that house's bed budget. Intra-household kin (parent/child, spouse,
/// sibling) are wired reciprocally; cross-household links come later in
/// [`link_cross_household`]. Professions are left `None` for the
/// [`assign_employment`] pass.
pub fn build_households(
    houses: &[HouseAnchors],
    culture: Culture,
    data: &NpcData,
    alloc: &mut IdAllocator,
    rng: &mut RNG,
) -> Population {
    let biome = villager_biome_for(culture);
    let mut pop = Population::new();
    // Every household's primary surname is unique within the town: each draw
    // re-rolls until it hits a combination not yet used. With ~prefixes *
    // suffixes ≈ 900 combos against ~50 households, this terminates in 1-2
    // tries per house on average; the cap below is defensive in case the
    // pool is ever shrunk below the household count.
    let mut used_surnames: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(houses.len());
    const SURNAME_MAX_TRIES: usize = 64;
    for (h_idx, house) in houses.iter().enumerate() {
        let budget = house.population.max(1);
        let wealth = house.wealth;
        // A manor usually takes its surname from its family colour (Black →
        // Blackwell), matching the colour it flies on its banners; 40% of the
        // time it falls back to a plain random surname. The colour choice is made
        // once per house so the uniqueness loop re-rolls a fresh colour *suffix*
        // (preserving the colour) rather than silently dropping it on collision.
        const COLOR_SURNAME_PCT: i32 = 40;
        let color_root = house.family_color.and_then(|c| c.surname_root());
        let use_color =
            color_root.is_some() && !data.surname_suffixes.is_empty() && rng.percent(COLOR_SURNAME_PCT);
        let roll = |rng: &mut RNG| -> String {
            match color_root {
                Some(root) if use_color => format!("{root}{}", rng.choose(&data.surname_suffixes)),
                _ => roll_surname(data, rng),
            }
        };
        let mut primary_surname = roll(rng);
        for _ in 0..SURNAME_MAX_TRIES {
            if !used_surnames.contains(&primary_surname) {
                break;
            }
            primary_surname = roll(rng);
        }
        used_surnames.insert(primary_surname.clone());
        let mut members: Vec<Npc> = Vec::new();
        let shape = pick_household_shape(budget, wealth, rng);
        expand_shape(
            shape, h_idx, &mut members, &mut pop.by_id, alloc,
            &primary_surname, data, culture, biome, rng,
        );
        pop.households.push(Household {
            surname: primary_surname,
            home: h_idx,
            pos: house.pos,
            wealth,
            members,
        });
    }
    pop
}

// ============================================================================
// Cross-household linking
// ============================================================================

/// Attach reciprocal kinship across household boundaries. Three opportunistic
/// passes — all probabilistic, all reciprocal:
///   * **in-laws**: for each married couple, ~30% one spouse gets a "birth
///     family" elsewhere (Parent edges from 1-2 adults of that house; Sibling
///     edges to any of *their* children);
///   * **adult siblings across town**: ~15% of adults with no sibling yet gain
///     one from another household;
///   * **adult-child elsewhere**: ~20% of adults with no Parent edge yet get
///     a parent (or two) in another household — the multigen pattern where
///     grown children live separately but kin is recorded.
pub fn link_cross_household(pop: &mut Population, rng: &mut RNG) {
    if pop.households.len() < 2 {
        return;
    }

    // ---- Pass A: in-laws ----
    let spouse_pairs: Vec<(NpcId, NpcId)> = collect_spouse_pairs(pop);
    for (a_id, b_id) in spouse_pairs {
        if !rng.percent(30) {
            continue;
        }
        let in_law = if rng.percent(50) { a_id } else { b_id };
        let in_law_house = pop.by_id.get(&in_law).map(|&(h, _)| h);
        let candidate_houses: Vec<usize> = (0..pop.households.len())
            .filter(|&i| Some(i) != in_law_house)
            .filter(|&i| {
                pop.households[i]
                    .members
                    .iter()
                    .any(|m| matches!(m.life_stage, LifeStage::Adult | LifeStage::Elder))
            })
            .collect();
        if candidate_houses.is_empty() {
            continue;
        }
        let other_h = *rng.choose(&candidate_houses);
        let other_adults: Vec<NpcId> = pop.households[other_h]
            .members
            .iter()
            .filter(|m| matches!(m.life_stage, LifeStage::Adult | LifeStage::Elder))
            .map(|m| m.id)
            .collect();
        let n_parents = if other_adults.len() >= 2 && rng.percent(70) { 2 } else { 1 };
        let parents: Vec<NpcId> = other_adults.iter().copied().take(n_parents).collect();
        for &pid in &parents {
            add_reciprocal(pop, pid, in_law, RelationshipKind::Child, RelationshipKind::Parent);
        }
        // Siblings: any other resident of `other_h` who already has a Parent
        // edge to one of these in-law parents reads as a sibling-in-law to the
        // newcomer.
        let siblings: Vec<NpcId> = pop.households[other_h]
            .members
            .iter()
            .filter(|m| m.id != in_law)
            .filter(|m| {
                m.relationships.iter().any(|r| {
                    r.kind == RelationshipKind::Parent && parents.contains(&r.to)
                })
            })
            .map(|m| m.id)
            .collect();
        for sib in siblings {
            add_reciprocal(pop, sib, in_law, RelationshipKind::Sibling, RelationshipKind::Sibling);
        }
    }

    // ---- Pass B: adult siblings across town ----
    let solo_adults: Vec<NpcId> = collect_adults_without(pop, RelationshipKind::Sibling);
    for a_id in solo_adults {
        if !rng.percent(15) {
            continue;
        }
        let cur_h = pop.by_id.get(&a_id).map(|&(h, _)| h);
        let mut candidates: Vec<NpcId> = Vec::new();
        for (i, h) in pop.households.iter().enumerate() {
            if Some(i) == cur_h {
                continue;
            }
            for m in &h.members {
                if matches!(m.life_stage, LifeStage::Adult)
                    && m.id != a_id
                    && !m.relationships.iter().any(|r| r.kind == RelationshipKind::Sibling)
                {
                    candidates.push(m.id);
                }
            }
        }
        if candidates.is_empty() {
            continue;
        }
        let b_id = *rng.choose(&candidates);
        add_reciprocal(pop, a_id, b_id, RelationshipKind::Sibling, RelationshipKind::Sibling);
    }

    // ---- Pass C: adult children whose parents live elsewhere ----
    let parentless_adults: Vec<NpcId> = collect_adults_without(pop, RelationshipKind::Parent);
    for a_id in parentless_adults {
        if !rng.percent(20) {
            continue;
        }
        let cur_h = pop.by_id.get(&a_id).map(|&(h, _)| h);
        let mut candidate_houses: Vec<usize> = Vec::new();
        for (i, h) in pop.households.iter().enumerate() {
            if Some(i) == cur_h {
                continue;
            }
            if h.members.iter().any(|m| {
                matches!(m.life_stage, LifeStage::Elder | LifeStage::Adult)
            }) {
                candidate_houses.push(i);
            }
        }
        if candidate_houses.is_empty() {
            continue;
        }
        let other_h = *rng.choose(&candidate_houses);
        let parent_pool: Vec<NpcId> = pop.households[other_h]
            .members
            .iter()
            .filter(|m| matches!(m.life_stage, LifeStage::Elder | LifeStage::Adult))
            .map(|m| m.id)
            .collect();
        let n_parents = if parent_pool.len() >= 2 && rng.percent(60) { 2 } else { 1 };
        for &pid in parent_pool.iter().take(n_parents) {
            add_reciprocal(pop, pid, a_id, RelationshipKind::Child, RelationshipKind::Parent);
        }
    }
}

fn collect_spouse_pairs(pop: &Population) -> Vec<(NpcId, NpcId)> {
    let mut out = Vec::new();
    for h in &pop.households {
        for m in &h.members {
            for r in &m.relationships {
                // Emit each pair once: lower id is the canonical "left".
                if r.kind == RelationshipKind::Spouse && m.id < r.to {
                    out.push((m.id, r.to));
                }
            }
        }
    }
    out
}

fn collect_adults_without(pop: &Population, kind: RelationshipKind) -> Vec<NpcId> {
    let mut out = Vec::new();
    for h in &pop.households {
        for m in &h.members {
            if matches!(m.life_stage, LifeStage::Adult)
                && !m.relationships.iter().any(|r| r.kind == kind)
            {
                out.push(m.id);
            }
        }
    }
    out
}

/// Add a reciprocal edge pair across the town. Either id may be a fixture
/// (not registered in `by_id`) — in that case the missing end is skipped, and
/// the relationship lands one-sided. Fixtures don't currently appear here, but
/// the guard keeps the function safe if they ever do.
fn add_reciprocal(
    pop: &mut Population,
    from: NpcId,
    to: NpcId,
    forward: RelationshipKind,
    back: RelationshipKind,
) {
    if let Some(&(h, m)) = pop.by_id.get(&from) {
        pop.households[h].members[m]
            .relationships
            .push(Relationship { kind: forward, to });
    }
    if let Some(&(h, m)) = pop.by_id.get(&to) {
        pop.households[h].members[m]
            .relationships
            .push(Relationship { kind: back, to: from });
    }
}

// ============================================================================
// Employment
// ============================================================================

/// Dress and employ every resident. Rolls a villager look-profession per member
/// (children stay plain villagers; most elders retire to a plain robe, 20%
/// keeping their old trade outfit for flavour; adults draw from their wealth
/// tier), then sets `look` to that villager outfit and `employment` to the job
/// it implies (see [`Profession::employment`] — the jobless `None`/`Nitwit`
/// looks carry no employment). A later patch can swap the roll for a workplace
/// jobs board without changing the surrounding pipeline.
pub fn assign_employment(pop: &mut Population, rng: &mut RNG) {
    for h in pop.households.iter_mut() {
        let pool = professions_for(h.wealth);
        for m in h.members.iter_mut() {
            let outfit = match m.life_stage {
                LifeStage::Child => Profession::None,
                // Most elders retire to a plain robe; 20% retain a trade — drawn
                // from the household's tier so a wealthy retiree keeps a prestige
                // outfit (the old librarian still wears the robe).
                LifeStage::Elder if !rng.percent(20) => Profession::None,
                LifeStage::Elder | LifeStage::Adult => *rng.choose(pool),
            };
            m.look = NpcLook::Villager(outfit);
            m.employment = outfit.employment().map(String::from);
        }
    }
}

// ============================================================================
// Diagnostics
// ============================================================================

/// Print a town-wide population breakdown: household counts, life-stage mix,
/// kin-edge counts (with cross-household share), surname diversity, and
/// employment distribution. Emits via `println!` so it lands in the same
/// console stream as the rest of the pipeline summaries. Cheap — a few passes
/// over members.
pub fn log_population_stats(pop: &Population) {
    if pop.households.is_empty() {
        println!("=== POPULATION STATS: empty (no households) ===");
        return;
    }

    let n_households = pop.households.len();
    let total_residents: usize = pop.households.iter().map(|h| h.members.len()).sum();
    let pct = |n: usize| -> f32 {
        if total_residents == 0 {
            0.0
        } else {
            100.0 * n as f32 / total_residents as f32
        }
    };

    // Life-stage breakdown.
    let mut children = 0usize;
    let mut adults = 0usize;
    let mut elders = 0usize;
    for h in &pop.households {
        for m in &h.members {
            match m.life_stage {
                LifeStage::Child => children += 1,
                LifeStage::Adult => adults += 1,
                LifeStage::Elder => elders += 1,
            }
        }
    }

    println!("=== POPULATION STATS ===");
    println!(
        "Households: {} | Total residents: {} | Avg household size: {:.2}",
        n_households,
        total_residents,
        total_residents as f32 / n_households as f32,
    );
    println!("Life stages:");
    println!("  Children: {:>4} ({:5.1}%)", children, pct(children));
    println!("  Adults:   {:>4} ({:5.1}%)", adults, pct(adults));
    println!("  Elders:   {:>4} ({:5.1}%)", elders, pct(elders));

    // Wealth tier distribution across households.
    let mut wealth_counts = [0usize; 4]; // Poor, Modest, Wealthy, Elite
    for h in &pop.households {
        let idx = match h.wealth {
            Wealth::Poor => 0,
            Wealth::Modest => 1,
            Wealth::Wealthy => 2,
            Wealth::Elite => 3,
        };
        wealth_counts[idx] += 1;
    }
    println!("Wealth tiers (households):");
    for (label, n) in ["Poor", "Modest", "Wealthy", "Elite"].iter().zip(wealth_counts.iter()) {
        let pct_h = 100.0 * *n as f32 / n_households as f32;
        println!("  {:<8} {:>3} ({:5.1}%)", label, n, pct_h);
    }

    // Household size distribution.
    let mut size_dist: HashMap<usize, usize> = HashMap::new();
    for h in &pop.households {
        *size_dist.entry(h.members.len()).or_insert(0) += 1;
    }
    let mut sizes: Vec<usize> = size_dist.keys().copied().collect();
    sizes.sort_unstable();
    println!("Household size distribution:");
    for s in sizes {
        let n = size_dist[&s];
        let pct_h = 100.0 * n as f32 / n_households as f32;
        println!("  {} members: {:>3} houses ({:5.1}%)", s, n, pct_h);
    }

    // Kin edges (each undirected edge counted once: Spouse/Sibling use id<to,
    // Parent edges are unique by direction since Child is the back-edge).
    let mut spouse_edges = 0usize;
    let mut sibling_edges = 0usize;
    let mut parent_edges = 0usize;
    let mut cross_spouse = 0usize;
    let mut cross_sibling = 0usize;
    let mut cross_parent = 0usize;
    let house_of = |id: NpcId| -> Option<usize> { pop.by_id.get(&id).map(|&(h, _)| h) };
    for h in &pop.households {
        for m in &h.members {
            let src_h = house_of(m.id);
            for r in &m.relationships {
                let dst_h = house_of(r.to);
                let cross = src_h.is_some() && dst_h.is_some() && src_h != dst_h;
                match r.kind {
                    RelationshipKind::Spouse if m.id < r.to => {
                        spouse_edges += 1;
                        if cross {
                            cross_spouse += 1;
                        }
                    }
                    RelationshipKind::Sibling if m.id < r.to => {
                        sibling_edges += 1;
                        if cross {
                            cross_sibling += 1;
                        }
                    }
                    RelationshipKind::Parent => {
                        parent_edges += 1;
                        if cross {
                            cross_parent += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let total_edges = spouse_edges + sibling_edges + parent_edges;
    let cross_edges = cross_spouse + cross_sibling + cross_parent;
    println!("Kin edges (undirected): {}", total_edges);
    println!("  Spouse:  {:>4} (cross-household: {})", spouse_edges, cross_spouse);
    println!("  Parent:  {:>4} (cross-household: {})", parent_edges, cross_parent);
    println!("  Sibling: {:>4} (cross-household: {})", sibling_edges, cross_sibling);
    if total_edges > 0 {
        println!(
            "  Cross-household share: {}/{} ({:.1}%)",
            cross_edges, total_edges,
            100.0 * cross_edges as f32 / total_edges as f32,
        );
    }
    if total_residents > 0 {
        println!(
            "  Avg edges/person: {:.2}",
            (total_edges * 2) as f32 / total_residents as f32,
        );
    }

    // Surname diversity (top 5 by count).
    let mut surname_counts: HashMap<String, usize> = HashMap::new();
    for h in &pop.households {
        for m in &h.members {
            *surname_counts.entry(m.surname.clone()).or_insert(0) += 1;
        }
    }
    let mut sn: Vec<(String, usize)> = surname_counts.into_iter().collect();
    sn.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("Surnames: {} distinct across {} residents", sn.len(), total_residents);
    let top: Vec<String> = sn.iter().take(5).map(|(s, c)| format!("{} ({})", s, c)).collect();
    if !top.is_empty() {
        println!("  Most common: {}", top.join(", "));
    }

    // Employment breakdown (full sorted by count).
    let mut emp_counts: HashMap<String, usize> = HashMap::new();
    let mut unemployed = 0usize;
    for h in &pop.households {
        for m in &h.members {
            match &m.employment {
                None => unemployed += 1,
                Some(e) => *emp_counts.entry(e.clone()).or_insert(0) += 1,
            }
        }
    }
    let mut emp: Vec<(String, usize)> = emp_counts.into_iter().collect();
    emp.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("Employment:");
    println!("  {:<14} {:>4} ({:5.1}%)", "Unemployed", unemployed, pct(unemployed));
    for (k, c) in &emp {
        println!("  {:<14} {:>4} ({:5.1}%)", k, c, pct(*c));
    }
    let unemp_children_share = if children > 0 {
        100.0 * children.min(unemployed) as f32 / unemployed as f32
    } else {
        0.0
    };
    println!(
        "  (Children account for {:.0}% of unemployed; rest is elders + adult nitwits/None)",
        unemp_children_share,
    );
}

/// Print a handful of sample households in detail (members + their kin links
/// by name). Picks `n` households spaced through the list so the sample
/// spans small, medium, and large families rather than the first `n`.
pub fn log_sample_households(pop: &Population, n: usize) {
    if pop.households.is_empty() || n == 0 {
        return;
    }
    println!("=== SAMPLE HOUSEHOLDS ({} of {}) ===", n.min(pop.households.len()), pop.households.len());
    let n = n.min(pop.households.len());
    let step = (pop.households.len() / n).max(1);
    let look_name = |id: NpcId| -> String {
        pop.get(id).map(|m| m.display_name()).unwrap_or_else(|| format!("npc#{}", id))
    };
    for h_idx in (0..pop.households.len()).step_by(step).take(n) {
        let h = &pop.households[h_idx];
        println!(
            "House #{:<3} {:<14} [{:<7}] ({} members):",
            h_idx, format!("\"{}\"", h.surname), h.wealth.label(), h.members.len(),
        );
        for m in &h.members {
            let job = m.employment.clone().unwrap_or_else(|| "—".to_string());
            let kin: Vec<String> = m
                .relationships
                .iter()
                .map(|r| {
                    let kind = format!("{:?}", r.kind).to_lowercase();
                    // Mark cross-household kin so the sample shows them.
                    let dst_h = pop.by_id.get(&r.to).map(|&(h, _)| h);
                    let mark = if dst_h.is_some() && dst_h != Some(h_idx) {
                        " (cross)"
                    } else {
                        ""
                    };
                    format!("{}: {}{}", kind, look_name(r.to), mark)
                })
                .collect();
            let kin_str = if kin.is_empty() {
                "—".to_string()
            } else {
                kin.join(", ")
            };
            println!(
                "  {:<24} {:<6} {:<14} | {}",
                m.display_name(),
                format!("{:?}", m.life_stage),
                job,
                kin_str,
            );
        }
    }
}

// ============================================================================
// Flat fixture roster (workplaces, plaza)
// ============================================================================

/// Generate a flat roster of `count` NPCs for fixture placement (plaza vendors
/// and onlookers, workplace workers, guards), `children` of them kids and the
/// rest working adults. Children are plain villagers with no trade — they exist
/// to fill `ChildOnly` slots (e.g. kids in a market crowd); pass `0` for
/// adult-only fixtures. Unlike household members, these are not registered in
/// any [`Population`] — they have unique ids but no kin edges, no home, and a
/// look/employment rolled up front (since fixtures usually override the look via
/// the scene's slot anyway). Callers shuffle before placing, so the kids-first
/// ordering here doesn't cluster them.
pub fn build_roster(
    count: usize,
    children: usize,
    culture: Culture,
    data: &NpcData,
    alloc: &mut IdAllocator,
    rng: &mut RNG,
) -> Vec<Npc> {
    let biome = villager_biome_for(culture);
    let children = children.min(count);
    (0..count)
        .map(|i| {
            let (stage, look, employment) = if i < children {
                (LifeStage::Child, NpcLook::Villager(Profession::None), None)
            } else {
                let outfit = *rng.choose(&PROFESSIONS);
                (
                    LifeStage::Adult,
                    NpcLook::Villager(outfit),
                    outfit.employment().map(String::from),
                )
            };
            Npc {
                id: alloc.next_id(),
                first_name: roll_first_name(culture, data, rng),
                surname: roll_surname(data, rng),
                epithet: roll_epithet(stage, data, rng),
                life_stage: stage,
                biome,
                look,
                employment,
                placed: false,
                relationships: Vec::new(),
            }
        })
        .collect()
}

// ============================================================================
// Placement
// ============================================================================

/// One house's harvested anchor scenes plus the population budget its beds
/// imply. `population` is the count of anchors the town-wide draw aims to staff
/// for this house — derived by the caller from sleeping capacity (see
/// `POPULATION_PER_BED` in `settlement`), so a double bed counts for two.
/// `wealth` is the building's SizeClass mapped through
/// [`Wealth::from_size_class`] and drives household-shape and employment skew.
pub struct HouseAnchors {
    pub scenes: Vec<AnchorScene>,
    pub population: usize,
    pub wealth: Wealth,
    /// Footprint centre (X/Z) of this house, used by the household it seeds for
    /// cheap proximity queries (work-binding to nearby workplaces, friendship
    /// drafting between neighbours).
    pub pos: Point2D,
    /// A manor's unique family colour, if this house is a manor. The household it
    /// seeds takes its surname from this colour most of the time (Black →
    /// Blackwell), matching the colour it flies on its exterior banners. `None`
    /// for ordinary houses, which keep a plain random surname.
    pub family_color: Option<Color>,
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
/// the NPC's own profession is used (defaulting to [`Profession::None`] for
/// children and unemployed residents). `home`, when set, tags every NPC in the
/// scene with the house they belong to (see [`spawn_villager_npc`]).
async fn staff_scene(
    editor: &Editor,
    scene: &AnchorScene,
    pool: &mut Vec<Npc>,
    data: &NpcData,
    rng: &mut RNG,
    home: Option<usize>,
) -> anyhow::Result<usize> {
    let need = scene.required_count();
    if need == 0 {
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

    // Select an NPC per slot *before* spawning, so the scene stays atomic: a
    // required slot that can't be matched by age drops the whole scene with
    // nothing placed. Match age-restricted slots first so an `AnyAge` slot can't
    // grab the only adult a later `AdultOnly` slot needs (or the only child a
    // `ChildOnly` slot needs).
    let mut assigned: Vec<Option<Npc>> = (0..scene.slots.len()).map(|_| None).collect();
    let mut order: Vec<usize> = (0..scene.slots.len()).collect();
    order.sort_by_key(|&i| {
        let s = &scene.slots[i];
        (matches!(s.occupant, Occupant::AnyAge) as u8, !s.required)
    });
    for &i in &order {
        let slot = &scene.slots[i];
        match take_for(pool, slot.occupant) {
            Some(npc) => assigned[i] = Some(npc),
            None if slot.required => {
                // Infeasible — restore what we took and drop the scene.
                for npc in assigned.into_iter().flatten() {
                    pool.push(npc);
                }
                return Ok(0);
            }
            None => {} // optional slot with no acceptable NPC left → skip it
        }
    }

    let mut placed = 0usize;
    for (i, slot) in scene.slots.iter().enumerate() {
        let Some(npc) = assigned[i].take() else { continue };
        // The slot may force a look (workplace trade, guard mob); otherwise the
        // NPC keeps its own rolled look. Name and dialogue always come from the
        // roster; a child draws a `{key}_child` line when the pool defines one.
        let look = slot.look.unwrap_or(npc.look);
        let dialogue = exchange
            .as_ref()
            .and_then(|lines| lines.get(i).cloned())
            .unwrap_or_else(|| data.line_aged(slot.dialogue.as_deref(), npc.is_child(), rng));
        let display_name = npc.display_name();
        match look {
            NpcLook::Villager(profession) => {
                spawn_villager_npc(
                    editor, slot.pos, slot.facing, &display_name, &dialogue, npc.biome,
                    profession, slot.volume, home, npc.is_child(), slot.y_offset,
                )
                .await?;
            }
            // The mob path is villager-free: biome/profession/home/child don't
            // apply, but the roster still supplies the name and dialogue.
            NpcLook::Mob(mob) => {
                spawn_mob_npc(
                    editor, slot.pos, slot.facing, &display_name, &dialogue, mob,
                    slot.volume, slot.y_offset,
                )
                .await?;
            }
        }
        placed += 1;
    }
    Ok(placed)
}

/// Remove and return the last pool NPC whose age the slot's [`Occupant`] rule
/// accepts, preferring an adult for `AnyAge` so children are left for any
/// `ChildOnly` slots in the same scene. `None` if the pool has no acceptable
/// NPC. Takes from the back to mirror the old `pool.pop()` draw order.
fn take_for(pool: &mut Vec<Npc>, occupant: Occupant) -> Option<Npc> {
    let pos = match occupant {
        Occupant::AdultOnly => pool.iter().rposition(|n| !n.is_child()),
        Occupant::ChildOnly => pool.iter().rposition(|n| n.is_child()),
        Occupant::AnyAge => pool
            .iter()
            .rposition(|n| !n.is_child())
            .or_else(|| pool.iter().rposition(|n| n.is_child())),
    }?;
    Some(pool.remove(pos))
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

/// Populate a whole town: per-house anchors paired with the pre-built
/// [`Population`].
///
/// `houses` and `population.households` must be parallel (same length, same
/// ordering by house index). For each house, the candidate NPC pool is its
/// household's members. A seed pass staffs one anchor in every house that has
/// any (so each populated house gets at least one resident), then a town-wide
/// weighted draw fills the rest: anchors are picked in proportion to their
/// current weight (multi-person scenes weigh more), and each placement halves
/// the rest of that house's weights so the crowd flows off filled houses onto
/// emptier ones. Returns the number of NPCs spawned. No-op (returns 0) in
/// offline mode.
pub async fn populate_town(
    editor: &Editor,
    houses: Vec<HouseAnchors>,
    population: Population,
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

    // Per-house NPC pools, taken from the population. A house with no household
    // entry (shouldn't happen if parallel) ends up with an empty pool — its
    // anchors will simply skip.
    let mut households = population.households;
    let mut pools: Vec<Vec<Npc>> = (0..num_houses)
        .map(|i| {
            if i < households.len() {
                // Residents already committed elsewhere (bound to a workplace by
                // `bind_workers`) are filtered out so they aren't seated twice.
                std::mem::take(&mut households[i].members)
                    .into_iter()
                    .filter(|m| !m.placed)
                    .collect()
            } else {
                Vec::new()
            }
        })
        .collect();

    let mut placed_anchors = 0usize;
    let mut placed_npcs = 0usize;

    // Seed pass: one anchor per house that has any, before the town-wide draw.
    for house in 0..num_houses {
        if pools[house].is_empty() || placed_anchors >= budget {
            continue;
        }
        let live: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.house == house && !e.used)
            .map(|(i, _)| i)
            .collect();
        let Some(idx) = weighted_pick(&entries, &live, rng) else {
            continue;
        };
        let n = staff_scene(editor, &entries[idx].scene, &mut pools[house], data, rng, Some(house))
            .await?;
        entries[idx].used = true;
        if n > 0 {
            placed_npcs += n;
            placed_anchors += 1;
            decay_house(&mut entries, house);
        }
    }

    // Town-wide draw for the remaining budget.
    while placed_anchors < budget && pools.iter().any(|p| !p.is_empty()) {
        let live: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.used && !pools[e.house].is_empty())
            .map(|(i, _)| i)
            .collect();
        let Some(idx) = weighted_pick(&entries, &live, rng) else {
            break;
        };
        let house = entries[idx].house;
        let n = staff_scene(editor, &entries[idx].scene, &mut pools[house], data, rng, Some(house))
            .await?;
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

// ============================================================================
// Employment binding — workplaces draft nearby residents
// ============================================================================

/// One worker post at a placed workplace: where the worker stands and which way
/// they face, the building centre used for proximity scoring, the skin pool the
/// post dresses its worker from, and the job label baked onto whoever fills it.
/// The settlement layer's claim scan produces these (see `discover_worker_slots`)
/// and [`bind_workers`] consumes them; posts left unfilled are handed back so the
/// caller can backfill them with anonymous fixtures.
pub struct WorkerSlot {
    pub stand: Point3D,
    pub facing: f32,
    pub workplace: Point2D,
    pub looks: Vec<NpcLook>,
    pub employment: String,
}

/// Distance falloff for the work draft. A resident next door to a workplace is
/// far likelier to staff it than one across town; exponential decay over this
/// many blocks keeps the pull strong but never absolute.
const WORK_PROXIMITY_SCALE: f32 = 48.0;
/// Weight multiplier for a resident whose flavour outfit already matches the
/// post's trade (a weaponsmith villager staffing the smithy). Mild on purpose,
/// so proximity stays the lead term and a nearby non-specialist can still be
/// hired — and re-dressed in the trade.
const WORK_AFFINITY_MATCH: f32 = 3.0;

/// Per-NPC base employability, independent of any particular workplace. Today
/// every working-age adult scores the same; this is the seam where skill,
/// trait, or age terms will multiply in later. Proximity and trade affinity are
/// applied per-workplace at draft time (see [`draft_worker`]), not here.
fn base_qualification(_npc: &Npc) -> f32 {
    1.0
}

/// Proximity weight between a household's house and a workplace.
fn work_proximity(home: Point2D, workplace: Point2D) -> f32 {
    let dx = (home.x - workplace.x) as f32;
    let dz = (home.y - workplace.y) as f32;
    let dist = (dx * dx + dz * dz).sqrt();
    (-dist / WORK_PROXIMITY_SCALE).exp()
}

/// Trade-affinity weight: a bonus when the resident's current look is one the
/// post would hire for, else neutral.
fn work_affinity(npc: &Npc, looks: &[NpcLook]) -> f32 {
    if looks.iter().any(|l| *l == npc.look) {
        WORK_AFFINITY_MATCH
    } else {
        1.0
    }
}

/// Weighted pick over `(household_idx, member_idx, weight)` candidates. Mirrors
/// [`weighted_pick`]'s tolerance of an all-zero total (returns the first).
fn weighted_choice(cands: &[(usize, usize, f32)], rng: &mut RNG) -> Option<(usize, usize)> {
    if cands.is_empty() {
        return None;
    }
    let total: f32 = cands.iter().map(|c| c.2).sum();
    if total <= 0.0 {
        return cands.first().map(|c| (c.0, c.1));
    }
    let mut r = rng.rand_i32(100_000) as f32 / 100_000.0 * total;
    for c in cands {
        if r < c.2 {
            return Some((c.0, c.1));
        }
        r -= c.2;
    }
    cands.last().map(|c| (c.0, c.1))
}

/// The pure core of the work draft: choose which unplaced working-age adult
/// staffs a post at `workplace`, weighting each candidate by
/// `base_qualification * proximity * affinity`. Returns the chosen member's
/// `(household_idx, member_idx)`, or `None` if no eligible adult remains. Pure
/// (no I/O), so the qualification model is unit-testable without a server.
fn draft_worker(
    households: &[Household],
    workplace: Point2D,
    looks: &[NpcLook],
    rng: &mut RNG,
) -> Option<(usize, usize)> {
    let mut cands: Vec<(usize, usize, f32)> = Vec::new();
    for (hi, h) in households.iter().enumerate() {
        for (mi, m) in h.members.iter().enumerate() {
            if m.placed || m.life_stage != LifeStage::Adult {
                continue;
            }
            let w = base_qualification(m)
                * work_proximity(h.pos, workplace)
                * work_affinity(m, looks);
            if w > 0.0 {
                cands.push((hi, mi, w));
            }
        }
    }
    weighted_choice(&cands, rng)
}

/// Staff each workplace from the resident population *before* residential
/// placement, so a resident drafted into a shop isn't also seated at home. For
/// every [`WorkerSlot`], [`draft_worker`] picks an unplaced working-age adult
/// (proximity-led qualification); the chosen resident is dressed in the post's
/// trade, given its job label, marked `placed`, and spawned at the post tagged
/// with their home. Posts with no eligible adult left (more posts than adults)
/// are returned unfilled for the caller to backfill with anonymous fixtures.
/// Returns `(residents_bound, unfilled_posts)`. Offline: binds nothing, hands
/// every post back.
pub async fn bind_workers(
    editor: &Editor,
    population: &mut Population,
    slots: Vec<WorkerSlot>,
    data: &NpcData,
    rng: &mut RNG,
) -> anyhow::Result<(usize, Vec<WorkerSlot>)> {
    if editor.is_offline() {
        return Ok((0, slots));
    }
    let mut bound = 0usize;
    let mut unfilled: Vec<WorkerSlot> = Vec::new();
    for slot in slots {
        let Some((hi, mi)) =
            draft_worker(&population.households, slot.workplace, &slot.looks, rng)
        else {
            unfilled.push(slot);
            continue;
        };
        // Dress the resident in the post's trade and bake its job label, then
        // mark them placed so `populate_town` skips them.
        let look = *rng.choose(&slot.looks);
        let home = population.households[hi].home;
        {
            let m = &mut population.households[hi].members[mi];
            m.look = look;
            m.employment = Some(slot.employment.clone());
            m.placed = true;
        }
        // Spawn this specific resident via the shared scene path (name, dialogue,
        // home tag) with the post's look forced.
        let npc = population.households[hi].members[mi].clone();
        let scene = AnchorScene::worker(slot.stand, slot.facing, look);
        let mut one = vec![npc];
        bound += staff_scene(editor, &scene, &mut one, data, rng, Some(home)).await?;
    }
    Ok((bound, unfilled))
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

    /// `npcs.yaml` parses (including the surname prefix/suffix pools) and
    /// `roll_surname` produces a non-empty concatenation. No server.
    #[test]
    fn npc_dialogue_pools_resolve() {
        let data = NpcData::load().expect("load npcs.yaml");
        assert!(!data.surname_prefixes.is_empty(), "surname_prefixes must be non-empty");
        assert!(!data.surname_suffixes.is_empty(), "surname_suffixes must be non-empty");
        // The combined surname is the literal prefix+suffix concat (no separator).
        let mut sn_rng = RNG::new(Seed(123));
        let sn = roll_surname(&data, &mut sn_rng);
        assert!(
            data.surname_prefixes.iter().any(|p| sn.starts_with(p)),
            "generated surname '{sn}' should start with one of the prefixes",
        );
        assert!(
            data.surname_suffixes.iter().any(|s| sn.ends_with(s)),
            "generated surname '{sn}' should end with one of the suffixes",
        );
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

    /// Every culture defines a non-empty localized first-name pool, and
    /// `roll_first_name` draws from that culture's list (not another's). Guards
    /// the `cultures` block in `npcs.yaml` against drifting away from the code.
    /// No server.
    #[test]
    fn first_names_are_localized_per_culture() {
        let data = NpcData::load().expect("load npcs.yaml");
        for culture in [Culture::Medieval, Culture::Japanese, Culture::Desert] {
            let pool = data
                .cultures
                .get(culture_key(culture))
                .map(|c| &c.first_names)
                .unwrap_or_else(|| panic!("npcs.yaml: no `cultures.{}` block", culture_key(culture)));
            assert!(
                !pool.is_empty(),
                "culture '{}' has an empty first_names pool",
                culture_key(culture),
            );
            // Several draws all land in this culture's own pool.
            let mut rng = RNG::new(Seed(42));
            for _ in 0..20 {
                let name = roll_first_name(culture, &data, &mut rng);
                assert!(
                    pool.contains(&name),
                    "roll_first_name({}) produced '{name}', not in that culture's pool",
                    culture_key(culture),
                );
            }
        }
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

    /// `display_name` renders epithet-or-surname per spec, but `surname` is
    /// always stored regardless.
    #[test]
    fn display_name_picks_epithet_over_surname() {
        let with_epithet = Npc {
            id: 1,
            first_name: "Doral".into(),
            surname: "Carter".into(),
            epithet: Some("the Quiet".into()),
            life_stage: LifeStage::Elder,
            biome: VillagerBiome::Plains,
            look: NpcLook::Villager(Profession::None),
            employment: None,
            placed: false,
            relationships: Vec::new(),
        };
        assert_eq!(with_epithet.display_name(), "Doral the Quiet");
        assert_eq!(with_epithet.surname, "Carter"); // still stored

        let without = Npc { epithet: None, ..with_epithet.clone() };
        assert_eq!(without.display_name(), "Doral Carter");
    }

    /// The work draft favours nearby unplaced adults, skips placed/non-adults,
    /// and yields `None` once the pool is exhausted.
    #[test]
    fn work_draft_prefers_nearby_unplaced_adults() {
        let workplace = Point2D::new(0, 0);
        let adult = |id: NpcId, pos: Point2D| Household {
            surname: "X".into(),
            home: id as usize,
            pos,
            wealth: Wealth::Modest,
            members: vec![Npc {
                id,
                first_name: "A".into(),
                surname: "X".into(),
                epithet: None,
                life_stage: LifeStage::Adult,
                biome: VillagerBiome::Plains,
                look: NpcLook::Villager(Profession::None),
                employment: None,
                placed: false,
                relationships: Vec::new(),
            }],
        };
        // House 0 sits beside the workplace; house 1 is far across town.
        let mut households = vec![adult(0, Point2D::new(2, 0)), adult(1, Point2D::new(400, 0))];
        let looks = [NpcLook::Villager(Profession::None)];
        let mut rng = RNG::new(Seed(7));

        let near = (0..200)
            .filter(|_| draft_worker(&households, workplace, &looks, &mut rng) == Some((0, 0)))
            .count();
        assert!(near > 180, "near household should dominate, got {near}/200");

        // Once the near adult is committed, the draft falls to the far one.
        households[0].members[0].placed = true;
        assert_eq!(draft_worker(&households, workplace, &looks, &mut rng), Some((1, 0)));

        // Both committed → nothing left to draft.
        households[1].members[0].placed = true;
        assert_eq!(draft_worker(&households, workplace, &looks, &mut rng), None);

        // A lone child is never working-age, so still nothing.
        households[0].members[0].placed = false;
        households[0].members[0].life_stage = LifeStage::Child;
        households[1].members[0].placed = false;
        households[1].members[0].life_stage = LifeStage::Child;
        assert_eq!(draft_worker(&households, workplace, &looks, &mut rng), None);
    }

    /// Build a small batch of households and assert intra-household kinship is
    /// reciprocal (every Parent has a matching Child, every Spouse is mutual,
    /// every Sibling is mutual) and member counts hit each bed budget. No
    /// server.
    #[test]
    fn intra_household_kin_is_reciprocal() {
        let data = NpcData::load().expect("load npcs.yaml");
        let mut rng = RNG::new(Seed(11));
        let mut alloc = IdAllocator::new();
        // A mix of bed budgets covering every shape branch.
        let houses: Vec<HouseAnchors> = (1..=6)
            .map(|pop| HouseAnchors {
                scenes: Vec::new(),
                population: pop,
                wealth: Wealth::Modest,
                pos: Point2D::new(pop as i32 * 16, 0),
                family_color: None,
            })
            .collect();
        let pop = build_households(&houses, Culture::Medieval, &data, &mut alloc, &mut rng);
        assert_eq!(pop.households.len(), houses.len());

        // Every household's primary surname is unique within the town.
        let mut seen = std::collections::HashSet::new();
        for h in &pop.households {
            assert!(
                seen.insert(h.surname.clone()),
                "duplicate primary surname '{}' across households",
                h.surname,
            );
        }

        // Every member is registered in by_id and the lookup round-trips.
        for (h_idx, h) in pop.households.iter().enumerate() {
            for (m_idx, m) in h.members.iter().enumerate() {
                let entry = pop.by_id.get(&m.id).expect("member registered in by_id");
                assert_eq!(*entry, (h_idx, m_idx));
            }
            assert!(!h.members.is_empty(), "every household has at least one member");
        }

        // Every edge has a reciprocal counterpart.
        for h in &pop.households {
            for m in &h.members {
                for r in &m.relationships {
                    let target = pop.get(r.to).expect("relationship target exists");
                    let expected_back = match r.kind {
                        RelationshipKind::Spouse => RelationshipKind::Spouse,
                        RelationshipKind::Sibling => RelationshipKind::Sibling,
                        RelationshipKind::Parent => RelationshipKind::Child,
                        RelationshipKind::Child => RelationshipKind::Parent,
                    };
                    assert!(
                        target.relationships.iter().any(|tr| tr.kind == expected_back && tr.to == m.id),
                        "missing reciprocal {:?} from {} back to {}", expected_back, target.id, m.id,
                    );
                }
            }
        }
    }

    /// `assign_employment` leaves children unemployed plain villagers, mostly
    /// retires elders, and dresses every adult as a villager. No server.
    #[test]
    fn employment_pass_respects_life_stage() {
        let data = NpcData::load().expect("load npcs.yaml");
        let mut rng = RNG::new(Seed(13));
        let mut alloc = IdAllocator::new();
        let houses: Vec<HouseAnchors> = (1..=6)
            .map(|pop| HouseAnchors {
                scenes: Vec::new(),
                population: pop,
                wealth: Wealth::Modest,
                pos: Point2D::new(pop as i32 * 16, 0),
                family_color: None,
            })
            .collect();
        let mut pop = build_households(&houses, Culture::Medieval, &data, &mut alloc, &mut rng);
        assign_employment(&mut pop, &mut rng);
        for h in &pop.households {
            for m in &h.members {
                match m.life_stage {
                    LifeStage::Child => {
                        assert!(m.employment.is_none(), "child stays unemployed");
                        assert_eq!(
                            m.look,
                            NpcLook::Villager(Profession::None),
                            "child is a plain villager",
                        );
                    }
                    // Adults are dressed as villagers; whether they hold a job
                    // depends on the roll (None/Nitwit looks are unemployed).
                    LifeStage::Adult => {
                        assert!(matches!(m.look, NpcLook::Villager(_)), "adult is a villager");
                    }
                    LifeStage::Elder => {} // retired (None) or kept a trade
                }
            }
        }
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
        let mut alloc = IdAllocator::new();

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
        let roster = build_roster(count as usize, 0, Culture::Desert, &data, &mut alloc, &mut rng);
        let placed = populate_npcs(&editor, scenes, roster, count as usize, &data, &mut rng)
            .await
            .expect("populate failed");

        println!("Placed {} NPCs in a demo row at z={}", placed, cz);
        assert!(placed > 0);
    }
}
