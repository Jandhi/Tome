//! `--houses` and `--furniture`: render individual buildings and furniture pieces.

use crate::generator::buildings_v2::furnish::{data::FurnitureData, preview::render_previews};
use crate::generator::buildings_v2::{
    footprint::SizeClass,
    showcase::{parse_rects, run_showcase, ShowcaseOptions, SizeSpec},
    Culture,
};
use crate::render::light::Lighting;

/// `--houses [--culture c] [--seed n] [--count n] [--size s] [--rects "x,z,w,h;..."] [--jetty] [--knee-wall] [--tall-top]
/// [--variants] [--live] [--px n] [--light day|night|dusk|<0..1>]`: build individual houses and render each one,
/// under `output/houses/[live-]<culture>-<seed>[...]/`. Offline on a blank flat
/// world by default; `--live` builds into the server's build area. `--variants`
/// builds one row per upper-storey variant (base, knee, tall, both). `--light`
/// lights the room views with sky light and lamps (the directory gains `-day`, `-night`, ...).
pub async fn run_houses(args: &[String]) -> anyhow::Result<()> {
    let arg_after = |flag: &str, n: usize| {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + n)).map(String::as_str)
    };
    let culture = match arg_after("--culture", 1).unwrap_or("medieval") {
        "medieval" => Culture::Medieval,
        "desert" => Culture::Desert,
        "japanese" => Culture::Japanese,
        other => anyhow::bail!("unknown --culture {other} (medieval|desert|japanese)"),
    };
    let size = match arg_after("--size", 1).unwrap_or("mixed") {
        "mixed" => SizeSpec::Mixed,
        "cottage" => SizeSpec::One(SizeClass::Cottage),
        "house" => SizeSpec::One(SizeClass::House),
        "hall" => SizeSpec::One(SizeClass::Hall),
        "manor" => SizeSpec::One(SizeClass::Manor),
        other => anyhow::bail!("unknown --size {other} (cottage|house|hall|manor|mixed)"),
    };
    let opts = ShowcaseOptions {
        culture,
        seed: arg_after("--seed", 1).map(str::parse).transpose()?.unwrap_or(42),
        count: arg_after("--count", 1).map(str::parse).transpose()?.unwrap_or(6),
        size,
        jetty: args.iter().any(|a| a == "--jetty"),
        knee_wall: args.iter().any(|a| a == "--knee-wall"),
        tall_top: args.iter().any(|a| a == "--tall-top"),
        variants: args.iter().any(|a| a == "--variants"),
        live: args.iter().any(|a| a == "--live"),
        px: arg_after("--px", 1).map(str::parse).transpose()?.unwrap_or(16),
        rects: arg_after("--rects", 1).map(parse_rects).transpose()?,
        lighting: arg_after("--light", 1).map(Lighting::parse).transpose()?,
    };
    run_showcase(&opts).await?;
    Ok(())
}

/// `--furniture <name[,name...]> [out_dir]`: render each named piece alone in a
/// preview corner (default `output/furniture`).
pub fn run_furniture(names: &str, out_dir: Option<&str>) -> anyhow::Result<()> {
    let names: Vec<String> = names.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
    let out_dir = std::path::PathBuf::from(out_dir.unwrap_or("output/furniture"));
    for path in render_previews(&FurnitureData::load()?, &names, &out_dir)? {
        println!("Rendered {}", path.display());
    }
    Ok(())
}
