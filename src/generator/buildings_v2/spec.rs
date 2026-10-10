//! House spec: the high-level decisions of one buildings_v2 house as a YAML
//! file (see `docs/plans/house_spec.md`).
//!
//! Every field is optional. A spec written by the generator
//! ([`HouseSpec::resolved`]) has every decision filled in with what the house
//! actually used; a hand-written one may hold anything from just a `seed` to a
//! fully pinned house. The generator writes one per house built
//! (`<house>.spec.yaml` beside each `--houses` house,
//! `output/runs/<seed>/houses/house_<n>.spec.yaml` per town house), and
//! `--houses --spec <file>` builds from one ([`resolve`], then
//! [`ResolvedHouse::build`]).
//!
//! **Resolution.** A pinned field is used as written; an open one is drawn
//! from its own named stream of the house seed (see `seed`), with the same
//! draw an unpinned build makes, so an emitted spec rebuilds the same house
//! and a field left open draws what the generator would have drawn:
//!
//! | field | open: drawn from |
//! |---|---|
//! | `culture` | not drawn: the `--culture` flag (default medieval) |
//! | `size` | `size`: uniform over the four classes (a town draws from its lot tier's pool) |
//! | `roof` | `roof`: the culture's size-weighted bag (`roof_styles_for`), as a town does; narrowed to roofs matching a pinned `attic` |
//! | `palette` | `palette`: a uniform style from the culture's catalog, then its pools (a town picks from its district scheme). `style: default` is the culture's plain palette with no pools (what `--houses` builds on); pinned materials replace the style's picks |
//! | `colour` | `colour`: uniform over the culture's colour pool |
//! | `footprint` | `footprint`, inside the plot; `floors`: the frame's `floors` stream |
//! | `engawa`, `jetty` | `engawa`, `jetty`: the culture's chances |
//! | `knee_wall`, `tall_top_floor` | not drawn: off |
//! | `timber` | `timber` (medieval; plain elsewhere) |
//! | `cellar` | `cellar`: the size class's chance and a uniform theme |
//! | `rooms` | `rooms` / `attic`: the size class's room rules |
//! | `attic` | derived from `roof` (exactly `{ gable: double }` has one); a pin must agree |
//!
//! Every pin skips or overrides only its own draw: a stream that draws
//! several things (the frame's floors, the cellar's roll and stair) still
//! makes every draw and replaces the pinned one, so pinning a field to the
//! value it drew changes nothing. `site` is ignored on input except
//! `site.plot`, which sizes the plot: the front door goes on the side nearest
//! the plot's edge, so the same plot puts it back where it was. The ground
//! floor's height comes from the plot's terrain.
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

use std::path::Path;

use anyhow::{anyhow, bail, Context};
use serde::{Deserialize, Serialize};

use crate::editor::Editor;
use crate::generator::data::LoadedData;
use crate::generator::materials::{Palette, PaletteId};
use crate::geometry::{Cardinal, Point2D, Point3D, Rect2D};
use crate::minecraft::Color;
use crate::noise::RNG;

use super::cellar::CELLAR_THEMES;
use super::footprint::{Footprint, SizeClass};
use super::roof::gable::GablePitch;
use super::roof::RoofStyle;
use super::showcase::{house_context, house_footprint};
use super::style::PalettePicks;
use super::{build_house, BuildCtx, BuildingContext, Culture, HouseOutput, HouseSeed, RoomType, TimberPattern};

/// Every size class, in the order an open `size` draws from.
pub const SIZES: [SizeClass; 4] = [SizeClass::Cottage, SizeClass::House, SizeClass::Hall, SizeClass::Manor];

/// The palette style name meaning "the culture's own palette, no style".
pub const DEFAULT_STYLE: &str = "default";

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
    /// Style from the culture's catalog, snake_case (`whitewash_wattle`), or
    /// `default`: the culture's own palette with no style pools (what plain
    /// `--houses` builds on). Open: drawn from the catalog.
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
    /// Size `[x, z]` of the plot the footprint sat in (`--houses` only). The
    /// one site field a build reads: it sizes the plot (grown to fit the
    /// footprint), so the front door lands on the same side again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plot: Option<[i32; 2]>,
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
                style: Some(picks.style.as_deref().map(style_key).unwrap_or_else(|| DEFAULT_STYLE.to_string())),
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
                plot: None,
            }),
        }
    }

    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    pub fn from_yaml(text: &str) -> anyhow::Result<Self> {
        Ok(serde_yaml::from_str(text)?)
    }

    /// Read a spec file; parse errors name the file.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("reading spec {}", path.display()))?;
        Self::from_yaml(&text).with_context(|| format!("parsing spec {}", path.display()))
    }

    /// The plot size this spec asks for: `site.plot` grown to its pinned
    /// footprint's extent (rects are plot-relative). `None` when neither is given.
    pub fn plot_extent(&self) -> Option<Point2D> {
        let mut ext: Option<Point2D> = self.site.as_ref().and_then(|s| s.plot).map(|[x, z]| Point2D::new(x, z));
        for r in self.footprint.iter().flatten() {
            let e = Point2D::new(r.x + r.w, r.z + r.h);
            ext = Some(ext.map_or(e, |p| Point2D::new(p.x.max(e.x), p.y.max(e.y))));
        }
        ext
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

/// Whether a house with this roof has an attic (the pipeline's rule).
pub fn roof_has_attic(roof: RoofStyle) -> bool {
    matches!(roof, RoofStyle::Gable(GablePitch::Double))
}

/// One house's decisions resolved against its seed and plot: everything
/// `build_house` needs. Made by [`resolve`].
pub struct ResolvedHouse {
    pub seed: HouseSeed,
    pub palette: Palette,
    pub picks: PalettePicks,
    /// Build-area local.
    pub footprint: Footprint,
    pub plot_bounds: Rect2D,
    /// Culture, size, roof, engawa, jetty, the upper-storey variants and the
    /// pins the build itself honours (floors, timber, cellar, room types).
    pub bctx: BuildingContext,
}

impl ResolvedHouse {
    /// Build the house into `editor` and return it with its resolved spec
    /// (footprint relative to the plot, `site.plot` recorded). `Err` carries
    /// `build_house`'s message (a failed invariant).
    pub async fn build(&self, editor: &mut Editor, data: &LoadedData) -> Result<(HouseOutput, HouseSpec), String> {
        let world_origin = editor.world().build_area.origin;
        // `bctx.seed` is set, so `build_house` never draws from this one.
        let mut caller_rng = RNG::new(self.seed.0);
        let mut ctx = BuildCtx::new(editor, data, &self.palette, &mut caller_rng);
        let house = build_house(&mut ctx, self.footprint.clone(), &self.bctx, self.plot_bounds).await?;
        let origin = self.plot_bounds.min();
        let mut spec = HouseSpec::resolved(&self.bctx, &house, &self.palette, &self.picks, origin, world_origin);
        if let Some(site) = spec.site.as_mut() {
            site.plot = Some([self.plot_bounds.size.x, self.plot_bounds.size.y]);
        }
        Ok((house, spec))
    }
}

/// Resolve `spec` against the house seed `seed` and `plot_bounds`. The caller
/// picks the seed: the spec's own `seed` for a single house, a derived one in
/// a sweep (`showcase::spec_house_seed`); `spec.seed` itself is not read here.
/// `plot_bounds` (build-area local; pinned rects are relative to
/// its minimum corner). Pinned fields are taken as written, open ones drawn
/// as the module doc says. `Ok(None)` when no generated footprint fits the
/// plot. Errors name the offending field.
pub fn resolve(spec: &HouseSpec, seed: HouseSeed, plot_bounds: Rect2D, data: &LoadedData) -> anyhow::Result<Option<ResolvedHouse>> {
    let culture = spec.culture.unwrap_or(Culture::Medieval);
    let size = match spec.size {
        Some(s) => s,
        None => *seed.stream("size").choose(&SIZES),
    };

    // Roof, and the attic that follows from it.
    let roof = match spec.roof {
        Some(r) => r,
        None => {
            let bag = culture.roof_styles_for(size);
            let mut rng = seed.stream("roof");
            let drawn = bag[rng.rand_i32_range(0, bag.len() as i32) as usize];
            match spec.attic {
                // A pinned attic the drawn roof disagrees with: draw again,
                // on the same stream, among the roofs that agree.
                Some(a) if roof_has_attic(drawn) != a => {
                    let fits: Vec<RoofStyle> = bag.into_iter().filter(|&r| roof_has_attic(r) == a).collect();
                    if fits.is_empty() {
                        bail!(
                            "attic: {a}, but no {} roof {} an attic; pin a roof (only {{ gable: double }} has one)",
                            label(culture), if a { "has" } else { "lacks" },
                        );
                    }
                    fits[rng.rand_i32_range(0, fits.len() as i32) as usize]
                }
                _ => drawn,
            }
        }
    };
    let has_attic = roof_has_attic(roof);
    if let Some(a) = spec.attic {
        if a != has_attic {
            bail!(
                "attic: {a} contradicts roof: {} ({}); the attic follows the roof, only {{ gable: double }} has one",
                roof_name(roof), if has_attic { "it has an attic" } else { "it has none" },
            );
        }
    }

    check_footprint(spec, plot_bounds)?;
    check_rooms(spec, has_attic)?;
    let (cellar, cellar_theme) = resolve_cellar(spec.cellar.as_ref())?;
    let (mut palette, picks) = resolve_palette(spec.palette.as_ref(), culture, seed, data)?;
    palette.primary_color = Some(match spec.colour {
        Some(c) => c,
        None => *seed.stream("colour").choose(&culture.color_pool()),
    });

    let rects: Option<Vec<Rect2D>> = spec
        .footprint
        .as_ref()
        .map(|fp| fp.iter().map(|r| Rect2D::new(Point2D::new(r.x, r.z), Point2D::new(r.w, r.h))).collect());
    let Some(footprint) = house_footprint(rects.as_deref(), plot_bounds, size, culture, seed)? else {
        return Ok(None);
    };

    let mut bctx = house_context(culture, size, roof, seed);
    if let Some(e) = spec.engawa {
        bctx.engawa = e;
    }
    bctx.jetty = spec.jetty.unwrap_or_else(|| {
        let (jn, jd) = culture.jetty_chance();
        jd > 0 && seed.stream("jetty").rand_i32_range(0, jd as i32) < jn as i32
    });
    bctx.knee_wall = spec.knee_wall.unwrap_or(false);
    bctx.tall_top_floor = spec.tall_top_floor.unwrap_or(false);
    bctx.timber_pattern = spec.timber;
    bctx.floors = spec
        .footprint
        .as_ref()
        .filter(|fp| fp.iter().any(|r| r.floors.is_some()))
        .map(|fp| fp.iter().map(|r| r.floors).collect());
    bctx.cellar = cellar;
    bctx.cellar_theme = cellar_theme;
    bctx.room_types = spec.rooms.clone();

    Ok(Some(ResolvedHouse { seed, palette, picks, footprint, plot_bounds, bctx }))
}

/// `footprint`: no empty list or rect, nothing outside the plot, no two rects
/// overlapping, every rect touching the rest along an edge, sane floors.
fn check_footprint(spec: &HouseSpec, plot_bounds: Rect2D) -> anyhow::Result<()> {
    let Some(fp) = &spec.footprint else { return Ok(()) };
    if fp.is_empty() {
        bail!("footprint: empty; give at least the core rect, or leave footprint out to draw one");
    }
    let plot = plot_bounds.size;
    for (i, r) in fp.iter().enumerate() {
        if r.w < 1 || r.h < 1 {
            bail!("footprint[{i}]: size {}x{} is empty (w and h must be at least 1)", r.w, r.h);
        }
        if r.x < 0 || r.z < 0 || r.x + r.w > plot.x || r.z + r.h > plot.y {
            bail!(
                "footprint[{i}]: x {}..{}, z {}..{} is outside the {}x{} plot (rects are relative to the plot's corner, x and z from 0)",
                r.x, r.x + r.w - 1, r.z, r.z + r.h - 1, plot.x, plot.y,
            );
        }
        if r.floors == Some(0) {
            bail!("footprint[{i}].floors: 0; a rect has at least one floor");
        }
    }
    // Half-open spans along one axis, and whether two of them share a cell.
    let span = |a: i32, len: i32| (a, a + len);
    let overlap = |(a0, a1): (i32, i32), (b0, b1): (i32, i32)| a0.max(b0) < a1.min(b1);
    for (i, a) in fp.iter().enumerate() {
        for (j, b) in fp.iter().enumerate().take(i) {
            if overlap(span(a.x, a.w), span(b.x, b.w)) && overlap(span(a.z, a.h), span(b.z, b.h)) {
                bail!("footprint[{i}]: overlaps footprint[{j}]; rects sit side by side, sharing an edge");
            }
        }
    }
    // Touching: abutting along x with overlapping z extents, or the reverse.
    let touch = |a: &RectSpec, b: &RectSpec| {
        ((a.x + a.w == b.x || b.x + b.w == a.x) && overlap(span(a.z, a.h), span(b.z, b.h)))
            || ((a.z + a.h == b.z || b.z + b.h == a.z) && overlap(span(a.x, a.w), span(b.x, b.w)))
    };
    let mut reached = vec![false; fp.len()];
    reached[0] = true;
    let mut stack = vec![0];
    while let Some(i) = stack.pop() {
        for j in 0..fp.len() {
            if !reached[j] && touch(&fp[i], &fp[j]) {
                reached[j] = true;
                stack.push(j);
            }
        }
    }
    if let Some(i) = reached.iter().position(|r| !r) {
        bail!("footprint[{i}]: does not touch the rest of the footprint; every rect shares an edge with another (no gaps, no corner-only contact)");
    }
    if let Some(core) = fp[0].floors {
        if let Some((i, f)) = fp.iter().enumerate().skip(1).find_map(|(i, r)| r.floors.filter(|&f| f > core).map(|f| (i, f))) {
            bail!("footprint[{i}].floors: {f} is more than the core's {core}; the core (footprint[0]) has the most floors");
        }
    }
    Ok(())
}

/// `rooms`: one list per footprint rect, one type per floor plus the attic.
/// Needs the footprint and every rect's floors pinned, so the shape is known.
fn check_rooms(spec: &HouseSpec, has_attic: bool) -> anyhow::Result<()> {
    let Some(rooms) = &spec.rooms else { return Ok(()) };
    let Some(fp) = &spec.footprint else {
        bail!("rooms: pinned but footprint is open; room types are per footprint rect, so pin the footprint (with floors) too");
    };
    if rooms.len() != fp.len() {
        bail!("rooms: {} lists but footprint has {} rects; give one list per rect, in footprint order", rooms.len(), fp.len());
    }
    for (i, (list, r)) in rooms.iter().zip(fp).enumerate() {
        let Some(floors) = r.floors else {
            bail!("rooms[{i}]: footprint[{i}].floors is open; pin every rect's floors to pin its rooms");
        };
        let want = floors as usize + usize::from(has_attic);
        if list.len() != want {
            bail!(
                "rooms[{i}]: {} entries but rect {i} has {floors} floor{}{} ({want} entries, bottom floor first)",
                list.len(),
                if floors == 1 { "" } else { "s" },
                if has_attic { " plus its attic room" } else { "" },
            );
        }
    }
    Ok(())
}

/// `cellar` as the build takes it: (dig?, theme key), each `None` = rolled.
fn resolve_cellar(cellar: Option<&CellarSpec>) -> anyhow::Result<(Option<bool>, Option<&'static str>)> {
    Ok(match cellar {
        None => (None, None),
        Some(CellarSpec::Built(b)) => (Some(*b), None),
        Some(CellarSpec::Themed { theme }) => {
            let name = |k: &&'static str| k.trim_start_matches("cellar_");
            let key = CELLAR_THEMES.iter().find(|k| name(k) == theme).ok_or_else(|| {
                let names: Vec<_> = CELLAR_THEMES.iter().map(|k| name(&k)).collect();
                anyhow!("cellar.theme: unknown theme {theme:?}; one of {}", names.join(", "))
            })?;
            (Some(true), Some(*key))
        }
    })
}

/// The palette: the style pinned (by key), `default`, or drawn from the
/// culture's catalog on the `palette` stream and rolled there; then every
/// pinned material replaces the style's pick. The style draw is made even
/// when the style is pinned, so pinning the drawn style leaves the pool rolls
/// where they were.
fn resolve_palette(
    spec: Option<&PaletteSpec>,
    culture: Culture,
    seed: HouseSeed,
    data: &LoadedData,
) -> anyhow::Result<(Palette, PalettePicks)> {
    let empty = PaletteSpec::default();
    let spec = spec.unwrap_or(&empty);
    let catalog = culture.style_catalog();
    let mut picks = match spec.style.as_deref() {
        Some(DEFAULT_STYLE) => PalettePicks::base(culture.palette_id()),
        pinned => {
            let mut rng = seed.stream("palette");
            let drawn = rng.rand_i32_range(0, catalog.len() as i32) as usize;
            let style = match pinned {
                None => &catalog[drawn],
                Some(key) => catalog.iter().find(|s| style_key(s.name) == key).ok_or_else(|| {
                    let names: Vec<String> =
                        std::iter::once(DEFAULT_STYLE.to_string()).chain(catalog.iter().map(|s| style_key(s.name))).collect();
                    anyhow!("palette.style: unknown {} style {key:?}; one of {}", label(culture), names.join(", "))
                })?,
            };
            style.roll_palette_picks(&mut rng, data, None).1
        }
    };
    if let Some(base) = &spec.base {
        check_palette_id("palette.base", base, data)?;
        picks.base = base.clone();
    }
    for (field, pin, pick) in [
        ("wood", &spec.wood, &mut picks.wood),
        ("stone", &spec.stone, &mut picks.stone),
        ("roof", &spec.roof, &mut picks.roof),
        ("accent", &spec.accent, &mut picks.accent),
    ] {
        if let Some(id) = pin {
            check_palette_id(&format!("palette.{field}"), id, data)?;
            *pick = Some(id.clone());
        }
    }
    // Merged in roll_palette_picks' order, so unpinned picks give its palette.
    let fetch = |id: &PaletteId| data.palettes.get(id).cloned().ok_or_else(|| anyhow!("palette {id:?} not found"));
    let mut palette = fetch(&picks.base)?;
    for id in [&picks.wood, &picks.stone, &picks.roof, &picks.accent].into_iter().flatten() {
        palette = palette.merged_with(&fetch(id)?);
    }
    Ok((palette, picks))
}

fn check_palette_id(field: &str, id: &PaletteId, data: &LoadedData) -> anyhow::Result<()> {
    if data.palettes.contains_key(id) {
        return Ok(());
    }
    let mut names: Vec<String> = data.palettes.keys().map(|k| k.clone().into()).collect();
    names.sort();
    let name: String = id.clone().into();
    bail!("{field}: unknown palette {name:?}; one of {}", names.join(", "))
}

/// The snake_case word serde writes for an enum value (`Culture::Japanese` -> `japanese`).
fn label<T: Serialize>(value: T) -> String {
    serde_yaml::to_string(&value).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// A roof as a spec writes it, on one line (`{ gable: slab }`, `flat`).
fn roof_name(roof: RoofStyle) -> String {
    match roof {
        RoofStyle::Flat | RoofStyle::Irimoya => label(roof),
        RoofStyle::Gable(p) => format!("{{ gable: {} }}", label(p)),
        RoofStyle::Hipped(p) => format!("{{ hipped: {} }}", label(p)),
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
#[path = "spec_tests.rs"]
mod build_tests;

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
