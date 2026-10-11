//! [`SettlementReport`]: the facts a town run prints, captured as data so a run can be
//! saved as `metrics.json` next to its `settlement.scene` and compared with another.

use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// What [`generate_settlement_with`](super::settlement::generate_settlement_with) built.
/// Counts mirror the `println!` summaries of the same run. Lists of pairs keep
/// a stable order (sorted by name) so two reports diff cleanly.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SettlementReport {
    pub seed: i64,
    /// `live` | `offline`, filled in by the caller that saves the report.
    pub source: String,
    /// RFC 3339 time the report was saved, filled in by the caller.
    pub captured_at: String,
    pub culture: String,
    pub settlement_name: String,
    pub settlement_subtitle: String,
    /// Build area origin `[x, y, z]` (absolute) and size `[x, y, z]`.
    pub build_area_origin: [i32; 3],
    pub build_area_size: [i32; 3],
    pub urban_super_parcels: usize,
    pub total_super_parcels: usize,
    pub wall_type: String,
    pub wall_material: String,
    pub gates: usize,
    /// Named urban districts, `name (quarter)`.
    pub districts: Vec<String>,
    pub lots: usize,
    pub buildings_total: usize,
    /// `(SizeClass, count)`.
    pub buildings_by_size: Vec<(String, usize)>,
    /// `(RoofStyle, count)`.
    pub buildings_by_roof: Vec<(String, usize)>,
    pub industrial_placed: usize,
    pub industrial_attempted: usize,
    pub rural_buildings: usize,
    pub rural_parcels: usize,
    pub rural_road_segments: usize,
    /// Urban road network segments, all tiers.
    pub road_segments: usize,
    /// `(PathPriority, count)` for the urban road network.
    pub road_segments_by_tier: Vec<(String, usize)>,
    pub street_lamps: usize,
    pub street_signs: usize,
    pub ships: usize,
    /// Resident target (sum of per-house bed budgets).
    pub population: usize,
    pub workers: usize,
    pub guards: usize,
    pub market_stalls: usize,
    pub stages: usize,
    pub performer_slots: usize,
    /// `(job, count)` from the jobs summary, largest first.
    pub npcs_by_job: Vec<(String, usize)>,
    /// `(stage, milliseconds)` in pipeline order.
    pub timings_ms: Vec<(String, u64)>,
}

impl SettlementReport {
    /// Record the time since `*t` as stage `name` and restart the clock.
    pub fn lap(&mut self, t: &mut Instant, name: &str) {
        self.timings_ms.push((name.to_string(), t.elapsed().as_millis() as u64));
        *t = Instant::now();
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json).with_context(|| format!("write {}", path.display()))
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<SettlementReport> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Map counts to a name-sorted list for the report.
pub fn sorted_counts<'a>(counts: impl IntoIterator<Item = (&'a String, &'a usize)>) -> Vec<(String, usize)> {
    let mut v: Vec<(String, usize)> = counts.into_iter().map(|(k, &n)| (k.clone(), n)).collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_report_roundtrips_through_json() {
        let r = SettlementReport {
            seed: -42,
            source: "offline".into(),
            culture: "Medieval".into(),
            settlement_name: "Ashford".into(),
            build_area_origin: [-100, 60, 20],
            build_area_size: [256, 128, 256],
            districts: vec!["Millbrook (north)".into()],
            buildings_total: 3,
            buildings_by_size: vec![("Cottage".into(), 2), ("House".into(), 1)],
            npcs_by_job: vec![("smith".into(), 2)],
            timings_ms: vec![("parcels".into(), 120), ("roads".into(), 45)],
            ..Default::default()
        };
        let path = std::env::temp_dir().join("tome_report_test").join("metrics.json");
        r.save(&path).unwrap();
        let l = SettlementReport::load(&path).unwrap();
        assert_eq!(serde_json::to_value(&l).unwrap(), serde_json::to_value(&r).unwrap());
        assert_eq!(l.timings_ms[1], ("roads".to_string(), 45));
        assert_eq!(l.build_area_origin, [-100, 60, 20]);
    }
}
