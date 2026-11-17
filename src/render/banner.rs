//! Banner flags drawn the way the game draws them. `BannerRenderer` renders
//! the flag box with `entity/banner_base` (the bare cloth), then the flag
//! again once per layer: first `entity/banner/base` tinted with the banner's
//! own colour, then each pattern (`entity/banner/<pattern>`, at most 16) tinted
//! with its dye colour, each blended over what is below by its alpha. The
//! pattern sheets are white masks on the flag's part of the 64x64 sheet, so
//! a layer's colour is its texel times the dye's texture colour.
//!
//! Here that stack is baked into one texture per design ([`flag_texture`]),
//! which the banner models in [`super::entity`] put on their flag. A design
//! reaches the renderer through the block string: the editor turns a banner's
//! block-entity data (`{patterns:[{pattern:"minecraft:cross",color:"red"}]}`)
//! into a `patterns=cross:red+border:black` state ([`patterns_state`]), so it
//! survives the scene file. The game has no such state; it never leaves the
//! renderer.

use std::sync::Arc;

use super::assets::{Rgba, Texture};
use crate::util::HashMap;

/// The dye colours' `textureDiffuseColor`, which tints banner layers.
const DYES: [(&str, u32); 16] = [
    ("white", 0xF9FFFE),
    ("orange", 0xF9801D),
    ("magenta", 0xC74EBD),
    ("light_blue", 0x3AB3DA),
    ("yellow", 0xFED83D),
    ("lime", 0x80C71F),
    ("pink", 0xF38BAA),
    ("gray", 0x474F52),
    ("light_gray", 0x9D9D97),
    ("cyan", 0x169C9C),
    ("purple", 0x8932B8),
    ("blue", 0x3C44AA),
    ("brown", 0x835432),
    ("green", 0x5E7C16),
    ("red", 0xB02E26),
    ("black", 0x1D1D21),
];

/// The game draws at most this many pattern layers.
const MAX_LAYERS: usize = 16;

/// Prefix of the texture keys the banner models' flags use; see [`flag_key`].
pub(super) const FLAG_KEY: &str = "tome_banner:";

/// Every dye colour name.
pub(super) fn colours() -> impl Iterator<Item = &'static str> {
    DYES.iter().map(|(c, _)| *c)
}

fn dye(name: &str) -> Option<[u8; 3]> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    DYES.iter().find(|(c, _)| *c == name).map(|(_, v)| [(v >> 16) as u8, (v >> 8) as u8, *v as u8])
}

/// The colour of a banner block id (`red_banner`, `light_blue_wall_banner`).
fn banner_colour(id: &str) -> Option<&str> {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    let c = id.strip_suffix("_wall_banner").or_else(|| id.strip_suffix("_banner"))?;
    dye(c).map(|_| c)
}

/// `(pattern, colour)` layers of a banner's block-entity SNBT, bottom first,
/// namespaces dropped: `{patterns:[{pattern:"minecraft:cross",color:"red"}]}`
/// gives `[("cross", "red")]`. Keys may come in either order and values with
/// or without quotes.
pub fn patterns_from_snbt(data: &str) -> Vec<(String, String)> {
    let Some(start) = data.find("patterns") else { return Vec::new() };
    let rest = &data[start..];
    let Some(open) = rest.find('[') else { return Vec::new() };
    let body = &rest[open + 1..];
    let body = &body[..body.find(']').unwrap_or(body.len())];
    let value = |entry: &str, key: &str| -> Option<String> {
        let i = entry.find(&format!("{key}:"))? + key.len() + 1;
        let v = entry[i..].trim_start();
        let v = v.split([',', '}']).next()?.trim().trim_matches(['"', '\'']);
        Some(v.strip_prefix("minecraft:").unwrap_or(v).to_string())
    };
    body.split('}')
        .filter_map(|entry| Some((value(entry, "pattern")?, value(entry, "color")?)))
        .filter(|(p, c)| !p.is_empty() && !c.is_empty())
        .collect()
}

/// The `patterns` state value for a banner's block-entity SNBT, or `None`
/// when it has no layers: `cross:red+border:black`.
pub fn patterns_state(data: &str) -> Option<String> {
    let layers = patterns_from_snbt(data);
    (!layers.is_empty()).then(|| layers.iter().map(|(p, c)| format!("{p}:{c}")).collect::<Vec<_>>().join("+"))
}

/// The flag texture key for a banner block `id` with state `props`, or `None`
/// when `id` is not a banner: `tome_banner:red` or
/// `tome_banner:red/cross:white+border:black`.
pub(super) fn flag_key(id: &str, props: &std::collections::BTreeMap<String, String>) -> Option<String> {
    let colour = banner_colour(id)?;
    Some(match props.get("patterns").filter(|p| !p.is_empty()) {
        Some(p) => format!("{FLAG_KEY}{colour}/{p}"),
        None => format!("{FLAG_KEY}{colour}"),
    })
}

/// Bake the flag texture named by a [`flag_key`] from the jar's banner
/// sheets: the bare cloth, the base layer in the banner's colour, then each
/// pattern in its dye. Unknown patterns or dyes are skipped, as the game
/// skips patterns it has no texture for.
pub(super) fn flag_texture(textures: &HashMap<String, Arc<Texture>>, key: &str) -> Option<Texture> {
    let design = key.strip_prefix(FLAG_KEY)?;
    let (colour, layers) = design.split_once('/').unwrap_or((design, ""));
    let cloth = textures.get("entity/banner_base")?;
    let mut out: Vec<Rgba> = cloth.rgba.clone();
    let size = cloth.size;
    let layers = std::iter::once(("base", colour))
        .chain(layers.split('+').filter_map(|l| l.split_once(':')).take(MAX_LAYERS));
    for (pattern, colour) in layers {
        let (Some(tex), Some(tint)) = (textures.get(&format!("entity/banner/{pattern}")), dye(colour)) else {
            log::debug!("banner: no texture or dye for layer {pattern}:{colour}");
            continue;
        };
        if tex.size != size {
            log::debug!("banner: layer {pattern} is {}px, the cloth {size}px; skipped", tex.size);
            continue;
        }
        for (dst, src) in out.iter_mut().zip(&tex.rgba) {
            blend(dst, *src, tint);
        }
    }
    Some(Texture::from_rgba(size, out))
}

/// `src` tinted by `tint`, blended over `dst` by its alpha.
fn blend(dst: &mut Rgba, src: Rgba, tint: [u8; 3]) {
    let a = src[3] as u32;
    if a == 0 {
        return;
    }
    for k in 0..3 {
        let s = src[k] as u32 * tint[k] as u32 / 255;
        dst[k] = ((s * a + dst[k] as u32 * (255 - a)) / 255) as u8;
    }
    dst[3] = (a + dst[3] as u32 * (255 - a) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_read_from_block_entity_snbt() {
        let data = r#"{patterns:[{pattern:"minecraft:cross",color:"red"},{color:"light_blue",pattern:"minecraft:border"}]}"#;
        assert_eq!(
            patterns_from_snbt(data),
            vec![("cross".to_string(), "red".to_string()), ("border".to_string(), "light_blue".to_string())]
        );
        assert_eq!(patterns_state(data).as_deref(), Some("cross:red+border:light_blue"));
        assert_eq!(patterns_state("{patterns:[]}"), None);
        assert_eq!(patterns_state("{CustomName:'x'}"), None);
    }

    #[test]
    fn flag_keys_name_colour_and_design() {
        let mut props = std::collections::BTreeMap::new();
        assert_eq!(flag_key("minecraft:light_blue_wall_banner", &props).as_deref(), Some("tome_banner:light_blue"));
        props.insert("patterns".to_string(), "cross:red".to_string());
        assert_eq!(flag_key("red_banner", &props).as_deref(), Some("tome_banner:red/cross:red"));
        assert_eq!(flag_key("oak_sign", &props), None);
    }

    #[test]
    fn layers_blend_by_alpha_through_the_dye() {
        let mut px = [255, 255, 255, 255];
        blend(&mut px, [255, 255, 255, 0], [0, 0, 0]);
        assert_eq!(px, [255, 255, 255, 255], "a clear texel leaves the cloth");
        blend(&mut px, [255, 255, 255, 255], [176, 46, 38]);
        assert_eq!(px, [176, 46, 38, 255], "an opaque white texel takes the dye");
    }
}
