//! Town NPCs: static villagers that stand in place (`NoAI`), wear a floating
//! name tag, and carry a "dialogue bubble" — a [`text_display`] entity hovering
//! above their head that shows a line of text.
//!
//! Entities (unlike blocks) are spawned straight to the server via
//! [`Editor::spawn_entity`] and bypass the block buffer/cache, so they are
//! invisible to later `get_block` reads and to offline/dry-run mode.
//!
//! Text is written as a native SNBT text component (`{text:"…"}`), which is the
//! 1.21.5+ form the server (1.21.11) expects for `text_display` and entity
//! `CustomName`.

use crate::editor::Editor;
use crate::geometry::Point3D;

/// How many blocks above the NPC's feet the dialogue bubble's cell sits, plus a
/// fractional raise for fine height — together ~2.5 blocks up, clear of the head
/// without floating too high above the name tag.
const BUBBLE_HEIGHT: i32 = 2;
const BUBBLE_RAISE: f32 = 0.5;

/// Dialogue text colour — a soft light gray so it reads as muted speech rather
/// than a stark white label. Hex text-component colour (vanilla "gray" is darker).
const DIALOGUE_COLOR: &str = "#C8C8C8";

/// Max line width (in pixels) before the bubble wraps to a new line. Keeps long
/// dialogue as a tidy block instead of one runaway line.
const DIALOGUE_LINE_WIDTH: i32 = 160;

/// Uniform scale of the dialogue text (1.0 = vanilla size). Shrunk so the bubble
/// reads as small floating speech rather than a billboard.
const DIALOGUE_SCALE: f32 = 0.6;

/// How far (roughly) the dialogue bubble stays visible. Render distance is
/// `view_range * 64` blocks (scaled by the client's entity-distance setting), so
/// 0.25 ≈ 16 blocks — the bubble fades out unless the player is close.
const DIALOGUE_VIEW_RANGE: f32 = 0.03;

// --- "Yelled" dialogue: a market crier's hawk or a stage performer's call.
// Larger, bolder, warmer, and visible from much further than ordinary speech, so
// it reads as a shout carrying across a square rather than a quiet aside. It also
// floats a touch higher so it clears the heads of any crowd between the speaker
// and the player.
const YELL_HEIGHT: i32 = 3;
const YELL_RAISE: f32 = 0.0;
/// Warm parchment-gold so a shout stands out from the muted gray of small talk.
const YELL_COLOR: &str = "#FFE9A8";
const YELL_LINE_WIDTH: i32 = 220;
const YELL_SCALE: f32 = 1.05;
/// ~10 blocks (`view_range * 64`): a yell carries noticeably further than a
/// normal close-range aside, but still only reads once the player is fairly near.
const YELL_VIEW_RANGE: f32 = 0.16;

/// How loudly an NPC's dialogue bubble reads. `Normal` is a quiet, close-range
/// aside; `Yelled` is a big, bold, far-visible shout for market criers and stage
/// performers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogueVolume {
    #[default]
    Normal,
    Yelled,
}

/// The per-volume bubble styling: how high it floats, its colour, wrap width,
/// uniform scale, fade-out range, and whether the text is bold.
struct BubbleStyle {
    height: i32,
    raise: f32,
    color: &'static str,
    line_width: i32,
    scale: f32,
    view_range: f32,
    bold: bool,
}

impl DialogueVolume {
    fn style(self) -> BubbleStyle {
        match self {
            DialogueVolume::Normal => BubbleStyle {
                height: BUBBLE_HEIGHT,
                raise: BUBBLE_RAISE,
                color: DIALOGUE_COLOR,
                line_width: DIALOGUE_LINE_WIDTH,
                scale: DIALOGUE_SCALE,
                view_range: DIALOGUE_VIEW_RANGE,
                bold: false,
            },
            DialogueVolume::Yelled => BubbleStyle {
                height: YELL_HEIGHT,
                raise: YELL_RAISE,
                color: YELL_COLOR,
                line_width: YELL_LINE_WIDTH,
                scale: YELL_SCALE,
                view_range: YELL_VIEW_RANGE,
                bold: true,
            },
        }
    }
}

/// The villager's biome "type" — the skin/outfit variant. These are the seven
/// vanilla villager types. See <https://minecraft.wiki/w/Villager>.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VillagerBiome {
    Plains,
    Desert,
    Jungle,
    Savanna,
    Snow,
    Swamp,
    Taiga,
}

impl VillagerBiome {
    /// The `minecraft:`-namespaced id used in the villager's `VillagerData.type`.
    pub fn id(self) -> &'static str {
        match self {
            VillagerBiome::Plains => "minecraft:plains",
            VillagerBiome::Desert => "minecraft:desert",
            VillagerBiome::Jungle => "minecraft:jungle",
            VillagerBiome::Savanna => "minecraft:savanna",
            VillagerBiome::Snow => "minecraft:snow",
            VillagerBiome::Swamp => "minecraft:swamp",
            VillagerBiome::Taiga => "minecraft:taiga",
        }
    }
}

/// The villager's profession — sets its outfit (and, in vanilla, its trades).
/// `None` is the unemployed green robe and `Nitwit` the lazy green-robe variant;
/// the rest are the working professions. See <https://minecraft.wiki/w/Villager>.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profession {
    None,
    Armorer,
    Butcher,
    Cartographer,
    Cleric,
    Farmer,
    Fisherman,
    Fletcher,
    Leatherworker,
    Librarian,
    Mason,
    Nitwit,
    Shepherd,
    Toolsmith,
    Weaponsmith,
}

impl Profession {
    /// The `minecraft:`-namespaced id used in `VillagerData.profession`.
    pub fn id(self) -> &'static str {
        match self {
            Profession::None => "minecraft:none",
            Profession::Armorer => "minecraft:armorer",
            Profession::Butcher => "minecraft:butcher",
            Profession::Cartographer => "minecraft:cartographer",
            Profession::Cleric => "minecraft:cleric",
            Profession::Farmer => "minecraft:farmer",
            Profession::Fisherman => "minecraft:fisherman",
            Profession::Fletcher => "minecraft:fletcher",
            Profession::Leatherworker => "minecraft:leatherworker",
            Profession::Librarian => "minecraft:librarian",
            Profession::Mason => "minecraft:mason",
            Profession::Nitwit => "minecraft:nitwit",
            Profession::Shepherd => "minecraft:shepherd",
            Profession::Toolsmith => "minecraft:toolsmith",
            Profession::Weaponsmith => "minecraft:weaponsmith",
        }
    }
}

/// A non-villager townsfolk mob we can drop in as a static fixture, the same way
/// as [`spawn_villager_npc`] but without the villager-specific biome/profession.
/// Witches and wandering traders are the colourful background characters that
/// make a town feel lived-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mob {
    Witch,
    WanderingTrader,
    Evoker,
    Pillager,
    Vindicator,
    Husk,
    ZombieVillager,
}

impl Mob {
    /// The `minecraft:`-namespaced entity id passed to [`Editor::spawn_entity`].
    pub fn id(self) -> &'static str {
        match self {
            Mob::Witch => "minecraft:witch",
            Mob::WanderingTrader => "minecraft:wandering_trader",
            Mob::Evoker => "minecraft:evoker",
            Mob::Pillager => "minecraft:pillager",
            Mob::Vindicator => "minecraft:vindicator",
            Mob::Husk => "minecraft:husk",
            Mob::ZombieVillager => "minecraft:zombie_villager",
        }
    }
}

/// Escape a user string for embedding inside a double-quoted SNBT string.
fn escape_snbt(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Spawn a stationary villager NPC at build-area-local `point` (its feet),
/// facing `angle` (yaw in degrees, 0 = south, like vanilla), with a `name` tag
/// and a `dialogue` bubble floating above its head. `biome` and `profession`
/// pick the villager's appearance.
///
/// `home`, when set, is the index of the house this NPC belongs to; it's baked
/// into the villager's entity `Tags` as `home_<id>` so the spawned NPC carries a
/// persistent, queryable record of which house it came from (residents have one;
/// workplace/plaza fixtures pass `None`).
///
/// `child`, when true, spawns a baby villager: `Age` is pinned to the most
/// negative value so it never grows up (baby `Age` ticks toward 0; starting at
/// `i32::MIN` keeps it a child for any realistic run).
///
/// The villager has `NoAI` so it won't wander or turn, and `CustomNameVisible`
/// so the name tag always shows. The bubble is a center-billboarded
/// `text_display`, so it always rotates to face the player.
pub async fn spawn_villager_npc(
    editor: &Editor,
    point: Point3D,
    angle: f32,
    name: &str,
    dialogue: &str,
    biome: VillagerBiome,
    profession: Profession,
    volume: DialogueVolume,
    home: Option<usize>,
    child: bool,
    y_offset: f32,
) -> anyhow::Result<()> {
    // The villager itself: frozen, named, faced, and skinned by biome/profession.
    // level:1 so a working profession shows its job outfit (level 0 reads as none).
    // A resident also carries a `home_<id>` entity tag recording its house.
    let home_tag = match home {
        Some(id) => format!(",Tags:[\"home_{id}\"]"),
        None => String::new(),
    };
    // Babies start at the most negative Age so they never visibly grow up.
    let age_tag = if child { format!(",Age:{}", i32::MIN) } else { String::new() };
    let villager_data = format!(
        "{{NoAI:1b,CustomName:{{text:\"{}\"}},CustomNameVisible:1b,Rotation:[{}f,0f],\
         VillagerData:{{type:\"{}\",profession:\"{}\",level:1}}{}{}}}",
        escape_snbt(name),
        angle,
        biome.id(),
        profession.id(),
        age_tag,
        home_tag,
    );
    // Raise onto a slab top (e.g. a tower battlement) when asked; otherwise keep
    // the integer-grid spawn so ground NPCs are unchanged.
    if y_offset != 0.0 {
        editor
            .spawn_entity_offset("minecraft:villager", point, y_offset, Some(&villager_data))
            .await?;
    } else {
        editor
            .spawn_entity("minecraft:villager", point, Some(&villager_data))
            .await?;
    }

    spawn_dialogue_bubble(editor, point, angle, dialogue, volume).await
}

/// Spawn a stationary mob NPC (e.g. a [`Mob::Witch`] or [`Mob::WanderingTrader`])
/// at build-area-local `point` (its feet), facing `angle`, with a `name` tag and
/// a `dialogue` bubble floating above its head.
///
/// This is the villager-free counterpart to [`spawn_villager_npc`]: the same
/// frozen (`NoAI`), named, faced, talking fixture, but for the colourful
/// non-villager townsfolk that don't carry a biome or profession.
pub async fn spawn_mob_npc(
    editor: &Editor,
    point: Point3D,
    angle: f32,
    name: &str,
    dialogue: &str,
    mob: Mob,
    volume: DialogueVolume,
) -> anyhow::Result<()> {
    // Frozen, named, and faced — same fixture treatment as a villager NPC.
    let mob_data = format!(
        "{{NoAI:1b,CustomName:{{text:\"{}\"}},CustomNameVisible:1b,Rotation:[{}f,0f]}}",
        escape_snbt(name),
        angle,
    );
    editor.spawn_entity(mob.id(), point, Some(&mob_data)).await?;

    spawn_dialogue_bubble(editor, point, angle, dialogue, volume).await
}

/// Spawn the dialogue bubble: a `text_display` hovering above an NPC's head,
/// styled by how loud the line is (a quiet aside vs. a far-carrying yell). The
/// transformation nudges it up a fractional block (entity coords are integer).
/// Shared by [`spawn_villager_npc`] and [`spawn_mob_npc`].
async fn spawn_dialogue_bubble(
    editor: &Editor,
    point: Point3D,
    angle: f32,
    dialogue: &str,
    volume: DialogueVolume,
) -> anyhow::Result<()> {
    let style = volume.style();
    let bubble = point + Point3D::new(0, style.height, 0);
    let bubble_data = format!(
        "{{text:{{text:\"{}\",color:\"{}\",bold:{}b}},line_width:{},billboard:\"center\",see_through:1b,alignment:\"center\",view_range:{}f,\
         transformation:{{translation:[0f,{}f,0f],scale:[{s}f,{s}f,{s}f],left_rotation:[0f,0f,0f,1f],right_rotation:[0f,0f,0f,1f]}},\
         Rotation:[{}f,0f]}}",
        escape_snbt(dialogue),
        style.color,
        i32::from(style.bold),
        style.line_width,
        style.view_range,
        style.raise,
        angle,
        s = style.scale,
    );
    editor
        .spawn_entity("minecraft:text_display", bubble, Some(&bubble_data))
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Editor, World};
    use crate::geometry::{Point2D, Point3D};
    use crate::http_mod::GDMCHTTPProvider;
    use crate::util::init_logger;

    /// Spawn a placeholder NPC at the centre of the build area, on the ground, so
    /// it can be eyeballed in-game. Needs a live server.
    /// Run with: `cargo test spawn_test_npc -- --nocapture`.
    #[tokio::test]
    async fn spawn_test_npc() {
        init_logger();

        let provider = GDMCHTTPProvider::new();
        let build_area = provider.get_build_area().await.expect("Failed to get build area");
        let world = World::new(&provider).await.expect("Failed to create world");
        let editor = Editor::new(build_area, world);

        // Build-area-local coordinates: the editor adds the origin back on spawn.
        let size = editor.world().world_rect_2d().size;
        let centre = Point2D::new(size.x / 2, size.y / 2);
        let ground = editor.world().get_ocean_floor_height_at(centre);
        let feet = Point3D::new(centre.x, ground, centre.y);

        spawn_villager_npc(
            &editor,
            feet,
            180.0, // face north (toward a player approaching from +z)
            "Hilda the Baker",
            "Fresh bread, half price today! Loaves still warm from the oven, \
             and the honey rolls are going fast — best grab a few before my \
             neighbour buys the lot again.",
            VillagerBiome::Desert,
            Profession::Butcher,
            DialogueVolume::Normal,
            Some(0),
            false,
            0.0,
        )
        .await
        .expect("failed to spawn NPC");

        println!("Spawned test NPC at local {:?} (ground y={})", centre, ground);
    }
}
