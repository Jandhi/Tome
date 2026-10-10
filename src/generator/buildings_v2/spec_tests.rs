//! Building from house specs: rebuilds, pins, sweeps and errors (offline).

use super::*;
use crate::editor::World;
use crate::generator::buildings_v2::roof::hipped::HippedPitch;
use crate::generator::buildings_v2::showcase::spec_house_seed;
use crate::geometry::Rect3D;
use crate::util::HashMap;

const CELL: i32 = 48;
const MARGIN: i32 = 6;
const GROUND_Y: i32 = 64;

type Blocks = HashMap<Point3D, String>;

/// The showcase's plot in a 48-block cell (36x36, what manor runs use).
fn plot_bounds() -> Rect2D {
    Rect2D::from_points(Point2D::new(MARGIN, MARGIN), Point2D::new(CELL - 1 - MARGIN, CELL - 1 - MARGIN))
}

fn fresh_editor() -> Editor {
    let build_area = Rect3D::from_points(Point3D::new(0, 0, 0), Point3D::new(CELL - 1, 159, CELL - 1));
    World::synthetic(build_area, GROUND_Y).get_offline_editor()
}

async fn capture(editor: Editor) -> Blocks {
    editor.flush_buffer().await;
    let scene = editor.capture_scene();
    scene.blocks.iter().map(|(p, &i)| (*p, scene.palette[i as usize].clone())).collect()
}

/// Build `spec` with `seed` on a fresh flat world the way `--houses --spec`
/// does. `None` when no generated footprint fits; panics on a failed build.
async fn build_spec(data: &LoadedData, spec: &HouseSpec, seed: HouseSeed) -> anyhow::Result<Option<(Blocks, HouseOutput, HouseSpec)>> {
    let Some(resolved) = resolve(spec, seed, plot_bounds(), data)? else { return Ok(None) };
    let mut editor = fresh_editor();
    let (house, out) = resolved.build(&mut editor, data).await.map_err(|e| anyhow!(e))?;
    Ok(Some((capture(editor).await, house, out)))
}

/// The plain `--houses` build as it was before specs (no resolver): culture
/// palette, `house_context`, a generated footprint. Returns its emitted spec.
async fn build_plain(
    data: &LoadedData,
    culture: Culture,
    size: SizeClass,
    roof: RoofStyle,
    seed: HouseSeed,
) -> Option<(Blocks, HouseOutput, HouseSpec)> {
    let mut editor = fresh_editor();
    let palette = data.palettes.get(&culture.palette_id()).expect("palette").clone();
    let footprint = house_footprint(None, plot_bounds(), size, culture, seed).expect("ok")?;
    let bctx = house_context(culture, size, roof, seed);
    let mut rng = RNG::new(seed.0);
    let mut ctx = BuildCtx::new(&mut editor, data, &palette, &mut rng);
    let house = build_house(&mut ctx, footprint, &bctx, plot_bounds()).await.expect("invariants hold");
    let picks = PalettePicks::base(culture.palette_id());
    let mut spec = HouseSpec::resolved(&bctx, &house, &palette, &picks, plot_bounds().min(), Point3D::new(0, 0, 0));
    spec.site.as_mut().unwrap().plot = Some([plot_bounds().size.x, plot_bounds().size.y]);
    Some((capture(editor).await, house, spec))
}

/// Positions whose block differs between two scenes (either side missing counts).
fn diff_count(a: &Blocks, b: &Blocks) -> usize {
    a.iter().filter(|(p, s)| b.get(*p) != Some(*s)).count() + b.keys().filter(|p| !a.contains_key(*p)).count()
}

fn yaml_round_trip(spec: &HouseSpec) -> HouseSpec {
    HouseSpec::from_yaml(&spec.to_yaml().unwrap()).unwrap()
}

/// Cultures and sizes covering every roof family: gable (with an attic),
/// a Japanese manor (always an engawa), hipped, irimoya and desert flat roofs.
fn cases() -> Vec<(Culture, SizeClass, RoofStyle)> {
    vec![
        (Culture::Medieval, SizeClass::Manor, RoofStyle::Gable(GablePitch::Stairs)),
        (Culture::Medieval, SizeClass::House, RoofStyle::Gable(GablePitch::Double)),
        (Culture::Medieval, SizeClass::Cottage, RoofStyle::Gable(GablePitch::Slab)),
        (Culture::Japanese, SizeClass::Manor, RoofStyle::Irimoya),
        (Culture::Japanese, SizeClass::Hall, RoofStyle::Hipped(HippedPitch::Slab)),
        (Culture::Desert, SizeClass::Hall, RoofStyle::Flat),
        (Culture::Desert, SizeClass::Cottage, RoofStyle::Flat),
    ]
}

/// A house built without a spec, then rebuilt from the spec it emitted
/// (through YAML), is the same house block for block.
#[tokio::test]
async fn emitted_spec_rebuilds_the_same_house() {
    let data = LoadedData::load().expect("data");
    let mut rebuilt = 0;
    let mut engawa = false;
    for (culture, size, roof) in cases() {
        for s in 0..2 {
            let seed = HouseSeed(5000 + 17 * s + rebuilt as i64);
            let Some((plain, house, spec)) = build_plain(&data, culture, size, roof, seed).await else { continue };
            engawa |= spec.engawa == Some(true) && house.frame.rect_count() > 0 && culture == Culture::Japanese;
            let (again, _, again_spec) = build_spec(&data, &yaml_round_trip(&spec), HouseSeed(spec.seed.unwrap())).await.unwrap().expect("pinned footprint fits");
            let diffs = diff_count(&plain, &again);
            assert_eq!(diffs, 0, "{culture:?} {size:?} seed {}: rebuild changed {diffs} blocks\n{}", seed.0, spec.to_yaml().unwrap());
            assert_eq!(again_spec, spec, "the rebuild's spec is the spec it was built from");
            rebuilt += 1;
        }
    }
    assert!(rebuilt >= 12, "only {rebuilt} houses fit their plots");
    assert!(engawa, "no engawa house was rebuilt");
}

/// An open spec (just a seed, and a culture) draws everything, and the spec
/// it resolves to rebuilds the same house: palette styles and colours drawn
/// on their streams round-trip too.
#[tokio::test]
async fn open_spec_resolves_to_a_spec_that_rebuilds_it() {
    let data = LoadedData::load().expect("data");
    let mut rebuilt = 0;
    for culture in [None, Some(Culture::Japanese), Some(Culture::Desert)] {
        for s in 0..3 {
            let spec = HouseSpec { seed: Some(700 + s), culture, ..Default::default() };
            let Some((blocks, _, resolved)) = build_spec(&data, &spec, HouseSeed(spec.seed.unwrap())).await.unwrap() else { continue };
            assert_eq!(resolved.seed, spec.seed);
            assert!(resolved.palette.as_ref().and_then(|p| p.style.as_deref()).is_some_and(|s| s != DEFAULT_STYLE));
            let (again, _, _) = build_spec(&data, &yaml_round_trip(&resolved), HouseSeed(resolved.seed.unwrap())).await.unwrap().expect("fits");
            assert_eq!(diff_count(&blocks, &again), 0, "{culture:?} seed {}:\n{}", 700 + s, resolved.to_yaml().unwrap());
            rebuilt += 1;
        }
    }
    assert!(rebuilt >= 7, "only {rebuilt} houses fit their plots");
}

/// A spec holding the seed plus what plain `--houses` pins from its flags
/// (culture, size, roof, the default palette and its colour) builds the same
/// house as the plain build with that house seed: the resolver adds nothing.
#[tokio::test]
async fn seed_spec_matches_the_plain_build() {
    let data = LoadedData::load().expect("data");
    for (culture, size, roof) in cases() {
        let seed = HouseSeed(31);
        let Some((plain, _, _)) = build_plain(&data, culture, size, roof, seed).await else { continue };
        let spec = HouseSpec::from_yaml(&format!(
            "seed: 31\nculture: {}\nsize: {}\nroof: {}\npalette: {{ style: default }}\ncolour: red\njetty: false\n",
            label(culture), label(size), roof_name(roof),
        ))
        .unwrap();
        let (built, _, _) = build_spec(&data, &spec, HouseSeed(31)).await.unwrap().expect("fits");
        assert_eq!(diff_count(&plain, &built), 0, "{culture:?} {size:?}");
    }
}

/// Pinning any one field to the value an open build drew changes nothing.
#[tokio::test]
async fn pinning_the_drawn_value_changes_nothing() {
    let data = LoadedData::load().expect("data");
    let mut checked = 0;
    for (culture, s) in [(Culture::Medieval, 0), (Culture::Medieval, 1), (Culture::Japanese, 0), (Culture::Desert, 0)] {
        let open = HouseSpec { seed: Some(900 + s), culture: Some(culture), ..Default::default() };
        let seed = HouseSeed(900 + s);
        let Some((blocks, _, r)) = build_spec(&data, &open, seed).await.unwrap() else { continue };
        let fp_no_floors: Vec<RectSpec> = r.footprint.clone().unwrap().into_iter().map(|f| RectSpec { floors: None, ..f }).collect();
        let pins: Vec<(&str, HouseSpec)> = vec![
            ("culture", HouseSpec { culture: r.culture, ..open.clone() }),
            ("size", HouseSpec { size: r.size, ..open.clone() }),
            ("palette", HouseSpec { palette: r.palette.clone(), ..open.clone() }),
            (
                "palette.style",
                HouseSpec { palette: Some(PaletteSpec { style: r.palette.as_ref().unwrap().style.clone(), ..Default::default() }), ..open.clone() },
            ),
            ("colour", HouseSpec { colour: r.colour, ..open.clone() }),
            ("roof", HouseSpec { roof: r.roof, ..open.clone() }),
            ("attic", HouseSpec { attic: r.attic, ..open.clone() }),
            ("footprint", HouseSpec { footprint: Some(fp_no_floors.clone()), ..open.clone() }),
            ("footprint+floors", HouseSpec { footprint: r.footprint.clone(), ..open.clone() }),
            ("rooms", HouseSpec { footprint: r.footprint.clone(), rooms: r.rooms.clone(), roof: r.roof, ..open.clone() }),
            ("jetty", HouseSpec { jetty: r.jetty, ..open.clone() }),
            ("engawa", HouseSpec { engawa: r.engawa, ..open.clone() }),
            ("knee_wall", HouseSpec { knee_wall: r.knee_wall, ..open.clone() }),
            ("tall_top_floor", HouseSpec { tall_top_floor: r.tall_top_floor, ..open.clone() }),
            ("timber", HouseSpec { timber: r.timber, ..open.clone() }),
            ("cellar", HouseSpec { cellar: r.cellar.clone(), ..open.clone() }),
        ];
        for (field, spec) in pins {
            let (pinned, _, _) = build_spec(&data, &spec, seed).await.unwrap().expect("fits");
            let diffs = diff_count(&blocks, &pinned);
            assert_eq!(diffs, 0, "{culture:?} seed {}: pinning {field} changed {diffs} blocks\n{}", seed.0, r.to_yaml().unwrap());
        }
        // A cellar pinned `true` (theme open) where one was dug is the same too.
        if matches!(r.cellar, Some(CellarSpec::Themed { .. })) {
            let spec = HouseSpec { cellar: Some(CellarSpec::Built(true)), ..open.clone() };
            let (pinned, _, _) = build_spec(&data, &spec, seed).await.unwrap().expect("fits");
            assert_eq!(diff_count(&blocks, &pinned), 0, "cellar: true");
        }
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} houses fit their plots");
}

/// Pinning another timber pattern changes timber blocks, not the shape or
/// the rooms; pinning another roof leaves the palette, timber and footprint.
#[tokio::test]
async fn pins_leave_unrelated_decisions_alone() {
    let data = LoadedData::load().expect("data");
    let (mut compared, mut timber_changed) = (0, false);
    for s in 0..4 {
        let seed = HouseSeed(1200 + s);
        let open = HouseSpec { culture: Some(Culture::Medieval), size: Some([SizeClass::House, SizeClass::Hall, SizeClass::Manor][s as usize % 3]), ..Default::default() };
        let Some((blocks, _, r)) = build_spec(&data, &open, seed).await.unwrap() else { continue };

        let other_timber = if r.timber == Some(TimberPattern::Plain) { TimberPattern::Braced { spacing: 3 } } else { TimberPattern::Plain };
        let (tb, _, t) = build_spec(&data, &HouseSpec { timber: Some(other_timber), ..open.clone() }, seed).await.unwrap().expect("fits");
        assert_eq!(t.timber, Some(other_timber));
        timber_changed |= diff_count(&blocks, &tb) > 0;
        assert_eq!(t.footprint, r.footprint, "seed {}: footprint or floors moved with the timber", seed.0);
        assert_eq!(t.rooms, r.rooms, "seed {}: room types moved with the timber", seed.0);
        assert_eq!((&t.palette, t.roof, t.colour, &t.cellar), (&r.palette, r.roof, r.colour, &r.cellar));

        let roofs = Culture::Medieval.roof_styles();
        let other_roof = *roofs.iter().find(|&&x| Some(x) != r.roof).unwrap();
        let (_, _, o) = build_spec(&data, &HouseSpec { roof: Some(other_roof), ..open.clone() }, seed).await.unwrap().expect("fits");
        assert_eq!(o.roof, Some(other_roof));
        assert_eq!(o.palette, r.palette, "seed {}: palette moved with the roof", seed.0);
        assert_eq!(o.timber, r.timber, "seed {}: timber moved with the roof", seed.0);
        assert_eq!(o.footprint, r.footprint, "seed {}: footprint moved with the roof", seed.0);
        assert_eq!(o.colour, r.colour);
        compared += 1;
    }
    assert!(compared >= 3, "only {compared} houses fit their plots");
    assert!(timber_changed, "no other timber pattern changed a block");
}

/// The same spec swept twice builds the same houses; a spec pinning only the
/// culture and footprint keeps both across the sweep and varies the rest.
#[tokio::test]
async fn sweeps_repeat_and_vary_only_open_fields() {
    let data = LoadedData::load().expect("data");
    let spec = HouseSpec::from_yaml(
        "culture: medieval\nfootprint:\n  - { x: 4, z: 4, w: 9, h: 7 }\n  - { x: 13, z: 5, w: 5, h: 5 }\n",
    )
    .unwrap();
    let count = 5;
    let mut first = Vec::new();
    for k in 0..count {
        let (a, _, ra) = build_spec(&data, &spec, spec_house_seed(77, k, count)).await.unwrap().expect("fits");
        let (b, _, _) = build_spec(&data, &spec, spec_house_seed(77, k, count)).await.unwrap().expect("fits");
        assert_eq!(diff_count(&a, &b), 0, "sweep house {k} differs between runs");
        first.push(ra);
    }
    let seeds: Vec<_> = (0..count).map(|k| spec_house_seed(77, k, count)).collect();
    assert!(seeds.windows(2).all(|w| w[0] != w[1]), "sweep seeds repeat");
    assert_eq!(spec_house_seed(77, 0, 1), HouseSeed(77), "a single house keeps the spec's seed");
    let rects = |s: &HouseSpec| s.footprint.as_ref().unwrap().iter().map(|r| (r.x, r.z, r.w, r.h)).collect::<Vec<_>>();
    for r in &first {
        assert_eq!(r.culture, Some(Culture::Medieval));
        assert_eq!(rects(r), rects(&spec), "footprint moved in the sweep");
    }
    let looks: crate::util::HashSet<String> =
        first.iter().map(|r| format!("{:?} {:?} {:?} {:?}", r.palette, r.roof, r.size, r.colour)).collect();
    assert!(looks.len() > 1, "every sweep house drew the same palette, roof, size and colour");
}

/// Each malformed spec is refused with an error naming the field.
#[tokio::test]
async fn errors_name_the_field() {
    let data = LoadedData::load().expect("data");
    let cases: [(&str, &str); 15] = [
        ("palette: { style: plywood }", "palette.style: unknown medieval style \"plywood\"; one of default, timber_spruce"),
        ("palette: { wood: balsa }", "palette.wood: unknown palette \"balsa\""),
        ("cellar: { theme: dungeon }", "cellar.theme: unknown theme \"dungeon\"; one of storage, wine, larder, workshop"),
        ("footprint: []", "footprint: empty"),
        ("footprint: [{ x: 0, z: 0, w: 0, h: 5 }]", "footprint[0]: size 0x5 is empty"),
        ("footprint: [{ x: -1, z: 0, w: 5, h: 5 }]", "footprint[0]: x -1..3, z 0..4 is outside the 36x36 plot"),
        ("footprint: [{ x: 30, z: 0, w: 9, h: 5 }]", "footprint[0]: x 30..38, z 0..4 is outside"),
        ("footprint: [{ x: 0, z: 0, w: 9, h: 7 }, { x: 8, z: 0, w: 5, h: 5 }]", "footprint[1]: overlaps footprint[0]"),
        ("footprint: [{ x: 0, z: 0, w: 9, h: 7 }, { x: 10, z: 0, w: 5, h: 5 }]", "footprint[1]: does not touch"),
        ("footprint: [{ x: 0, z: 0, w: 9, h: 7, floors: 1 }, { x: 9, z: 0, w: 5, h: 5, floors: 2 }]", "footprint[1].floors: 2 is more than the core's 1"),
        (
            "roof: { gable: slab }\nfootprint: [{ x: 0, z: 0, w: 9, h: 7, floors: 2 }]\nrooms: [[hearth, bedroom, study]]",
            "rooms[0]: 3 entries but rect 0 has 2 floors",
        ),
        ("roof: flat\nfootprint: [{ x: 0, z: 0, w: 9, h: 7 }]\nrooms: [[hearth]]", "rooms[0]: footprint[0].floors is open"),
        ("roof: flat\nfootprint: [{ x: 0, z: 0, w: 9, h: 7, floors: 1 }]\nrooms: [[hearth], [study]]", "rooms: 2 lists but footprint has 1 rects"),
        ("roof: { gable: slab }\nattic: true", "attic: true contradicts roof: { gable: slab }"),
        ("culture: desert\nattic: true", "attic: true, but no desert roof has an attic"),
    ];
    for (yaml, want) in cases {
        let spec = HouseSpec::from_yaml(yaml).unwrap_or_else(|e| panic!("{yaml}: {e}"));
        let err = match build_spec(&data, &spec, HouseSeed(1)).await {
            Ok(_) => panic!("{yaml}: built, expected `{want}`"),
            Err(e) => format!("{e:#}"),
        };
        assert!(err.contains(want), "{yaml}:\n  got  {err}\n  want {want}");
    }

    // Unknown enum words fail to parse, naming the file and the valid words.
    let path = std::env::temp_dir().join("tome_bad_roof.spec.yaml");
    std::fs::write(&path, "roof: pagoda\n").unwrap();
    let err = format!("{:#}", HouseSpec::load(&path).unwrap_err());
    assert!(err.contains("tome_bad_roof.spec.yaml") && err.contains("pagoda") && err.contains("irimoya"), "{err}");
    std::fs::write(&path, "size: castle\n").unwrap();
    let err = format!("{:#}", HouseSpec::load(&path).unwrap_err());
    assert!(err.contains("castle") && err.contains("manor"), "{err}");
    let _ = std::fs::remove_file(&path);
}
