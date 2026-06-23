//! Welcome title: a hidden command-block "proximity sensor" that flashes a
//! title on screen the moment a player crosses into the urban area, then stays
//! quiet until they leave and come back.
//!
//! Four always-active command blocks, buried a few blocks under the town centre
//! (a chunk that's loaded and ticking whenever a player is in town). The first
//! three form one east-facing chain, so they fire in order on the SAME tick:
//!
//!   1. *subtitle* — repeating, faces east into the chain. Sets the subtitle
//!      text for every player inside the urban bbox not yet `tag=welcomed`
//!      (`title ... subtitle` only stages the text; it shows when the title fires).
//!   2. *title*    — chain block; shows the title (and the staged subtitle).
//!   3. *tag*      — chain block; tags those players so the banner doesn't
//!      re-fire every tick. Chaining keeps the order deterministic — the title
//!      always shows before the tag suppresses it.
//!   4. *untag*    — a separate repeating block that strips the tag once a player
//!      is back OUTSIDE the box. A player is never both inside and outside, so
//!      this can't race the subtitle/title/tag chain.
//!
//! The trigger region is an axis-aligned box (the urban footprint's bounding
//! box, full build-area height). That over-fires a little on a non-rectangular
//! town — a player skimming the corner outside the wall can still trip it — but
//! it's one cheap selector and good enough for a welcome banner.
//!
//! Requires `enable-command-block=true` in the server's `server.properties`,
//! otherwise the placed blocks exist but never run.

use std::collections::{HashMap, HashSet};

use crate::editor::Editor;
use crate::geometry::{Point2D, Point3D};
use crate::minecraft::Block;

/// Depth below the surface to bury the command blocks at the town centre.
const BURY_DEPTH: i32 = 6;

/// Per-settlement scoreboard tag marking a player already welcomed to *this*
/// town: `welcomed_<slug>`. A unique tag per settlement keeps each town's banner
/// independent — being welcomed at one doesn't suppress another's, and one box's
/// untag can't clear another's. The slug keeps only `[a-z0-9_]` (scoreboard tags
/// can't contain spaces or the `-`/`'` a name like "Bir al-Hamra" carries).
fn welcome_tag(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut prev_us = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            slug.push('_');
            prev_us = true;
        }
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "welcomed".to_string()
    } else {
        format!("welcomed_{slug}")
    }
}

/// Place the welcome-title sensor over the urban area. `urban` is in build-area
/// local coordinates (as returned by `World::get_urban_points`); `name` is the
/// settlement name shown as the (white) title and `subtitle` the line beneath it.
pub async fn place_welcome_title(
    editor: &mut Editor,
    urban: &HashSet<Point2D>,
    name: &str,
    subtitle: &str,
) {
    if urban.is_empty() {
        return;
    }

    // Urban bounding box, in LOCAL coords.
    let (mut min_x, mut min_z, mut max_x, mut max_z) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for p in urban {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.y);
        max_z = max_z.max(p.y);
    }
    let centre = Point2D::new((min_x + max_x) / 2, (min_z + max_z) / 2);

    // The `@a[...]` selector needs ABSOLUTE world coords, so add the build-area
    // origin. `dx/dy/dz` are spans (the box is [x, x+dx] inclusive), and the box
    // covers the full build-area height so the trigger fires at any altitude.
    let origin = editor.world().build_area.origin;
    let size = editor.world().build_area.size;
    let region = format!(
        "x={},y={},z={},dx={},dy={},dz={}",
        min_x + origin.x,
        origin.y,
        min_z + origin.z,
        (max_x - min_x).max(0),
        (size.y - 1).max(0),
        (max_z - min_z).max(0),
    );

    // The Command value is stored as a single-quoted SNBT string, so the title
    // JSON's double quotes survive as literals. Escape backslashes and single
    // quotes the text might carry so the SNBT string stays well-formed.
    let escape = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'");
    let safe_name = escape(name);
    let safe_subtitle = escape(subtitle);
    // Per-settlement tag so each town's banner fires independently.
    let tag = welcome_tag(name);
    // The selector for the two display commands: inside the box, not yet welcomed.
    let unwelcomed = format!("@a[{region},tag=!{tag}]");
    // Stage the subtitle, then show the title — `subtitle` only sets the text;
    // it appears when `title` fires.
    let subtitle_cmd =
        format!("title {unwelcomed} subtitle {{\"text\":\"{safe_subtitle}\",\"color\":\"gray\"}}");
    let title_cmd =
        format!("title {unwelcomed} title {{\"text\":\"{safe_name}\",\"color\":\"white\"}}");
    let tag_cmd = format!("tag @a[{region}] add {tag}");
    let untag_cmd =
        format!("execute as @a[tag={tag}] unless entity @s[{region}] run tag @s remove {tag}");

    // `auto:1b` = "Always Active" (runs without redstone). The chain block in
    // Always-Active mode fires when the block pointing into it (the repeating
    // title block, facing east) executes.
    let command_block = |id: &str, command: &str| Block {
        id: id.into(),
        state: Some(HashMap::from([
            ("facing".to_string(), "east".to_string()),
            ("conditional".to_string(), "false".to_string()),
        ])),
        data: Some(format!("{{Command:'{command}',auto:1b}}")),
    };

    // Bury at the town centre, still inside a chunk that loads when players are
    // in town.
    let surface_y = editor.world().get_height_at(centre);
    let base = Point3D::new(centre.x, surface_y - BURY_DEPTH, centre.y);

    // Chain (west→east): subtitle → title → tag. Each fires the next on the same
    // tick. The untag block sits on its own, one row over.
    editor
        .place_block(&command_block("repeating_command_block", &subtitle_cmd), base)
        .await;
    editor
        .place_block(
            &command_block("chain_command_block", &title_cmd),
            base + Point3D::new(1, 0, 0),
        )
        .await;
    editor
        .place_block(
            &command_block("chain_command_block", &tag_cmd),
            base + Point3D::new(2, 0, 0),
        )
        .await;
    editor
        .place_block(
            &command_block("repeating_command_block", &untag_cmd),
            base + Point3D::new(0, 0, 2),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::welcome_tag;

    #[test]
    fn tag_slugs_are_selector_safe() {
        assert_eq!(welcome_tag("Millford"), "welcomed_millford");
        assert_eq!(welcome_tag("Bir al-Hamra"), "welcomed_bir_al_hamra");
        assert_eq!(welcome_tag("Al-Wadi"), "welcomed_al_wadi");
        assert_eq!(welcome_tag("Frost Hollow"), "welcomed_frost_hollow");
        // Trailing/leading punctuation doesn't leave dangling underscores.
        assert_eq!(welcome_tag("'Oasis'"), "welcomed_oasis");
        // Degenerate name falls back to the bare tag.
        assert_eq!(welcome_tag("---"), "welcomed");
    }
}
