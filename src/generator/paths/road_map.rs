//! Top-down SVG town map of a generated settlement.
//!
//! Rasterises the settlement one cell per pixel — grass background, water,
//! building/wall/gate footprints, alleys — then recolours each named road by its
//! `road_id` (the 16-hue palette mirroring the in-world wool labels), prints each
//! road's id at its centroid, and optionally overlays the abstract road graph
//! (MST + shortcut edges with numbered nodes). Written to `output/town.svg`.
//!
//! Shared by the placement test and the live `generate_town` pipeline so there's
//! a single town-map renderer.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::editor::World;
use crate::generator::BuildClaim;
use crate::geometry::{get_surrounding_set, Point2D, Point3D};

use super::network::RoadNetwork;
use super::path::Path;

/// 16-hue road palette, mirroring the in-world wool road labels.
const ROAD_SVG: [&str; 16] = [
    "#e9ecec", "#f9801d", "#c74ebd", "#3ab3da", "#fed83d", "#80c71f", "#f38baa", "#474f52",
    "#9d9d97", "#169c9c", "#8932b8", "#3c44aa", "#835432", "#5e7c16", "#b02e26", "#1d1d21",
];

/// Render the town map. `urban` sets the drawn bounds; `paths` supplies the road
/// geometry (paved width), `road_labels` the geometric per-cell road number used
/// to colour them; `road_names` labels each road (falling back to its number);
/// `alleys` cover the unnamed paths; `network` adds the abstract MST/node overlay
/// when present; `signs` are marked as dots; `places` are named open spaces
/// (plazas/parks) drawn as labels at their centroid.
pub fn render_town_map(
    world: &World,
    urban: &HashSet<Point2D>,
    paths: &[Path],
    road_labels: &HashMap<Point2D, u32>,
    road_names: &HashMap<u32, String>,
    alleys: &HashSet<Point2D>,
    network: Option<&RoadNetwork>,
    signs: &[Point2D],
    places: &[(Point2D, String)],
) -> String {
    if urban.is_empty() {
        return String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n");
    }

    // Bounds from the urban footprint, padded.
    let (mut minx, mut minz, mut maxx, mut maxz) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for c in urban {
        minx = minx.min(c.x);
        maxx = maxx.max(c.x);
        minz = minz.min(c.y);
        maxz = maxz.max(c.y);
    }
    let pad = 3;
    minx -= pad;
    minz -= pad;
    maxx += pad;
    maxz += pad;
    let (w, h) = (maxx - minx + 1, maxz - minz + 1);

    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" \
         width=\"{}\" height=\"{}\" shape-rendering=\"crispEdges\">\n",
        w * 4,
        h * 4
    );
    let _ = write!(
        svg,
        "<rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#b9d68a\"/>\n"
    );

    // Base layer: water / footprints / wall / alleys (roads drawn on top).
    for z in minz..=maxz {
        for x in minx..=maxx {
            let c = Point2D::new(x, z);
            if !world.is_in_bounds_2d(c) {
                continue;
            }
            let fill = if alleys.contains(&c) {
                "#b8b8b8"
            } else {
                match world.get_claim(c) {
                    Some(BuildClaim::Wall) => "#3a3a3a",
                    Some(BuildClaim::Gate) => "#6a6a6a",
                    Some(BuildClaim::Building(_) | BuildClaim::Structure(_)) => "#d9cfa3",
                    // Pavement: named roads get recoloured on top; this keeps
                    // discarded (unnamed) short roads visible as grey.
                    Some(BuildClaim::Path(_)) => "#c4c4c4",
                    _ if world.is_water(c) => "#4a6fb0",
                    _ => continue,
                }
            };
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\" fill=\"{}\"/>\n",
                x - minx,
                z - minz,
                fill
            );
        }
    }

    // Road layer: paint each path's full paved width with the geometric road
    // number of its underlying centreline cell, so a colour = one continuous
    // physical road. Centroids (from centreline cells only) anchor the labels.
    let mut centroid: HashMap<u32, (i64, i64, i64)> = HashMap::new(); // rid -> (sumx, sumz, count)
    for path in paths {
        let widen = path.width().saturating_sub(1);
        for p in path.points() {
            let centre = p.drop_y();
            let Some(&rid) = road_labels.get(&centre) else { continue };
            // The centreline cell plus its paved shoulder, all this road's colour.
            let mut swath = get_surrounding_set(&HashSet::from([centre]), widen);
            swath.insert(centre);
            for c in swath {
                let _ = write!(
                    svg,
                    "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\" fill=\"{}\"/>\n",
                    c.x - minx,
                    c.y - minz,
                    ROAD_SVG[rid as usize % ROAD_SVG.len()]
                );
            }
            let e = centroid.entry(rid).or_insert((0, 0, 0));
            e.0 += (centre.x - minx) as i64;
            e.1 += (centre.y - minz) as i64;
            e.2 += 1;
        }
    }

    // Abstract graph overlay: the MST + shortcut edges as straight thin lines
    // between nodes (the structure before A* curved it). Arterials thicker;
    // shortcuts dashed. Nodes are numbered.
    if let Some(net) = network {
        let nx = |p: Point3D| p.x - minx;
        let nz = |p: Point3D| p.z - minz;
        for e in &net.edges {
            let (pa, pb) = (net.nodes[e.a], net.nodes[e.b]);
            let sw = if e.arterial { "1.2" } else { "0.6" };
            let dash = if e.shortcut { " stroke-dasharray=\"2,2\"" } else { "" };
            let _ = write!(
                svg,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#111\" \
                 stroke-width=\"{}\" stroke-opacity=\"0.85\"{}/>\n",
                nx(pa),
                nz(pa),
                nx(pb),
                nz(pb),
                sw,
                dash
            );
        }
        for (i, p) in net.nodes.iter().enumerate() {
            let _ = write!(
                svg,
                "<circle cx=\"{}\" cy=\"{}\" r=\"1.6\" fill=\"#111\" stroke=\"#fff\" stroke-width=\"0.4\"/>\n",
                nx(*p),
                nz(*p)
            );
            let _ = write!(
                svg,
                "<text x=\"{}\" y=\"{}\" font-size=\"4\" fill=\"#fff\" text-anchor=\"middle\">{}</text>\n",
                nx(*p),
                nz(*p) + 1,
                i
            );
        }
    }

    // Street-sign markers — small red dots.
    for s in signs {
        let _ = write!(
            svg,
            "<circle cx=\"{}\" cy=\"{}\" r=\"1.4\" fill=\"#c0392b\" stroke=\"#fff\" stroke-width=\"0.4\"/>\n",
            s.x - minx,
            s.y - minz
        );
    }

    // Road-name labels last, so they sit above the road fill (number as fallback).
    for (rid, (sx, sz, n)) in &centroid {
        if *n == 0 {
            continue;
        }
        let label = road_names
            .get(rid)
            .cloned()
            .unwrap_or_else(|| rid.to_string());
        let _ = write!(
            svg,
            "<text x=\"{}\" y=\"{}\" font-size=\"5\" font-weight=\"bold\" fill=\"#000\" \
             stroke=\"#fff\" stroke-width=\"1.0\" paint-order=\"stroke\" text-anchor=\"middle\">{}</text>\n",
            sx / n,
            sz / n + 2,
            xml_escape(&label)
        );
    }

    // Named open spaces (plazas/parks): a small marker + an italic label, in a
    // civic green to set them apart from the road-name labels.
    for (c, name) in places {
        let (x, y) = (c.x - minx, c.y - minz);
        let _ = write!(
            svg,
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"1.6\" fill=\"#1b5e20\" stroke=\"#fff\" stroke-width=\"0.5\"/>\n"
        );
        let _ = write!(
            svg,
            "<text x=\"{x}\" y=\"{}\" font-size=\"5\" font-style=\"italic\" font-weight=\"bold\" \
             fill=\"#1b5e20\" stroke=\"#fff\" stroke-width=\"1.0\" paint-order=\"stroke\" \
             text-anchor=\"middle\">{}</text>\n",
            y - 3,
            xml_escape(name)
        );
    }

    svg.push_str("</svg>\n");
    svg
}

/// Minimal XML text escaping for road names embedded in the SVG.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Rasterise an SVG string to a PNG file using resvg (pure-Rust). Uses the SVG's
/// own width/height for the output size and the system fonts for text.
pub fn rasterize_to_png(svg: &str, path: &str) -> anyhow::Result<()> {
    use resvg::{tiny_skia, usvg};

    let mut opt = usvg::Options::default();
    // Road-name labels use a generic sans-serif; resolve it from system fonts.
    opt.fontdb_mut().load_system_fonts();
    opt.font_family = "Arial".to_string();

    let tree = usvg::Tree::from_str(svg, &opt)
        .map_err(|e| anyhow::anyhow!("parse town SVG: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("zero-size town map"))?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.save_png(path)?;
    Ok(())
}
