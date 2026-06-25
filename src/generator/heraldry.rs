//! Heraldic banner designs for manor families. Every banner a manor owns — the
//! pair flanking its front door and every banner inside — flies one shared
//! design: the banner block's own colour is the family's primary colour (the
//! heraldic "field"), and each pattern layer is stamped in a distinct secondary
//! colour (the "charge"). The design is rendered both as block-entity pattern
//! SNBT (stamped on every one of the family's banners) and as an English blazon
//! for the chronicle, e.g. "a red cross on a black background". Designs live in
//! `data/banners.yaml`.

use serde_derive::Deserialize;

use crate::data::load_yaml;
use crate::minecraft::Color;
use crate::noise::RNG;

#[derive(Debug, Clone, Deserialize)]
struct BannerDesign {
    /// Identifier — documents the entry; unused at runtime.
    #[allow(dead_code)]
    name: String,
    /// Pattern ids (minecraft `banner_pattern` registry names), stacked
    /// bottom-to-top, each stamped in the secondary colour over the base.
    layers: Vec<String>,
    /// The charge as an English noun phrase, slotted into
    /// "a {secondary} {blazon} on a {primary} background".
    blazon: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BannerCfg {
    designs: Vec<BannerDesign>,
}

/// A family's chosen banner: the block-entity pattern SNBT to stamp on every one
/// of its banners, plus the English blazon describing it.
pub struct FamilyBanner {
    /// `{patterns:[...]}` block-entity data; pair with a `<primary>_*banner`
    /// block so the field reads as the primary colour.
    pub data: String,
    /// e.g. "a red cross on a black background".
    pub blazon: String,
}

/// Pick a heraldic design for one family and render it for a banner whose base
/// (field) colour is `primary`, with the charge in `secondary`. `secondary` must
/// differ from `primary` for the charge to read against the field. Returns
/// `None` if the design list is empty or fails to load — the family's banners
/// then stay solid `primary`.
pub fn pick_family_banner(primary: Color, secondary: Color, rng: &mut RNG) -> Option<FamilyBanner> {
    let cfg: BannerCfg = load_yaml("banners.yaml")
        .map_err(|e| log::warn!("banners.yaml failed to load ({e}); plain manor banners"))
        .ok()?;
    if cfg.designs.is_empty() {
        return None;
    }
    let design = rng.choose(&cfg.designs).clone();
    let sec: String = secondary.into();
    let patterns: Vec<String> = design
        .layers
        .iter()
        .map(|p| format!("{{pattern:\"minecraft:{p}\",color:\"{sec}\"}}"))
        .collect();
    Some(FamilyBanner {
        data: format!("{{patterns:[{}]}}", patterns.join(",")),
        blazon: format!(
            "a {} {} on a {} background",
            color_word(secondary),
            design.blazon,
            color_word(primary),
        ),
    })
}

/// A colour as an English word for prose ("light_blue" -> "light blue").
fn color_word(c: Color) -> String {
    let s: String = c.into();
    s.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::{Seed, RNG};

    #[test]
    fn banners_yaml_loads_and_is_nonempty() {
        let cfg: BannerCfg = load_yaml("banners.yaml").expect("banners.yaml parses");
        assert!(!cfg.designs.is_empty(), "no banner designs defined");
        for d in &cfg.designs {
            assert!(!d.layers.is_empty(), "design {} has no layers", d.name);
            assert!(!d.blazon.is_empty(), "design {} has no blazon", d.name);
        }
    }

    #[test]
    fn pick_renders_pattern_snbt_and_blazon() {
        let mut rng = RNG::new(Seed(7));
        let b = pick_family_banner(Color::Black, Color::Red, &mut rng)
            .expect("a design is picked");
        // SNBT carries the charge colour and is a patterns list.
        assert!(b.data.starts_with("{patterns:["), "data: {}", b.data);
        assert!(b.data.contains("color:\"red\""), "data: {}", b.data);
        // Blazon names both colours as the field/charge of the sentence.
        assert!(b.blazon.starts_with("a red "), "blazon: {}", b.blazon);
        assert!(b.blazon.ends_with("on a black background"), "blazon: {}", b.blazon);
    }

    #[test]
    fn multiword_colour_reads_with_a_space() {
        let mut rng = RNG::new(Seed(1));
        let b = pick_family_banner(Color::LightBlue, Color::White, &mut rng).unwrap();
        assert!(b.blazon.ends_with("on a light blue background"), "blazon: {}", b.blazon);
    }
}
