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
    /// Most common biomes, prettified ("forest", "river").
    pub biomes: Vec<String>,
    /// Size bucket ("hamlet"/"village"/"town"/"large town").
    pub size: String,
    pub walled: bool,
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

/// One labelled fact block (e.g. all "industry" landmarks). Skips empty groups.
fn push_block(s: &mut String, header: &str, marks: &[Landmark], kind: &str) {
    let group: Vec<&Landmark> = marks.iter().filter(|m| m.kind == kind).collect();
    if group.is_empty() {
        return;
    }
    s.push_str(header);
    s.push('\n');
    for m in group {
        let mut line = format!("  {}", m.name);
        // Prefer "by <road>"; fall back to the coarse quarter.
        if let Some(r) = m.near.first() {
            line.push_str(&format!(" — by {r}"));
        } else if !m.quarter.is_empty() {
            line.push_str(&format!(" — {}", m.quarter));
        }
        if !m.notes.is_empty() {
            line.push_str(&format!(" ({})", m.notes.join("; ")));
        }
        s.push_str(&line);
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

    // ── Persona: a brisk pocket fact-guide, not a story ──
    s.push_str(&format!(
        "You are writing a short visitor's guide to {name} — a pocket fact-sheet for a \
         traveller passing through, not a story. Keep it brisk, concrete and skimmable: say \
         what the town is and what is worth knowing, in plain present-tense English. Favour \
         short, punchy lines over flowing prose.\n\n",
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
    s.push('\n');

    push_block(&mut s, "THE STREETS", &d.landmarks, "road");
    push_block(&mut s, "TRADES", &d.landmarks, "industry");
    push_block(&mut s, "FAMILIES", &d.landmarks, "manor");
    push_block(&mut s, "GREENS & SQUARES", &d.landmarks, "park");
    push_block(&mut s, "GATES", &d.landmarks, "gate");

    // ── Shape + grounding ──
    s.push_str(
        "Organise the guide as a handful of SHORT titled sections, each opening with a brief \
         bold heading and then a few crisp lines. Good sections to use: an 'At a glance' \
         summary (what kind of town it is, its colours and setting); getting in (the gates \
         and main streets); what to see (notable trades, greens and squares); who runs it \
         (the families and their banners); and where life happens (markets and gathering \
         places). Keep each page light — a section with several items should run across \
         several short pages rather than one crowded page.\n\n\
         Use ONLY the facts above. You may add light colour — a smell, a sound, the feel of a \
         place — but invent NO new buildings, people, families, or places, and never \
         contradict the facts. If something is not listed, the town does not have it. Do not \
         name every street and trade; pick the ones worth a visitor's time. It should read \
         like a fact-sheet a traveller can skim, not a tale.",
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

pub async fn give_player_book(editor : &Editor, instruction : &str) -> anyhow::Result<()> {
    let book: Book = write_book(instruction).await?;

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

    for _ in 0..retries {
        let result = give_player_book(editor, &instruction).await;

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
            biomes: vec!["forest".to_string(), "river".to_string()],
            size: "town".to_string(),
            walled: true,
            landmarks: vec![
                Landmark { kind: "road".into(), name: "High Street".into(), quarter: "central".into(), near: vec![], notes: vec![] },
                Landmark { kind: "industry".into(), name: "the mill".into(), quarter: "eastern".into(), near: vec!["Mill Lane".into()], notes: vec![] },
                Landmark { kind: "manor".into(), name: "the Blackwell Manor".into(), quarter: "eastern".into(), near: vec!["the Rivergate".into()], notes: vec!["deep red".into(), "a red cross on a black background".into()] },
                Landmark { kind: "park".into(), name: "the flower garden".into(), quarter: "on the northern edge".into(), near: vec![], notes: vec![] },
                Landmark { kind: "gate".into(), name: "the north gate".into(), quarter: "on the northern edge".into(), near: vec![], notes: vec![] },
            ],
        }
    }

    #[test]
    fn instruction_carries_persona_facts_and_guardrails() {
        let s = build_instruction(&sample());
        // Persona frame.
        assert!(s.contains("short visitor's guide to Blackbarrow"), "{s}");
        assert!(s.contains("fact-sheet a traveller can skim"), "tldr shape missing:\n{s}");
        // Scalars.
        assert!(s.contains("Blackbarrow — \"Dark Hill\""), "{s}");
        assert!(s.contains("walled town in the medieval style"), "{s}");
        assert!(s.contains("forest and river country"), "{s}");
        assert!(s.contains("flies deep red and black"), "{s}");
        // Labelled blocks render with location + notes.
        assert!(s.contains("THE STREETS"), "{s}");
        assert!(s.contains("the mill — by Mill Lane"), "{s}");
        assert!(s.contains("the Blackwell Manor — by the Rivergate (deep red; a red cross on a black background)"), "{s}");
        assert!(s.contains("the flower garden — on the northern edge"), "{s}");
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
    fn empty_sections_are_skipped() {
        let d = CityDossier {
            name: "Nowhere".into(),
            subtitle: String::new(),
            culture: String::new(),
            town_colours: vec![],
            biomes: vec![],
            size: "hamlet".into(),
            walled: false,
            landmarks: vec![],
        };
        let s = build_instruction(&d);
        assert!(s.contains("Name: Nowhere\n"), "no bare-name line:\n{s}");
        assert!(s.contains("A hamlet\n"), "{s}");
        // No empty headers for absent groups.
        assert!(!s.contains("THE STREETS"), "{s}");
        assert!(!s.contains("FAMILIES"), "{s}");
    }
}