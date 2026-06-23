//! Feature-driven, per-culture settlement naming.
//!
//! Names a town after the things that make it *that* place: its most iconic
//! building, a civic landmark (market, cross, …), the land shape, and the biome.
//!
//! Detection is **culture-neutral** — each feature maps to an abstract *concept*
//! (`water`, `hill`, `market`, `wood`, …). Each culture then translates the
//! concept into its OWN words and morphology, so a river town is `Brookford`
//! (medieval), `Kawamura` (japanese), or `Bir Wadi` (desert) — never an English
//! stem glued onto a foreign suffix (the "Gardenyama" problem).
//!
//! Each detected concept becomes a weighted theme. A lead word is drawn from a
//! weighted theme, then the name is composed as either:
//!   - **stem + suffix** — `Mill` + `ford` → `Millford`, `Yama` + `mura` → `Yamamura`
//!   - **two words** — pairing two themes, `Bir` + `Wadi` → `Bir Wadi`
//!
//! Everything is drawn from the town's seeded RNG, so a given seed always yields
//! the same name. Vocabulary + tuning live in `data/settlement_names.yaml`.

use std::collections::{HashMap, HashSet};

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::editor::World;
use crate::generator::buildings_v2::Culture;
use crate::generator::BuildClaim;
use crate::geometry::{Point2D, CARDINALS_2D};
use crate::noise::RNG;

/// Vocabulary + tuning, loaded from `data/settlement_names.yaml`.
#[derive(Debug, Deserialize)]
struct NamesCfg {
    two_word_pct: i32,
    weights: Weights,
    hilly_relief: i32,
    flat_relief: i32,
    water_share: f64,
    /// structure type -> concept.
    buildings: HashMap<String, String>,
    /// open-space feature key (plaza/park `.key()`) -> concept.
    civic: HashMap<String, String>,
    /// landform kind -> concept.
    landforms: LandformConcepts,
    /// ordered, most-specific first; empty `contains` = fallback.
    biomes: Vec<BiomeRule>,
    /// culture key -> that culture's morphology + concept vocabulary.
    cultures: HashMap<String, CultureWords>,
}

#[derive(Debug, Deserialize)]
struct Weights {
    building: i32,
    civic: i32,
    landform: i32,
    color: i32,
    biome: i32,
}

#[derive(Debug, Deserialize)]
struct LandformConcepts {
    hilly: String,
    flat: String,
    water: String,
}

#[derive(Debug, Deserialize)]
struct BiomeRule {
    contains: Vec<String>,
    concept: String,
}

#[derive(Debug, Deserialize)]
struct CultureWords {
    #[serde(default)]
    suffixes: Vec<String>,
    /// optional per-culture override of the global two-word chance.
    #[serde(default)]
    two_word_pct: Option<i32>,
    /// Arabic-style article composition (`Al-X`, `X al-Y`). When present it
    /// replaces the suffix/two-word forms for this culture.
    #[serde(default)]
    article: Option<ArticleCfg>,
    /// capitalised second words for the two-word form.
    fillers: Vec<String>,
    /// colour modifiers — always lead the name (never a trailing word).
    #[serde(default)]
    colors: Vec<String>,
    /// concept -> this culture's words for it.
    concepts: HashMap<String, Vec<String>>,
}

/// Tuning for definite-article naming. The remainder after `prefix_pct +
/// construct_pct` is the bare two-word form (`Bir Zeit`).
#[derive(Debug, Deserialize)]
struct ArticleCfg {
    /// % chance of the `Al-{lead}` prefix form.
    prefix_pct: i32,
    /// % chance of the `{lead} al-{second}` construct form.
    construct_pct: i32,
}

impl CultureWords {
    /// This culture's words for `concept`, or empty if it doesn't define it.
    fn words_for(&self, concept: &str) -> Vec<&str> {
        self.concepts
            .get(concept)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// A name source: a word pool plus the weight it carries when picking the lead.
/// A `modifier` theme (colours) may LEAD the name but never be a trailing word,
/// so adjective order stays right ("Black Oak", not "Oak Black").
struct Theme<'a> {
    words: Vec<&'a str>,
    weight: i32,
    modifier: bool,
}

/// The yaml key for a culture's vocabulary.
fn culture_key(culture: Culture) -> &'static str {
    match culture {
        Culture::Desert => "desert",
        Culture::Japanese => "japanese",
        Culture::Medieval => "medieval",
    }
}

/// Generate a name for the settlement occupying `urban` (build-area local
/// coords). Reads the dominant building, civic landmarks (the open-space feature
/// keys in `civic_features` — plaza/park `.key()` strings), land shape, and
/// biome as concepts, renders them in the culture's vocabulary, and composes a
/// name with `rng`. Falls back to a culture-appropriate default if the
/// vocabulary file can't be loaded.
pub fn generate_settlement_name(
    world: &World,
    urban: &HashSet<Point2D>,
    civic_features: &[String],
    culture: Culture,
    rng: &mut RNG,
) -> String {
    let cfg: NamesCfg = match load_yaml("settlement_names.yaml") {
        Ok(c) => c,
        Err(e) => {
            log::warn!("settlement_names.yaml failed to load ({e}); using default name");
            return default_name(culture);
        }
    };
    if urban.is_empty() {
        return default_name(culture);
    }

    let cul = cfg
        .cultures
        .get(culture_key(culture))
        .or_else(|| cfg.cultures.get("medieval"))
        .expect("settlement_names.yaml: no vocabulary for culture or 'medieval' fallback");

    // ── Feature detection → concepts (culture-neutral) ───────────────────
    let centre = centroid(urban);

    // Building: the most common urban structure type, as its concept.
    let building_concept = dominant_building(world, urban, &cfg.buildings);

    // Civic: the concept of every present open-space landmark, deduped + sorted
    // (stable order — `rng.choose` indexes the pool built from these).
    let mut civic_concepts: Vec<&str> = civic_features
        .iter()
        .filter_map(|k| cfg.civic.get(k).map(String::as_str))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    civic_concepts.sort_unstable();

    // Land shape: relief across the footprint + whether the edge touches water.
    let (lo, hi) = urban
        .iter()
        .map(|&c| world.get_height_at(c))
        .fold((i32::MAX, i32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
    let relief = (hi - lo).max(0);
    let landform_concept: Option<&str> = if touches_water(world, urban, cfg.water_share) {
        Some(&cfg.landforms.water)
    } else if relief >= cfg.hilly_relief {
        Some(&cfg.landforms.hilly)
    } else if relief <= cfg.flat_relief {
        Some(&cfg.landforms.flat)
    } else {
        None
    };

    // Biome: always available.
    let biome = world.get_surface_biome_at(centre);
    let biome_concept = biome_concept(&cfg, biome.name());

    // ── Render concepts in the culture's vocabulary into weighted themes ──
    let mut themes: Vec<Theme> = Vec::new();
    if let Some(c) = &building_concept {
        let words = cul.words_for(c);
        if !words.is_empty() {
            themes.push(Theme { words, weight: cfg.weights.building, modifier: false });
        }
    }
    let civic_words: Vec<&str> = civic_concepts.iter().flat_map(|c| cul.words_for(c)).collect();
    if !civic_words.is_empty() {
        themes.push(Theme { words: civic_words, weight: cfg.weights.civic, modifier: false });
    }
    if let Some(c) = landform_concept {
        let words = cul.words_for(c);
        if !words.is_empty() {
            themes.push(Theme { words, weight: cfg.weights.landform, modifier: false });
        }
    }
    // Colour: a modifier theme — always available, leads the name only. Future:
    // bias this pool toward the build palette's dominant material colour.
    if !cul.colors.is_empty() {
        themes.push(Theme {
            words: cul.colors.iter().map(String::as_str).collect(),
            weight: cfg.weights.color,
            modifier: true,
        });
    }
    // Biome theme always contributes: fall back to the culture's `green` words
    // when it doesn't translate this specific biome concept.
    let biome_words = {
        let w = cul.words_for(&biome_concept);
        if w.is_empty() { cul.words_for("green") } else { w }
    };
    if !biome_words.is_empty() {
        themes.push(Theme { words: biome_words, weight: cfg.weights.biome, modifier: false });
    }
    // Last resort: nothing translated — name from the culture's filler words.
    if themes.is_empty() {
        themes.push(Theme {
            words: cul.fillers.iter().map(String::as_str).collect(),
            weight: 1,
            modifier: false,
        });
    }

    // ── Compose ──────────────────────────────────────────────────────────
    let lead_idx = pick_weighted_theme(&themes, rng);
    let lead = (*rng.choose(&themes[lead_idx].words)).to_string();

    let name = if let Some(article) = &cul.article {
        compose_article(article, &themes, lead_idx, &lead, cul, rng)
    } else {
        let two_word_pct = cul.two_word_pct.unwrap_or(cfg.two_word_pct);
        if themes.len() > 1 && rng.percent(two_word_pct) {
            match second_word(&themes, lead_idx, &lead, cul, rng) {
                Some(w) => format!("{lead} {w}"),
                None => attach_suffix(&lead, cul, rng),
            }
        } else {
            attach_suffix(&lead, cul, rng)
        }
    };

    log::info!(
        "settlement name '{}' (building={:?}, civic={:?}, landform={:?}, relief={}, biome='{}'->{})",
        name,
        building_concept,
        civic_concepts,
        landform_concept,
        relief,
        biome.name(),
        biome_concept,
    );
    name
}

/// Append a culture suffix to the lead word (`Mill` + `ford` → `Millford`).
fn attach_suffix(lead: &str, cul: &CultureWords, rng: &mut RNG) -> String {
    if cul.suffixes.is_empty() {
        return lead.to_string();
    }
    let suffix = rng.choose(&cul.suffixes);
    format!("{lead}{suffix}")
}

/// Arabic-style composition: `Al-{lead}` (article prefix), `{lead} al-{second}`
/// (construct/iḍāfa), or a bare `{lead} {second}` compound. The article
/// assimilates to sun letters (`al-Raml` → `ar-Raml`).
fn compose_article(
    article: &ArticleCfg,
    themes: &[Theme],
    lead_idx: usize,
    lead: &str,
    cul: &CultureWords,
    rng: &mut RNG,
) -> String {
    let roll = rng.rand_i32_range(0, 100);
    let prefixed = |word: &str| {
        let art = assimilated_article(word);
        format!("{}-{}", capitalize(&art), word)
    };
    if roll < article.prefix_pct {
        prefixed(lead)
    } else if roll < article.prefix_pct + article.construct_pct {
        // {lead} al-{second}, with the article lower-case in mid-name.
        match second_word(themes, lead_idx, lead, cul, rng) {
            Some(second) => format!("{lead} {}-{second}", assimilated_article(&second)),
            None => prefixed(lead),
        }
    } else {
        // Bare compound: Bir Zeit, Wadi Rum.
        match second_word(themes, lead_idx, lead, cul, rng) {
            Some(second) => format!("{lead} {second}"),
            None => prefixed(lead),
        }
    }
}

/// The definite article for `word`, assimilated to its first (sun) letter:
/// `al` before moon letters, else `a` + the sun consonant (`ar`, `as`, `at`,
/// `an`, `ad`, `az`, and the digraphs `ash`/`ath`/`adh`). Lower-case; the caller
/// capitalises it when it leads the name.
fn assimilated_article(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    // Sun-letter digraphs (sh/th/dh) double in transliteration: ash-Shams.
    if let Some(two) = lower.get(0..2) {
        if matches!(two, "th" | "dh" | "sh") {
            return format!("a{two}");
        }
    }
    match lower.chars().next() {
        // Single sun letters (t d r z s n l; emphatics omitted — not in our pools).
        Some(c @ ('t' | 'd' | 'r' | 'z' | 's' | 'n' | 'l')) => format!("a{c}"),
        _ => "al".to_string(),
    }
}

/// Upper-case the first ASCII letter (`ar` → `Ar`).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// A second, capitalised word for the two-word form: prefer a theme other than
/// the lead's, else a culture filler word — never the lead word itself.
fn second_word(
    themes: &[Theme],
    lead_idx: usize,
    lead: &str,
    cul: &CultureWords,
    rng: &mut RNG,
) -> Option<String> {
    // Skip the lead and any modifier theme (colours): a colour can lead but must
    // never trail, so "Black Oak" never comes out as "Oak Black".
    let others: Vec<usize> =
        (0..themes.len()).filter(|&i| i != lead_idx && !themes[i].modifier).collect();
    if !others.is_empty() {
        let ti = *rng.choose(&others);
        let w: &str = *rng.choose(&themes[ti].words);
        if w != lead {
            return Some(w.to_string());
        }
    }
    let candidates: Vec<&String> = cul.fillers.iter().filter(|w| w.as_str() != lead).collect();
    if candidates.is_empty() {
        None
    } else {
        Some((*rng.choose(&candidates)).clone())
    }
}

/// Weighted pick over themes; returns the chosen index.
fn pick_weighted_theme(themes: &[Theme], rng: &mut RNG) -> usize {
    let total: i32 = themes.iter().map(|t| t.weight.max(0)).sum();
    if total <= 0 {
        return 0;
    }
    let mut r = rng.rand_i32_range(0, total);
    for (i, t) in themes.iter().enumerate() {
        let w = t.weight.max(0);
        if r < w {
            return i;
        }
        r -= w;
    }
    themes.len() - 1
}

/// The most common urban structure/production type, as its mapped concept.
/// Tie-break alphabetically so the choice is deterministic.
fn dominant_building(
    world: &World,
    urban: &HashSet<Point2D>,
    buildings: &HashMap<String, String>,
) -> Option<String> {
    let mut tally: HashMap<&str, usize> = HashMap::new();
    for &c in urban {
        match world.get_claim(c) {
            Some(BuildClaim::Structure(s)) | Some(BuildClaim::ProductionArea(s)) => {
                if let Some(concept) = buildings.get(&s.structure_type.0) {
                    *tally.entry(concept.as_str()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
    tally
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(concept, _)| concept.to_string())
}

/// True when a notable share of footprint-edge cells border water.
fn touches_water(world: &World, urban: &HashSet<Point2D>, share: f64) -> bool {
    let mut edge_on_water = 0usize;
    for &c in urban {
        for d in CARDINALS_2D {
            let n = c + d;
            if !urban.contains(&n) && world.is_in_bounds_2d(n) && world.is_water(n) {
                edge_on_water += 1;
                break;
            }
        }
    }
    edge_on_water as f64 / urban.len() as f64 > share
}

/// The concept for a biome: the first matching rule's concept.
fn biome_concept(cfg: &NamesCfg, biome_name: &str) -> String {
    cfg.biomes
        .iter()
        .find(|r| r.contains.is_empty() || r.contains.iter().any(|s| biome_name.contains(s)))
        .map(|r| r.concept.clone())
        .unwrap_or_else(|| "green".to_string())
}

fn centroid(urban: &HashSet<Point2D>) -> Point2D {
    let n = urban.len().max(1) as i32;
    let sum = urban.iter().fold(Point2D::ZERO, |a, &p| a + p);
    Point2D::new(sum.x / n, sum.y / n)
}

/// Culture-appropriate fallback when the vocabulary file is missing.
fn default_name(culture: Culture) -> String {
    match culture {
        Culture::Desert => "Sandhaven",
        Culture::Japanese => "Yamamura",
        Culture::Medieval => "Blackbarrow",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_assimilates_sun_letters() {
        // Moon letters keep "al-".
        assert_eq!(assimilated_article("Wadi"), "al");
        assert_eq!(assimilated_article("Jebel"), "al");
        assert_eq!(assimilated_article("Bir"), "al");
        assert_eq!(assimilated_article("Qasr"), "al");
        // Single sun letters assimilate: a + the consonant.
        assert_eq!(assimilated_article("Raml"), "ar");
        assert_eq!(assimilated_article("Souk"), "as");
        assert_eq!(assimilated_article("Tell"), "at");
        assert_eq!(assimilated_article("Nakhl"), "an");
        assert_eq!(assimilated_article("Dar"), "ad");
        // Sun-letter digraphs double: ash-/ath-/adh-.
        assert_eq!(assimilated_article("Shams"), "ash");
    }

    #[test]
    fn prefixed_article_is_capitalised() {
        // The full prefixed form the namer emits for the Al-{lead} case.
        let prefixed = |w: &str| format!("{}-{}", capitalize(&assimilated_article(w)), w);
        assert_eq!(prefixed("Wadi"), "Al-Wadi");
        assert_eq!(prefixed("Raml"), "Ar-Raml");
        assert_eq!(prefixed("Souk"), "As-Souk");
        assert_eq!(prefixed("Tell"), "At-Tell");
    }
}
