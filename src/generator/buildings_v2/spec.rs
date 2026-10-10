//! House spec: the high-level decisions of one buildings_v2 house as a YAML
//! file (see `docs/plans/house_spec.md`).
//!
//! Every field is optional. A spec written by the generator
//! ([`HouseSpec::resolved`]) has every decision filled in with what the house
//! actually used; a hand-written one may hold anything from just a `seed` to a
//! fully pinned house. Building from a spec (`--spec`) is phase 3; today the
//! generator only writes them: `<house>.spec.yaml` beside each `--houses`
//! house, and `output/runs/<seed>/houses/house_<n>.spec.yaml` per town house.
//!
//! ```yaml
//! seed: -1840192364981520018
//! culture: medieval
//! size: manor
//! palette: { style: whitewash_wattle, base: medieval_spruce, wood: oak, stone: cobblestone, roof: brick_roof }
//! colour: red
//! roof: { gable: stairs }
//! footprint:            # plot-relative, core first (what --rects takes)
//!   - { x: 0, z: 0, w: 12, h: 8, floors: 3 }
//!   - { x: 12, z: 2, w: 6, h: 5, floors: 2 }
//! jetty: true
//! engawa: false
//! knee_wall: false
//! tall_top_floor: false
//! timber: { decorated: { spacing: 3 } }
//! attic: false
//! cellar: { theme: wine }
//! rooms:                # per rect, per floor, bottom up; then its attic room
//!   - [great_room, bedroom, study]
//!   - [kitchen, bedroom]
//! site: { origin: [120, 344], front_door: south, base_y: 71 }
//! ```

use serde::{Deserialize, Serialize};

use crate::generator::materials::{Palette, PaletteId};
use crate::geometry::{Cardinal, Point2D, Point3D};
use crate::minecraft::Color;

use super::footprint::SizeClass;
use super::roof::RoofStyle;
use super::style::PalettePicks;
use super::{BuildingContext, Culture, HouseOutput, RoomType, TimberPattern};

/// One house's decisions. See the module doc for the YAML shape.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HouseSpec {
    /// The house seed (`HouseSeed`): every open decision, and every computed
    /// stage (walls, stairs, windows, furniture), draws from its named streams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub culture: Option<Culture>,
    /// `cottage`, `house`, `hall` or `manor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<PaletteSpec>,
    /// Primary (dye) colour: tints banners, beds, carpets and other dyed accents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<Color>,
    /// `flat`, `irimoya`, `{ gable: slab|stairs|double }`, `{ hipped: slab|stairs }`.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_yaml::with::singleton_map")]
    pub roof: Option<RoofStyle>,
    /// Rects relative to the plot's minimum corner, core first: exactly what
    /// `--rects` takes, plus each rect's floor count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Vec<RectSpec>>,
    /// Requested upper-floor overhang. Frame generation drops it when the
    /// shape, floor count or plot doesn't allow it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jetty: Option<bool>,
    /// Requested raised veranda (Japanese only; dropped when the core is too
    /// small once inset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engawa: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knee_wall: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tall_top_floor: Option<bool>,
    /// `plain`, or `{ studded|braced|decorated: { spacing: N } }`.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "serde_yaml::with::singleton_map")]
    pub timber: Option<TimberPattern>,
    /// Whether the house has an attic. Derived, not a decision of its own:
    /// today exactly the `{ gable: double }` roofs have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cellar: Option<CellarSpec>,
    /// Room types per footprint rect (same order), bottom floor first. A
    /// rect's list holds one room per floor (`floors` entries); in a house
    /// with an attic every rect's list ends with one more entry, its attic
    /// room. The layout inside (walls, doors, furniture) stays computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rooms: Option<Vec<Vec<RoomType>>>,
    /// Where the house stood. Read back for reference only: a build from the
    /// spec takes these from its plot and road and ignores them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SiteSpec>,
}

/// The palette: a style name and/or the individual palette files merged onto
/// its base (what `BuildingStyle::roll_palette_picks` chose). Every field is
/// optional; a pinned material wins over what the style would roll.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaletteSpec {
    /// Style from the culture's catalog, snake_case (`whitewash_wattle`).
    /// Absent when the house used the culture's default palette (`--houses`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Base palette file (`data/palettes/`): walls, floors, flowers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<PaletteId>,
    /// Wood palette merged on top (frame, planks, doors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wood: Option<PaletteId>,
    /// Stone palette merged on top (foundation, cellar, base course).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stone: Option<PaletteId>,
    /// Roof palette merged on top (`data/palettes/roofs/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roof: Option<PaletteId>,
    /// Accent palette (the flat-roof parapet band).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<PaletteId>,
}

/// One footprint rect, plot-relative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectSpec {
    pub x: i32,
    pub z: i32,
    /// Size along x.
    pub w: i32,
    /// Size along z.
    pub h: i32,
    /// Storeys over this rect (the core has the most).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floors: Option<u32>,
}

/// `false` (no cellar), `true` (a cellar, theme open) or `{ theme: wine }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellarSpec {
    Built(bool),
    Themed {
        /// `storage`, `wine`, `larder` or `workshop` (furniture lists
        /// `storage` / `cellar_<theme>` in `data/rooms.yaml`).
        theme: String,
    },
}

/// Where a house stood (read-only; see [`HouseSpec::site`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteSpec {
    /// Absolute world `[x, z]` of the corner the footprint is relative to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<[i32; 2]>,
    /// The side the front door opens out of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_door: Option<Cardinal>,
    /// Absolute y of the ground floor (the front door's sill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_y: Option<i32>,
}

impl HouseSpec {
    /// The fully resolved spec of a built house: every decision as the
    /// pipeline used it. `origin` is the build-area-local corner the footprint
    /// is written relative to (the plot's minimum corner for `--houses`, so
    /// the rects go straight back into `--rects`); `world_origin` is the build
    /// area's origin, for the absolute `site` fields. Reads only, draws nothing.
    pub fn resolved(
        bctx: &BuildingContext,
        house: &HouseOutput,
        palette: &Palette,
        picks: &PalettePicks,
        origin: Point2D,
        world_origin: Point3D,
    ) -> Self {
        let floor_counts = house.frame.floor_counts();
        let rects = house.footprint.rects();
        let footprint = rects
            .iter()
            .enumerate()
            .map(|(i, r)| RectSpec {
                x: r.min().x - origin.x,
                z: r.min().y - origin.y,
                w: r.size.x,
                h: r.size.y,
                floors: floor_counts.get(i).copied(),
            })
            .collect();
        let rooms = (0..rects.len())
            .map(|i| {
                let mut on_rect: Vec<_> = house.room_plan.rooms.iter().filter(|r| r.rect_index == i).collect();
                on_rect.sort_by_key(|r| r.floor);
                on_rect.into_iter().map(|r| r.room_type).collect()
            })
            .collect();
        let cellar = match house.cellar_theme {
            Some(theme) => CellarSpec::Themed { theme: theme.trim_start_matches("cellar_").to_string() },
            None => CellarSpec::Built(false),
        };
        HouseSpec {
            seed: Some(house.seed.0),
            culture: Some(bctx.culture),
            size: Some(house.size_class),
            palette: Some(PaletteSpec {
                style: picks.style.as_deref().map(style_key),
                base: Some(picks.base.clone()),
                wood: picks.wood.clone(),
                stone: picks.stone.clone(),
                roof: picks.roof.clone(),
                accent: picks.accent.clone(),
            }),
            colour: palette.primary_color,
            roof: Some(house.roof_style),
            footprint: Some(footprint),
            jetty: Some(bctx.jetty),
            engawa: Some(bctx.engawa),
            knee_wall: Some(bctx.knee_wall),
            tall_top_floor: Some(bctx.tall_top_floor),
            timber: Some(house.timber_pattern),
            attic: Some(house.has_attic),
            cellar: Some(cellar),
            rooms: Some(rooms),
            site: Some(SiteSpec {
                origin: Some([origin.x + world_origin.x, origin.y + world_origin.z]),
                front_door: house.front_door,
                base_y: Some(house.base_y + world_origin.y),
            }),
        }
    }

    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    pub fn from_yaml(text: &str) -> anyhow::Result<Self> {
        Ok(serde_yaml::from_str(text)?)
    }

    /// Write the spec as YAML under a short comment saying how to read it.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
        let header = "# House spec (docs/plans/house_spec.md). footprint: rects relative to the plot, core first.\n\
                      # rooms: per footprint rect, bottom floor first, then its attic room if any.\n\
                      # site: where the house stood, read back only.\n";
        std::fs::write(path, format!("{header}{}", self.to_yaml()?))?;
        Ok(())
    }
}

/// A catalog style name as a spec key: `"Cherry & Ink"` -> `cherry_ink`.
pub fn style_key(name: &str) -> String {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::World;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::roof::hipped::HippedPitch;
    use crate::generator::buildings_v2::showcase::{house_context, house_footprint};
    use crate::generator::buildings_v2::{build_house, BuildCtx, HouseSeed};
    use crate::generator::data::LoadedData;
    use crate::geometry::{Rect2D, Rect3D};
    use crate::noise::RNG;

    const CELL: i32 = 48;
    const MARGIN: i32 = 6;
    const GROUND_Y: i32 = 64;

    fn plot_bounds() -> Rect2D {
        Rect2D::from_points(Point2D::new(MARGIN, MARGIN), Point2D::new(CELL - 1 - MARGIN, CELL - 1 - MARGIN))
    }

    /// Build one house the way `--houses` does and return it with its spec.
    async fn build_with_spec(
        data: &LoadedData,
        culture: Culture,
        size: SizeClass,
        roof: RoofStyle,
        seed: HouseSeed,
    ) -> Option<(HouseOutput, HouseSpec)> {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(CELL - 1, 159, CELL - 1));
        let world = World::synthetic(build_area, GROUND_Y);
        let world_origin = world.build_area.origin;
        let mut editor = world.get_offline_editor();
        let palette = data.palettes.get(&culture.palette_id()).expect("palette").clone();
        let footprint = house_footprint(None, plot_bounds(), size, culture, seed).expect("ok")?;
        let bctx = house_context(culture, size, roof, seed);
        let mut rng = RNG::new(seed.0);
        let mut ctx = BuildCtx::new(&mut editor, data, &palette, &mut rng);
        let house = build_house(&mut ctx, footprint, &bctx, plot_bounds()).await.expect("invariants hold");
        let picks = PalettePicks::base(culture.palette_id());
        let spec = HouseSpec::resolved(&bctx, &house, &palette, &picks, plot_bounds().min(), world_origin);
        Some((house, spec))
    }

    #[test]
    fn style_keys_are_snake_case() {
        assert_eq!(style_key("Whitewash Wattle"), "whitewash_wattle");
        assert_eq!(style_key("Cherry & Ink"), "cherry_ink");
    }

    #[test]
    fn enums_read_as_words() {
        let spec = HouseSpec {
            roof: Some(RoofStyle::Gable(GablePitch::Stairs)),
            timber: Some(TimberPattern::Decorated { spacing: 3 }),
            cellar: Some(CellarSpec::Themed { theme: "wine".into() }),
            rooms: Some(vec![vec![RoomType::GreatRoom, RoomType::MultiBedroom]]),
            ..Default::default()
        };
        let yaml = spec.to_yaml().unwrap();
        assert!(yaml.contains("gable: stairs"), "{yaml}");
        assert!(yaml.contains("decorated:"), "{yaml}");
        assert!(yaml.contains("spacing: 3"), "{yaml}");
        assert!(yaml.contains("theme: wine"), "{yaml}");
        assert!(yaml.contains("great_room") && yaml.contains("multi_bedroom"), "{yaml}");
        assert!(!yaml.contains("seed"), "unset fields are left out: {yaml}");
        let flat = HouseSpec { roof: Some(RoofStyle::Flat), cellar: Some(CellarSpec::Built(false)), ..Default::default() };
        let yaml = flat.to_yaml().unwrap();
        assert!(yaml.contains("roof: flat") && yaml.contains("cellar: false"), "{yaml}");
        assert_eq!(HouseSpec::from_yaml(&yaml).unwrap(), flat);
    }

    #[test]
    fn partial_specs_parse() {
        let spec = HouseSpec::from_yaml("seed: 5\n").unwrap();
        assert_eq!(spec, HouseSpec { seed: Some(5), ..Default::default() });

        let spec = HouseSpec::from_yaml("culture: japanese\nroof: { hipped: slab }\n").unwrap();
        assert_eq!(
            spec,
            HouseSpec {
                culture: Some(Culture::Japanese),
                roof: Some(RoofStyle::Hipped(HippedPitch::Slab)),
                ..Default::default()
            }
        );

        let spec = HouseSpec::from_yaml("roof: irimoya\ncellar: true\nfootprint: [{ x: 0, z: 0, w: 9, h: 7 }]\n").unwrap();
        assert_eq!(spec.roof, Some(RoofStyle::Irimoya));
        assert_eq!(spec.cellar, Some(CellarSpec::Built(true)));
        assert_eq!(spec.footprint, Some(vec![RectSpec { x: 0, z: 0, w: 9, h: 7, floors: None }]));
        assert_eq!(spec.size, None);
    }

    /// A built house's resolved spec survives YAML both ways, and its
    /// footprint, floors and room types are the house's own.
    #[tokio::test]
    async fn emitted_spec_matches_the_house() {
        let data = LoadedData::load().expect("data");
        let mut checked = 0;
        for (k, (culture, size)) in [
            (Culture::Medieval, SizeClass::Manor),
            (Culture::Medieval, SizeClass::House),
            (Culture::Japanese, SizeClass::Hall),
            (Culture::Desert, SizeClass::Hall),
            (Culture::Medieval, SizeClass::Cottage),
        ]
        .into_iter()
        .enumerate()
        {
            for s in 0..2 {
                let roofs = culture.roof_styles();
                let roof = roofs[(k + s) % roofs.len()];
                let seed = HouseSeed(3000 + 10 * k as i64 + s as i64);
                let Some((house, spec)) = build_with_spec(&data, culture, size, roof, seed).await else { continue };

                let yaml = spec.to_yaml().unwrap();
                assert_eq!(HouseSpec::from_yaml(&yaml).unwrap(), spec, "round trip:\n{yaml}");

                let shift = plot_bounds().min();
                let rects: Vec<Rect2D> = spec
                    .footprint
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|r| Rect2D::new(Point2D::new(r.x, r.z) + shift, Point2D::new(r.w, r.h)))
                    .collect();
                let corners = |rs: &[Rect2D]| rs.iter().map(|r| (r.min(), r.size)).collect::<Vec<_>>();
                assert_eq!(corners(&rects), corners(house.footprint.rects()), "{culture:?} {size:?}: footprint\n{yaml}");
                let floors: Vec<u32> = spec.footprint.as_ref().unwrap().iter().map(|r| r.floors.unwrap()).collect();
                assert_eq!(floors, house.frame.floor_counts().to_vec(), "{culture:?} {size:?}: floors\n{yaml}");

                let rooms = spec.rooms.as_ref().unwrap();
                assert_eq!(rooms.len(), rects.len());
                for (i, list) in rooms.iter().enumerate() {
                    let attic = usize::from(house.has_attic);
                    assert_eq!(list.len(), floors[i] as usize + attic, "rect {i}: one room per floor (+ attic)\n{yaml}");
                    for (f, ty) in list.iter().enumerate() {
                        let room = house
                            .room_plan
                            .rooms
                            .iter()
                            .find(|r| r.rect_index == i && r.floor == f as u32)
                            .expect("room for every rect and floor");
                        assert_eq!(*ty, room.room_type, "rect {i} floor {f}\n{yaml}");
                    }
                }
                assert_eq!(spec.attic, Some(house.has_attic));
                assert_eq!(matches!(spec.cellar, Some(CellarSpec::Themed { .. })), house.has_cellar);
                assert_eq!(spec.roof, Some(house.roof_style));
                assert_eq!(spec.timber, Some(house.timber_pattern));
                assert_eq!(spec.seed, Some(seed.0));
                assert!(spec.site.as_ref().unwrap().front_door.is_some(), "front door recorded");
                checked += 1;
            }
        }
        assert!(checked >= 6, "only {checked} houses fit their plots");
    }
}
