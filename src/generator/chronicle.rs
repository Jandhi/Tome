use anyhow::Ok;
use log::{error, info};
use schemars::JsonSchema;
use serde_derive::Serialize;

use crate::{ai::try_ai_json, editor::Editor};

/// A single narratable place in the town — a road, manor, trade, park, gate, etc.
/// This is a view-model: a flat, stringly-typed digest assembled from the typed
/// generator state purely so the chronicle LLM can write about it. All fields are
/// human-readable (colour words, road names, blazons), never raw ids/coords.
#[derive(Debug, Clone, Serialize)]
pub struct Landmark {
    /// "road" | "industry" | "manor" | "park" | "gate" — groups the labelled block.
    pub kind: String,
    /// e.g. "Mill Lane", "the Blackwell Manor", "the flower garden".
    pub name: String,
    /// Coarse location relative to the town centre: "central", "eastern",
    /// "on the northern edge". Deterministic, so it can't contradict the build.
    pub quarter: String,
    /// Named anchors the place sits by — usually the nearest road. Preferred over
    /// `quarter` in prose because it matches the world's own signage.
    pub near: Vec<String>,
    /// Freeform type-specific detail: a manor's colour + blazon, a park's subtype.
    pub notes: Vec<String>,
    /// Name of the named district this place sits in (see [`DossierDistrict`]).
    /// Empty for places with no urban district — gates and the like — which the
    /// guide collects under a trailing "around the edge" section.
    pub district: String,
    /// World `(x, y, z)` to stand the player at — used to turn the place's name
    /// into a clickable `/tp` link in the finished book. `None` if no ground
    /// height could be sampled (off-map centroid).
    pub tp: Option<(i32, i32, i32)>,
}

/// A named urban district — the unit the chronicle is organised around. Each is a
/// contiguous neighbourhood of the city with its own palette and a procedurally
/// generated, culturally-flavoured name (see `districts/naming.rs`).
#[derive(Debug, Clone, Serialize)]
pub struct DossierDistrict {
    /// e.g. "Smith Row", "Kajimachi", "Hayy as-Souk".
    pub name: String,
    /// Coarse location relative to the town centre: "central", "eastern", … —
    /// matches the `quarter` on the landmarks within it.
    pub quarter: String,
}

/// Everything the chronicle needs to write a guide to the town. Scalars describe
/// what the place *is*; `landmarks` is what's *in* it. Assembled once at the end
/// of `generate_town`, where every source is still in scope, and handed to
/// [`generate_chronicle`]. Serializable so a run can dump it to inspect the exact
/// facts the model was given.
#[derive(Debug, Clone, Serialize)]
pub struct CityDossier {
    pub name: String,
    /// English gloss of the name, e.g. "Dark Hill". May be empty.
    pub subtitle: String,
    /// Culture word ("medieval"/"desert"/"japanese"); empty if unknown.
    pub culture: String,
    /// Town colours as English words ("deep red", "black").
    pub town_colours: Vec<String>,
    /// English blazon of the town's civic banner (flown on the towers/gates),
    /// e.g. "a red cross on a white background". Empty if the town has no banner.
    pub civic_blazon: String,
    /// Most common biomes, prettified ("forest", "river").
    pub biomes: Vec<String>,
    /// Size bucket ("hamlet"/"village"/"town"/"large town").
    pub size: String,
    pub walled: bool,
    /// The town's named districts, in stable order (by quarter then name). The
    /// chronicle walks these one at a time; empty for the minimal pipeline.
    pub districts: Vec<DossierDistrict>,
    pub landmarks: Vec<Landmark>,
}

/// House-count → size word for prose.
pub fn size_word(houses: usize) -> String {
    match houses {
        0..=7 => "hamlet",
        8..=24 => "village",
        25..=59 => "town",
        _ => "large town",
    }
    .to_string()
}

/// Join words naturally: `["a","b","c"]` → "a, b and c".
fn join_natural(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

/// One landmark as a guide line: `"  name [kind] — by <road> (<notes>)"`. The
/// `[kind]` tag tells the model what the thing is, since district blocks mix kinds.
fn landmark_line(m: &Landmark) -> String {
    let kind = match m.kind.as_str() {
        "road" => "street",
        "industry" => "trade",
        "manor" => "family",
        "park" => "green",
        other => other, // "gate", or any future kind, verbatim
    };
    let mut line = format!("  {} [{}]", m.name, kind);
    // Prefer "by <road>"; fall back to the coarse quarter.
    if let Some(r) = m.near.first() {
        line.push_str(&format!(" — by {r}"));
    } else if !m.quarter.is_empty() {
        line.push_str(&format!(" — {}", m.quarter));
    }
    if !m.notes.is_empty() {
        line.push_str(&format!(" ({})", m.notes.join("; ")));
    }
    line
}

/// Emit a fact block: a header line, then one [`landmark_line`] per landmark.
/// Skips the block entirely when there's nothing in it.
fn push_block(s: &mut String, header: &str, marks: &[&Landmark]) {
    if marks.is_empty() {
        return;
    }
    s.push_str(header);
    s.push('\n');
    for m in marks {
        s.push_str(&landmark_line(m));
        s.push('\n');
    }
    s.push('\n');
}

/// Build the content instruction for the chronicle book from the dossier. The
/// formatting rules are appended later by [`give_player_book`]; this is purely
/// the persona, the facts, and the grounding rules. Pure string-building — no
/// I/O — so it's fully testable offline.
pub fn build_instruction(d: &CityDossier) -> String {
    let mut s = String::new();

    // ── Persona: a travel guide with a point of view, not a fact list ──
    s.push_str(&format!(
        "You are writing a short visitor's guide to {name}, in the voice of a good travel \
         guide — the kind that gives a traveller a feel for a place, not just a list of what \
         is there. Write with personality and a point of view: tell the reader what the town \
         is like to arrive in, what is worth their time, and what gives it its character. \
         Stay concrete and grounded in present-tense English, but let the prose breathe — a \
         vivid phrase, a wry observation, a recommendation worth following. Steer clear of \
         both dry fact-listing and overwrought storytelling; aim for the warm, knowing voice \
         of a guidebook a traveller actually enjoys reading.\n\n",
        name = d.name,
    ));

    // ── Facts as labelled blocks the model selects from ──
    s.push_str("Here is everything true about the town. Use ONLY what is listed here.\n\n");

    s.push_str("THE PLACE\n");
    if d.subtitle.is_empty() {
        s.push_str(&format!("  Name: {}\n", d.name));
    } else {
        s.push_str(&format!("  Name: {} — \"{}\"\n", d.name, d.subtitle));
    }
    let walled = if d.walled { " walled" } else { "" };
    if d.culture.is_empty() {
        s.push_str(&format!("  A{walled} {}\n", d.size));
    } else {
        s.push_str(&format!("  A{walled} {} in the {} style\n", d.size, d.culture));
    }
    if !d.biomes.is_empty() {
        s.push_str(&format!("  Set in {} country\n", join_natural(&d.biomes)));
    }
    if !d.town_colours.is_empty() {
        s.push_str(&format!("  The town flies {}\n", join_natural(&d.town_colours)));
    }
    if !d.civic_blazon.is_empty() {
        s.push_str(&format!("  Its civic banner, on the towers and gates: {}\n", d.civic_blazon));
    }
    s.push('\n');

    // ── Facts grouped BY DISTRICT — the unit the guide walks through ──
    // Each block is a named district and everything in it (streets, trades,
    // families, greens, all mixed), so the model writes the place as a place
    // rather than a category list. Districts come pre-sorted by quarter.
    if !d.districts.is_empty() {
        s.push_str(
            "The town divides into these named DISTRICTS. Walk the guide through them one at a \
             time, weaving together what each one holds.\n\n",
        );
    }
    let known: std::collections::HashSet<&str> =
        d.districts.iter().map(|x| x.name.as_str()).collect();
    for dist in &d.districts {
        let marks: Vec<&Landmark> =
            d.landmarks.iter().filter(|m| m.district == dist.name).collect();
        let header = if dist.quarter.is_empty() {
            dist.name.to_uppercase()
        } else {
            format!("{} — {}", dist.name.to_uppercase(), dist.quarter)
        };
        push_block(&mut s, &header, &marks);
    }
    // Anything with no (recognised) district — gates on the wall and the like.
    let edge: Vec<&Landmark> = d
        .landmarks
        .iter()
        .filter(|m| m.district.is_empty() || !known.contains(m.district.as_str()))
        .collect();
    push_block(&mut s, "AROUND THE EDGE", &edge);

    // ── Shape + grounding ──
    // The opener is fixed in spirit: every guide must begin "Welcome to {name}, …"
    // and, in the same breath, say what makes the place distinctive.
    s.push_str(&format!(
        "Begin the very FIRST page with one warm, welcoming sentence that opens literally with \
         \"Welcome to {name}, \" and then, in the same breath, says what makes the place \
         distinctive — its setting, its colours, a defining trade, or the families who hold it \
         (e.g. \"Welcome to {name}, the river town where the smiths' hammers never rest\"). Keep \
         it to a sentence or two; let it set the tone before the guide proper.\n\n",
        name = d.name,
    ));
    s.push_str(
        "Then organise the rest as a handful of SHORT titled sections, each opening with a brief \
         bold heading and then a few flowing lines. Follow the welcome with an at-a-glance sense \
         of the town (what kind of place it is, its colours and setting, the feel of arriving). \
         Then give roughly ONE section to each named DISTRICT, in turn: use the district's own \
         name as the heading, and describe what a traveller would find as they walk it — weaving \
         its streets, trades, families and greens together into a sense of that one place, not a \
         category list. A district with little in it can share a short section or a single line. \
         Fold the gates and edge into 'getting in' or the district they adjoin. Close with \
         whatever is worth a last word.\n\n\
         The ruling FAMILIES — the manor households listed under the districts — must each be \
         named somewhere in the guide, with the colours they fly; they are who holds the town, \
         and a visitor should know them.\n\n\
         Use ONLY the facts above. You may add light colour — a smell, a sound, the feel of a \
         place, what a traveller might do there — but invent NO new buildings, people, \
         families, or places, and never contradict the facts. If something is not listed, the \
         town does not have it. Do not name every street and trade; pick the ones worth a \
         visitor's time and give them life. When you do name one of the places listed above, \
         use its exact wording where it reads naturally — those names become clickable links in \
         the finished book. It should read like a guidebook a traveller enjoys, not a list of \
         facts.",
    );

    s
}

#[derive(Debug, serde_derive::Deserialize, Serialize, JsonSchema)]
struct Text {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<TextColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    underlined: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strikethrough: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    obfuscated: Option<bool>,
}

#[derive(Debug, serde_derive::Deserialize, Serialize, JsonSchema)]
enum TextColors {
    #[serde(rename = "dark_blue")]
    DarkBlue,
    #[serde(rename = "dark_green")]
    DarkGreen,
    #[serde(rename = "dark_aqua")]
    DarkAqua,
    #[serde(rename = "dark_red")]
    DarkRed,
    #[serde(rename = "dark_purple")]
    DarkPurple,
    #[serde(rename = "gold")]
    Gold,
    #[serde(rename = "gray")]
    Gray,
    #[serde(rename = "dark_gray")]
    DarkGray,
    #[serde(rename = "blue")]
    Blue,
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "aqua")]
    Aqua,
    #[serde(rename = "red")]
    Red,
    #[serde(rename = "light_purple")]
    LightPurple,
    #[serde(rename = "yellow")]
    Yellow,
    #[serde(rename = "white")]
    White,
    #[serde(rename = "black")]
    #[serde(other)]
    Black,
}

#[derive(Debug, serde_derive::Deserialize, JsonSchema)]
struct Book {
    title: String,
    author : String,
    pages : Vec<Vec<Text>>,
}

/// Run the instruction through the LLM and return a formatted [`Book`] — the
/// network half, with no Minecraft I/O. Split out so the live model can be
/// exercised in a test without a running server (see `tests::live_llm_*`).
async fn write_book(instruction : &str) -> anyhow::Result<Book> {
    let user = &format!(r#"{}.

            Formatting rules — follow exactly:
            - Each entry in `pages` is ONE physical Minecraft book page, which shows only ~14 narrow lines of about 19 characters each (so a normal sentence wraps to 2+ lines). Keep each page to AT MOST about 5 short lines / ~120 characters of text INCLUDING its heading. This is small on purpose — leaving a page half-empty is correct, and packing a page full means the bottom is CUT OFF in-game and lost.
            - Use MANY pages — 8 to 14 is normal and good. A section with several facts must SPAN multiple pages rather than overfill one: continue it on the next page (repeat the heading, or add " (cont.)"). One fact or two per page is fine.
            - Body text is PLAIN by default: no color, no bold, no italics. This is true for the vast majority of the text.
            - A page may open with a short heading that is bold (bold only, never colored). If it does, the heading's own text must END with a newline so it sits on its own line — the body must never run straight on from the heading (no "HeadingBody...").
            - Color at most ONE word or short phrase per page, and only for a genuinely important name or concept. Most pages should have NO colored text at all. Treat color like rare rubrication in an old manuscript, not a highlighter.
            - NEVER use yellow text — it is illegible on the book's pale page. If a name's colour would be yellow (or gold/white), leave that text plain instead.
            - Never write "Keyword:" labels, glossaries, or lists of highlighted terms.
            - Adjacent text components are concatenated with NO space added. Whenever you split text into multiple components (to color a word, or between sentences/paragraphs), put the needed spaces and line breaks INSIDE the components so the rendered run reads with normal spacing — never "endOfOne.startOfNext".
            - When in doubt, leave text plain.

            Keep the title and author under 32 characters each.
            Do NOT use § section codes or unicode escape codes — format only using the JSON elements."#, instruction);
    try_ai_json::<Book>(user).await
        .ok_or_else(|| anyhow::anyhow!("Failed to get or parse AI response for book"))
}

/// A landmark name paired with the world coordinate a `/tp` link should send the
/// player to. Built from the dossier's [`Landmark`]s that carry a `tp` position.
struct LandmarkLink {
    name: String,
    pos: (i32, i32, i32),
}

/// Case-insensitive search for `needle` in `haystack`, returning the byte range of
/// the first match. `to_ascii_lowercase` preserves byte length, so the indices map
/// straight back onto `haystack` (non-ASCII bytes are left untouched on both sides).
fn find_ci(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let (h, n) = (haystack.to_ascii_lowercase(), needle.to_ascii_lowercase());
    h.find(&n).map(|i| (i, i + n.len()))
}

/// Like [`find_ci`] but matches only whole words — the char on each side must be
/// a non-alphanumeric boundary (or the string end). Stops short names colouring
/// inside longer words ("Ash" in "Ashes").
fn find_ci_word(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let h = haystack.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    let mut from = 0;
    while from <= h.len() {
        let Some(rel) = h[from..].find(&n) else { return None };
        let s = from + rel;
        let e = s + n.len();
        let before_ok = s == 0 || !h.as_bytes()[s - 1].is_ascii_alphanumeric();
        let after_ok = e == h.len() || !h.as_bytes()[e].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return Some((s, e));
        }
        from = s + 1;
    }
    None
}

/// Colour every whole-word occurrence of the town `name` like a keyword
/// (rubricated dark red + bold), so the place it's a guide to stands out wherever
/// it appears. Runs after link injection; a linked landmark span never contains
/// the town name, so the two passes don't fight.
fn highlight_name(comps: Vec<serde_json::Value>, name: &str) -> Vec<serde_json::Value> {
    if name.is_empty() {
        return comps;
    }
    let mut out = Vec::with_capacity(comps.len());
    for comp in comps {
        match comp.get("text").and_then(|t| t.as_str()) {
            Some(text) if !text.is_empty() => out.extend(split_name(&comp, text, name)),
            _ => out.push(comp),
        }
    }
    out
}

/// Split one component's `text` around each whole-word occurrence of `name`,
/// colouring the matched spans dark red + bold (overriding any prior colour so the
/// name reads uniformly).
fn split_name(comp: &serde_json::Value, text: &str, name: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((s, e)) = find_ci_word(rest, name) {
        if s > 0 {
            out.push(plain(comp, &rest[..s]));
        }
        let mut v = plain(comp, &rest[s..e]);
        v["color"] = serde_json::Value::String("dark_red".to_string());
        v["bold"] = serde_json::Value::Bool(true);
        out.push(v);
        rest = &rest[e..];
    }
    if !rest.is_empty() || out.is_empty() {
        out.push(plain(comp, rest));
    }
    out
}

/// Turn landmark-name mentions into clickable `/tp` links. Walks each text
/// component; the first time a landmark name appears (across the whole book —
/// `linked` carries state between pages), that span becomes its own component with
/// an underline + colour and a `run_command` click event. Already-linked names and
/// non-matching text are left as prose.
fn link_components(
    comps: Vec<serde_json::Value>,
    links: &[LandmarkLink],
    linked: &mut std::collections::HashSet<String>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(comps.len());
    for comp in comps {
        match comp.get("text").and_then(|t| t.as_str()) {
            Some(text) if !text.is_empty() => out.extend(split_links(&comp, text, links, linked)),
            _ => out.push(comp),
        }
    }
    out
}

/// Split one component's `text` around each earliest unlinked landmark mention,
/// emitting plain spans verbatim and matched spans as link components.
fn split_links(
    comp: &serde_json::Value,
    text: &str,
    links: &[LandmarkLink],
    linked: &mut std::collections::HashSet<String>,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        // Earliest match; on a tie the longest name wins ("the X Manor" over "Manor").
        let next = links
            .iter()
            .filter(|l| !linked.contains(&l.name))
            .filter_map(|l| find_ci(rest, &l.name).map(|(s, e)| (s, e, l)))
            .min_by(|a, b| a.0.cmp(&b.0).then((b.1 - b.0).cmp(&(a.1 - a.0))));
        let Some((s, e, l)) = next else {
            if !rest.is_empty() {
                out.push(plain(comp, rest));
            }
            break;
        };
        if s > 0 {
            out.push(plain(comp, &rest[..s]));
        }
        out.push(linkify(comp, &rest[s..e], l));
        linked.insert(l.name.clone());
        rest = &rest[e..];
    }
    out
}

/// Clone `comp` with a new `text`, keeping its existing styling.
fn plain(comp: &serde_json::Value, text: &str) -> serde_json::Value {
    let mut v = comp.clone();
    v["text"] = serde_json::Value::String(text.to_string());
    v
}

/// Clone `comp` with a new `text`, an underline + colour so it reads as a link, and
/// a `run_command` click event that teleports the player. Uses the 1.21.5+ text
/// component format (`click_event` / `command`, not the old `clickEvent` / `value`).
fn linkify(comp: &serde_json::Value, text: &str, l: &LandmarkLink) -> serde_json::Value {
    let (x, y, z) = l.pos;
    let mut v = plain(comp, text);
    v["underlined"] = serde_json::Value::Bool(true);
    v["color"] = serde_json::Value::String("dark_aqua".to_string());
    v["click_event"] = serde_json::json!({
        "action": "run_command",
        "command": format!("/tp @s {x} {y} {z}"),
    });
    v
}

pub async fn give_player_book(
    editor: &Editor,
    instruction: &str,
    links: &[LandmarkLink],
    town_name: &str,
) -> anyhow::Result<()> {
    let book: Book = write_book(instruction).await?;

    // First mention of each landmark across the whole book becomes the link.
    let mut linked: std::collections::HashSet<String> = std::collections::HashSet::new();
    let pages: Vec<String> = book.pages.iter().map(|page| {
            // Wrap every component under a NEUTRAL empty root, so each keeps its
            // own formatting. (If we promoted the first component to the root, its
            // bold/color would inherit down into every `extra` child — turning a
            // bold heading into a wholly-bold page.) The result is a bare SNBT
            // compound; a text component is valid SNBT, so the component-format
            // book parses it as formatted text rather than printing raw JSON.
            let extra: Vec<serde_json::Value> = page.iter()
                .map(|t| serde_json::to_value(t).unwrap())
                .collect();
            let extra = link_components(extra, links, &mut linked);
            let extra = highlight_name(extra, town_name);
            serde_json::json!({ "text": "", "extra": extra }).to_string()
        }).collect();
    let page_refs = pages.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    if let Err(e) = editor.give_player_book(&page_refs, &book.title, &book.author).await {
        error!("Error while giving player book: {:?}", e);
        return Err(e);
    }

    Ok(())
}

pub async fn generate_chronicle(editor: &Editor, dossier : &CityDossier) -> anyhow::Result<()> {
    let retries = 3;
    let instruction = build_instruction(dossier);
    // Landmarks with a known world position become clickable `/tp` links.
    let links: Vec<LandmarkLink> = dossier
        .landmarks
        .iter()
        .filter_map(|m| m.tp.map(|pos| LandmarkLink { name: m.name.clone(), pos }))
        .collect();

    for _ in 0..retries {
        let result = give_player_book(editor, &instruction, &links, &dossier.name).await;

        match result {
            Result::Ok(()) => {
                info!("Chronicle generated successfully.");
                return anyhow::Ok(());
            },
            Err(e) => {
                error!("Error generating chronicle: {:?}, retrying", e);
            }
        }
    }

    Err(anyhow::anyhow!("Failed to generate chronicle after {} retries", retries))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CityDossier {
        CityDossier {
            name: "Blackbarrow".to_string(),
            subtitle: "Dark Hill".to_string(),
            culture: "medieval".to_string(),
            town_colours: vec!["deep red".to_string(), "black".to_string()],
            civic_blazon: "a red cross on a black background".to_string(),
            biomes: vec!["forest".to_string(), "river".to_string()],
            size: "town".to_string(),
            walled: true,
            districts: vec![
                DossierDistrict { name: "Old Quarter".into(), quarter: "central".into() },
                DossierDistrict { name: "Smith Row".into(), quarter: "eastern".into() },
                DossierDistrict { name: "Garden End".into(), quarter: "on the northern edge".into() },
            ],
            landmarks: vec![
                Landmark { kind: "road".into(), name: "High Street".into(), quarter: "central".into(), near: vec![], notes: vec![], district: "Old Quarter".into(), tp: Some((100, 64, 200)) },
                Landmark { kind: "industry".into(), name: "the mill".into(), quarter: "eastern".into(), near: vec!["Mill Lane".into()], notes: vec![], district: "Smith Row".into(), tp: Some((150, 65, 210)) },
                Landmark { kind: "manor".into(), name: "the Blackwell Manor".into(), quarter: "eastern".into(), near: vec!["the Rivergate".into()], notes: vec!["deep red".into(), "a red cross on a black background".into()], district: "Smith Row".into(), tp: Some((160, 66, 190)) },
                Landmark { kind: "park".into(), name: "the flower garden".into(), quarter: "on the northern edge".into(), near: vec![], notes: vec![], district: "Garden End".into(), tp: Some((120, 64, 250)) },
                Landmark { kind: "gate".into(), name: "the north gate".into(), quarter: "on the northern edge".into(), near: vec![], notes: vec![], district: String::new(), tp: Some((120, 64, 260)) },
            ],
        }
    }

    #[test]
    fn instruction_carries_persona_facts_and_guardrails() {
        let s = build_instruction(&sample());
        // Persona frame: a travel guide, not a fact-sheet.
        assert!(s.contains("short visitor's guide to Blackbarrow"), "{s}");
        assert!(s.contains("voice of a good travel guide"), "guide persona missing:\n{s}");
        assert!(s.contains("guidebook a traveller enjoys, not a list of facts"), "guide shape missing:\n{s}");
        // Scalars.
        assert!(s.contains("Blackbarrow — \"Dark Hill\""), "{s}");
        assert!(s.contains("walled town in the medieval style"), "{s}");
        assert!(s.contains("forest and river country"), "{s}");
        assert!(s.contains("flies deep red and black"), "{s}");
        assert!(s.contains("civic banner, on the towers and gates: a red cross on a black background"), "{s}");
        // Facts are grouped under district headings (NAME — quarter), in
        // quarter-sorted order, each landmark tagged with its kind.
        assert!(s.contains("SMITH ROW — eastern"), "district heading missing:\n{s}");
        assert!(s.contains("GARDEN END — on the northern edge"), "{s}");
        assert!(s.contains("the mill [trade] — by Mill Lane"), "{s}");
        assert!(
            s.contains("the Blackwell Manor [family] — by the Rivergate (deep red; a red cross on a black background)"),
            "{s}",
        );
        assert!(s.contains("the flower garden [green] — on the northern edge"), "{s}");
        // Every guide opens with a "Welcome to <name>, …" sentence.
        assert!(s.contains("\"Welcome to Blackbarrow, \""), "welcome opener missing:\n{s}");
        // The ruling families must be named.
        assert!(s.contains("ruling FAMILIES"), "families requirement missing:\n{s}");
        // District-less landmarks (gates) trail in an edge block.
        assert!(s.contains("AROUND THE EDGE"), "{s}");
        assert!(s.contains("the north gate [gate]"), "{s}");
        // The mill sits in Smith Row, not its own TRADES category.
        assert!(!s.contains("TRADES"), "facts should not be category-grouped:\n{s}");
        // Grounding rule present.
        assert!(s.contains("invent NO new buildings"), "{s}");
    }

    /// Exercises the live LLM end of the chronicle (instruction → `write_book`)
    /// with no Minecraft server. Hits the real API, so it's `#[ignore]`d by
    /// default — run explicitly:
    ///   cargo test chronicle::tests::live_llm_writes_a_book -- --ignored --nocapture
    /// Needs the API key in `.env`. Prints the book so the prose can be eyeballed.
    #[tokio::test]
    #[ignore = "hits the live LLM API; run with --ignored"]
    async fn live_llm_writes_a_book_from_dossier() {
        dotenv::dotenv().ok();
        let instruction = build_instruction(&sample());
        println!("\n===== INSTRUCTION =====\n{instruction}\n");
        let book = write_book(&instruction).await.expect("LLM returned a parseable book");
        assert!(!book.title.is_empty(), "empty title");
        assert!(!book.pages.is_empty(), "no pages");
        println!("\n===== BOOK: {} — by {} =====", book.title, book.author);
        let mut oversized: Vec<(usize, usize)> = Vec::new();
        for (i, page) in book.pages.iter().enumerate() {
            let text: String = page.iter().map(|t| t.text.as_str()).collect();
            let chars = text.chars().count();
            println!("\n--- page {} ({chars} chars) ---\n{text}", i + 1);
            // A Minecraft page renders ~14 lines; well under 200 chars stays inside it.
            if chars > 200 {
                oversized.push((i + 1, chars));
            }
        }
        assert!(
            oversized.is_empty(),
            "pages exceed the book render budget and will clip in-game: {oversized:?}",
        );
        // Grounding smoke check: the town's name should appear somewhere (title or body).
        let all: String = book.pages.iter().flatten().map(|t| t.text.as_str()).collect();
        assert!(
            book.title.contains("Blackbarrow") || all.contains("Blackbarrow"),
            "the town name never appears in the book (title or body)",
        );
    }

    #[test]
    fn landmark_names_become_tp_links() {
        let links = vec![
            LandmarkLink { name: "the mill".into(), pos: (10, 64, 20) },
            LandmarkLink { name: "the Blackwell Manor".into(), pos: (30, 70, 40) },
        ];
        let comps = vec![serde_json::json!({
            "text": "Visit the mill and the Blackwell Manor, then the mill again."
        })];
        let mut linked = std::collections::HashSet::new();
        let out = link_components(comps, &links, &mut linked);

        // The mill span is linked with a run_command /tp to its coords.
        let mill = out.iter().find(|v| v["text"] == "the mill" && v.get("click_event").is_some())
            .expect("mill linked span");
        assert_eq!(mill["click_event"]["action"], "run_command");
        assert_eq!(mill["click_event"]["command"], "/tp @s 10 64 20");
        assert_eq!(mill["underlined"], true);
        // The manor (longer name) links to its own coords.
        let manor = out.iter().find(|v| v["text"] == "the Blackwell Manor")
            .expect("manor linked span");
        assert_eq!(manor["click_event"]["command"], "/tp @s 30 70 40");
        // First mention only: exactly one linked "the mill"; the second stays prose.
        let mill_links = out.iter()
            .filter(|v| v["text"] == "the mill" && v.get("click_event").is_some())
            .count();
        assert_eq!(mill_links, 1, "second mention should not be re-linked");
        let joined: String = out.iter().filter_map(|v| v["text"].as_str()).collect();
        assert!(joined.contains("the mill again"), "prose lost: {joined}");
    }

    #[test]
    fn town_name_is_rubricated() {
        let comps = vec![serde_json::json!({
            "text": "Welcome to Greenton, a fine place. Greenton thrives; Greentonshire is elsewhere."
        })];
        let out = highlight_name(comps, "Greenton");
        // Both whole-word mentions of Greenton are coloured dark_red + bold.
        let hits: Vec<_> = out.iter()
            .filter(|v| v["text"] == "Greenton" && v["color"] == "dark_red" && v["bold"] == true)
            .collect();
        assert_eq!(hits.len(), 2, "expected both Greenton mentions coloured: {out:?}");
        // "Greentonshire" must NOT be split/coloured (word-boundary match).
        let joined: String = out.iter().filter_map(|v| v["text"].as_str()).collect();
        assert!(joined.contains("Greentonshire is elsewhere"), "boundary match broke a word: {joined}");
    }

    #[test]
    fn empty_sections_are_skipped() {
        let d = CityDossier {
            name: "Nowhere".into(),
            subtitle: String::new(),
            culture: String::new(),
            town_colours: vec![],
            civic_blazon: String::new(),
            biomes: vec![],
            size: "hamlet".into(),
            walled: false,
            districts: vec![],
            landmarks: vec![],
        };
        let s = build_instruction(&d);
        assert!(s.contains("Name: Nowhere\n"), "no bare-name line:\n{s}");
        assert!(s.contains("A hamlet\n"), "{s}");
        // No district intro or edge block when there's nothing to place.
        assert!(!s.contains("named DISTRICTS"), "{s}");
        assert!(!s.contains("AROUND THE EDGE"), "{s}");
    }
}