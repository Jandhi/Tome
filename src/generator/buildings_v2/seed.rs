//! Per-house seed and its named decision streams.
//!
//! Every buildings_v2 house has one [`HouseSeed`]. Each decision about that
//! house draws from its own stream, `RNG::from_seed_and_string(seed, name)`,
//! instead of from one sequential RNG, so how many draws one decision makes
//! (or whether it draws at all, once a spec pins it) never shifts another
//! decision, and one house never shifts the next. A stage may consume its
//! stream sequentially (all stairwells from `stairs`, every room type from
//! `rooms`): the stage is the decision unit.
//!
//! Streams, by name (callers outside `build_house` first):
//!
//! | name | drives |
//! |---|---|
//! | `size` | size class from the lot tier's pool (settlement) |
//! | `core` | core front width, depth and the square roll (settlement) |
//! | `palette` | building style from the district scheme + its palette roll (settlement) |
//! | `colour` | primary colour, and a manor's banner charge + design (settlement) |
//! | `sign` | a manor's name-sign designation and wood (settlement) |
//! | `roof` | roof style (settlement) |
//! | `footprint` | footprint wings / layout (settlement, showcase) |
//! | `engawa`, `jetty` | the veranda / overhang rolls (settlement; showcase: engawa) |
//! | `foundation` | foundation course material variation |
//! | `floors` | floor count per rect (`generate_frame`) |
//! | `doors`, `windows` | door / window placement (draw nothing today) |
//! | `stairs` | stairwell kinds and positions (`select_stairwells`) |
//! | `floors.blocks` | floor slab, ceiling and stair block variation |
//! | `walls.blocks` | wall infill block variation |
//! | `timber` | timber pattern auto-pick |
//! | `frame.blocks` | timber frame placement (posts, braces, panel shapes) |
//! | `roof.ridge` | gable ridge axis per rect |
//! | `roof.chimney` | which tallest rect carries the chimney, and where |
//! | `roof.parapet` | flat-roof parapet style |
//! | `roof.blocks` | roof surface, gable wall and parapet block variation |
//! | `engawa.blocks` | engawa deck and skirt block variation |
//! | `rooms` | interior wall blocks, room budget and room types |
//! | `attic` | attic room types |
//! | `curtains` | window blocks and desert curtains (`place_openings`) |
//! | `ramps` | door ramp block variation |
//! | `furnish` | furniture in every room |
//! | `rooftop` | flat-roof terrace theme and furniture |
//! | `exterior` | props against the outside walls |
//! | `cellar` | whether to dig a cellar, its theme and furniture |
//! | `build` | fallback for anything in `build_house` not given its own stream |

use crate::noise::{RNG, Seed};

/// One house's seed. Copy it around freely; draws come from [`HouseSeed::stream`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HouseSeed(pub i64);

impl HouseSeed {
    /// Draw a house seed from a parent stream: exactly one draw, whatever the
    /// house then does, so the parent's next house is unaffected by this one.
    pub fn draw(rng: &mut RNG) -> Self {
        HouseSeed(rng.next_i64())
    }

    /// A fresh RNG for one named decision of this house (see the module table).
    /// The same seed and name always give the same stream.
    pub fn stream(&self, name: &str) -> RNG {
        RNG::from_seed_and_string(Seed(self.0), name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::World;
    use crate::generator::buildings_v2::footprint::SizeClass;
    use crate::generator::buildings_v2::roof::RoofStyle;
    use crate::generator::buildings_v2::roof::gable::GablePitch;
    use crate::generator::buildings_v2::showcase::{house_context, house_footprint};
    use crate::generator::buildings_v2::{BuildCtx, Culture, HouseOutput, TimberPattern, build_house};
    use crate::generator::data::LoadedData;
    use crate::geometry::{Point2D, Point3D, Rect2D, Rect3D};
    use crate::util::HashMap;

    /// Synthetic world edge and the plot inside it (the showcase's cell margin).
    const CELL: i32 = 48;
    const MARGIN: i32 = 6;
    const GROUND_Y: i32 = 64;

    fn plot_bounds() -> Rect2D {
        Rect2D::from_points(Point2D::new(MARGIN, MARGIN), Point2D::new(CELL - 1 - MARGIN, CELL - 1 - MARGIN))
    }

    /// Build one house the way `--houses` does, on a fresh flat world: the
    /// footprint generated from `seed` or pinned to `rects` (plot-relative), and
    /// optionally a pinned timber pattern. Returns every block the scene holds
    /// (position -> `id[state]`) and the house.
    async fn build_one(
        data: &LoadedData,
        culture: Culture,
        size_class: SizeClass,
        roof_style: RoofStyle,
        seed: HouseSeed,
        rects: Option<&[Rect2D]>,
        timber: Option<TimberPattern>,
    ) -> Option<(HashMap<Point3D, String>, HouseOutput)> {
        let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(CELL - 1, 159, CELL - 1));
        let mut editor = World::synthetic(build_area, GROUND_Y).get_offline_editor();
        let palette = data.palettes.get(&culture.palette_id()).expect("palette").clone();
        let footprint = house_footprint(rects, plot_bounds(), size_class, culture, seed).expect("rects fit")?;
        let mut bctx = house_context(culture, size_class, roof_style, seed);
        bctx.timber_pattern = timber;
        // A caller stream build_house must leave alone: it differs between the
        // drawn and pinned builds, so drawing from it would show up as a diff.
        let mut caller_rng = RNG::new(if rects.is_some() { 2 } else { 1 });
        let mut ctx = BuildCtx::new(&mut editor, data, &palette, &mut caller_rng);
        let house = build_house(&mut ctx, footprint, &bctx, plot_bounds()).await.expect("invariants hold");
        editor.flush_buffer().await;
        let scene = editor.capture_scene();
        let blocks = scene.blocks.iter().map(|(p, &i)| (*p, scene.palette[i as usize].clone())).collect();
        Some((blocks, house))
    }

    /// The footprint as `--rects` takes it: relative to the plot, core first.
    fn relative_rects(house: &HouseOutput) -> Vec<Rect2D> {
        let shift = plot_bounds().min();
        house.footprint.rects().iter().map(|r| Rect2D::new(r.min() - shift, r.size)).collect()
    }

    /// Positions whose block differs between two scenes (either side missing counts).
    fn diff_count(a: &HashMap<Point3D, String>, b: &HashMap<Point3D, String>) -> usize {
        let changed = a.iter().filter(|(p, s)| b.get(*p) != Some(*s)).count();
        let only_b = b.keys().filter(|p| !a.contains_key(*p)).count();
        changed + only_b
    }

    /// Every culture at every size, cycling the culture's roof styles.
    fn cases() -> Vec<(Culture, SizeClass, RoofStyle)> {
        let mut out = Vec::new();
        for culture in [Culture::Medieval, Culture::Japanese, Culture::Desert] {
            let roofs = culture.roof_styles();
            for (k, size) in [SizeClass::Cottage, SizeClass::House, SizeClass::Hall, SizeClass::Manor].into_iter().enumerate() {
                out.push((culture, size, roofs[k % roofs.len()]));
            }
        }
        out
    }

    #[test]
    fn streams_are_keyed_by_seed_and_name() {
        let a = HouseSeed(42);
        assert_eq!(a.stream("floors").next_i64(), a.stream("floors").next_i64());
        assert_ne!(a.stream("floors").next_i64(), a.stream("stairs").next_i64());
        assert_ne!(a.stream("floors").next_i64(), HouseSeed(43).stream("floors").next_i64());
    }

    /// Pinning the footprint to exactly the one the seed draws (what `--rects`
    /// does) builds the identical house: skipping the `footprint` stream moves
    /// no other decision, engawa roll and interior included.
    #[tokio::test]
    async fn pinned_footprint_rebuilds_the_same_house() {
        let data = LoadedData::load().expect("data");
        let mut built = 0;
        for (culture, size, roof) in cases() {
            for s in 0..3 {
                let seed = HouseSeed(1000 + s);
                let Some((drawn, house)) = build_one(&data, culture, size, roof, seed, None, None).await else { continue };
                let rects = relative_rects(&house);
                let (pinned, _) = build_one(&data, culture, size, roof, seed, Some(&rects), None).await.expect("pinned builds");
                let diffs = diff_count(&drawn, &pinned);
                assert_eq!(diffs, 0, "{culture:?} {size:?} {roof:?} seed {}: pinned footprint {rects:?} changed {diffs} blocks", seed.0);
                built += 1;
            }
        }
        assert!(built >= 30, "only {built} houses fit their plots");
    }

    /// Same seed, same house; different seeds, different houses.
    #[tokio::test]
    async fn seed_alone_decides_the_house() {
        let data = LoadedData::load().expect("data");
        for (culture, size, roof) in [
            (Culture::Medieval, SizeClass::House, RoofStyle::Gable(GablePitch::Stairs)),
            (Culture::Desert, SizeClass::Hall, RoofStyle::Flat),
        ] {
            let (a, _) = build_one(&data, culture, size, roof, HouseSeed(7), None, None).await.expect("fits");
            let (a2, _) = build_one(&data, culture, size, roof, HouseSeed(7), None, None).await.expect("fits");
            let (b, _) = build_one(&data, culture, size, roof, HouseSeed(8), None, None).await.expect("fits");
            assert_eq!(diff_count(&a, &a2), 0, "{culture:?}: same seed built differently");
            assert!(diff_count(&a, &b) > 0, "{culture:?}: seeds 7 and 8 built the same house");
        }
    }

    /// Pinning the timber pattern to another value changes the timber, not the
    /// shape or the rooms: floor counts, room types and furniture stay put.
    #[tokio::test]
    async fn pinned_timber_leaves_rooms_alone() {
        let data = LoadedData::load().expect("data");
        let roof = Culture::Medieval.roof_styles()[1];
        let mut compared = 0;
        for s in 0..6 {
            let seed = HouseSeed(2000 + s);
            let size = [SizeClass::House, SizeClass::Hall, SizeClass::Manor][s as usize % 3];
            let Some((_, drawn)) = build_one(&data, Culture::Medieval, size, roof, seed, None, None).await else { continue };
            let other = if drawn.timber_pattern == TimberPattern::Plain {
                TimberPattern::Braced { spacing: 3 }
            } else {
                TimberPattern::Plain
            };
            let (_, pinned) = build_one(&data, Culture::Medieval, size, roof, seed, None, Some(other)).await.expect("fits");
            assert_eq!(pinned.timber_pattern, other);
            assert_eq!(drawn.frame.floor_counts(), pinned.frame.floor_counts(), "seed {}: floor counts moved", seed.0);
            let rooms = |h: &HouseOutput| -> Vec<_> {
                h.room_plan.rooms.iter()
                    .map(|r| (r.rect_index, r.floor, r.room_type, r.furniture.iter().map(|f| f.name.clone()).collect::<Vec<_>>()))
                    .collect()
            };
            assert_eq!(rooms(&drawn), rooms(&pinned), "seed {}: rooms moved with the timber", seed.0);
            compared += 1;
        }
        assert!(compared >= 4, "only {compared} houses fit their plots");
    }
}
