//! Which artifacts a run writes, from `--out`. Settlement runs (`cargo run`,
//! `cargo offline`) and `--houses` each have their own defaults; `--out` either
//! replaces them with a list (`--out scene,renders`) or adjusts them with
//! `+`/`-` (`--out +specs,-renders`). `all` and `none` work as list items.
//! Parsed before generation starts, so a typo fails at once. A run only
//! writes; artifacts it skips keep whatever an earlier run left in the folder.

/// One kind of artifact a run can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Artifact {
    /// `terrain.fixture.nbt.gz`, the untouched terrain snapshot (live runs only).
    Snapshot,
    /// `settlement.scene`, every placed block.
    Scene,
    /// The PNG views (top, iso, elevations). `--houses` always renders.
    Renders,
    /// `metrics.json` for settlements, `houses.json` for `--houses`.
    Metrics,
    /// One `*.spec.yaml` per house.
    Specs,
    /// `viewer.html`, the first-person viewer of the scene.
    Viewer,
}

impl Artifact {
    pub const ALL: [Artifact; 6] =
        [Artifact::Snapshot, Artifact::Scene, Artifact::Renders, Artifact::Metrics, Artifact::Specs, Artifact::Viewer];

    pub fn name(self) -> &'static str {
        match self {
            Artifact::Snapshot => "snapshot",
            Artifact::Scene => "scene",
            Artifact::Renders => "renders",
            Artifact::Metrics => "metrics",
            Artifact::Specs => "specs",
            Artifact::Viewer => "viewer",
        }
    }

    fn parse(s: &str) -> anyhow::Result<Artifact> {
        // `fixture` is the snapshot's old name.
        let s = if s == "fixture" { "snapshot" } else { s };
        Artifact::ALL.into_iter().find(|a| a.name() == s).ok_or_else(|| {
            let names: Vec<&str> = Artifact::ALL.iter().map(|a| a.name()).collect();
            anyhow::anyhow!("--out: unknown artifact `{s}` (valid: {}, all, none)", names.join(", "))
        })
    }

    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// A set of [`Artifact`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outputs(u8);

impl Outputs {
    pub const NONE: Outputs = Outputs(0);

    pub fn of(artifacts: &[Artifact]) -> Outputs {
        Outputs(artifacts.iter().fold(0, |m, a| m | a.bit()))
    }

    /// What a settlement run writes without `--out`: everything but specs and the viewer.
    pub fn settlement_default() -> Outputs {
        Outputs::of(&[Artifact::Snapshot, Artifact::Scene, Artifact::Renders, Artifact::Metrics])
    }

    /// What `--houses` writes without `--out`: everything but the viewer.
    pub fn houses_default() -> Outputs {
        Outputs::of(&[Artifact::Scene, Artifact::Renders, Artifact::Metrics, Artifact::Specs])
    }

    /// This set less `a`.
    pub fn without(self, a: Artifact) -> Outputs {
        Outputs(self.0 & !a.bit())
    }

    pub fn has(self, a: Artifact) -> bool {
        self.0 & a.bit() != 0
    }

    /// `--out`'s value against `defaults`; `None` (no `--out`) keeps them. A
    /// list made only of `+x`/`-x` items adjusts the defaults; any plain item
    /// starts from nothing instead (and `+x` then just means `x`).
    pub fn parse(arg: Option<&str>, defaults: Outputs) -> anyhow::Result<Outputs> {
        let Some(arg) = arg else { return Ok(defaults) };
        let items: Vec<&str> = arg.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if items.is_empty() {
            anyhow::bail!("--out needs a list, e.g. --out scene,renders or --out +specs");
        }
        let adjust = items.iter().all(|s| s.starts_with('+') || s.starts_with('-'));
        let mut out = if adjust { defaults } else { Outputs::NONE };
        for item in items {
            let (remove, name) = match item.as_bytes()[0] {
                b'-' => (true, &item[1..]),
                b'+' => (false, &item[1..]),
                _ => (false, item),
            };
            let bits = match name {
                "all" => Outputs::of(&Artifact::ALL).0,
                "none" => 0,
                _ => Artifact::parse(name)?.bit(),
            };
            if remove {
                out.0 &= !bits;
            } else if name == "none" {
                out.0 = 0;
            } else {
                out.0 |= bits;
            }
        }
        Ok(out)
    }

    /// The artifact names in this set, for a log line.
    pub fn names(self) -> String {
        let names: Vec<&str> = Artifact::ALL.iter().filter(|a| self.has(**a)).map(|a| a.name()).collect();
        if names.is_empty() { "nothing".into() } else { names.join(", ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Artifact::*;

    #[test]
    fn out_lists_replace_and_signs_adjust_the_defaults() {
        let d = Outputs::settlement_default();
        assert_eq!(Outputs::parse(None, d).unwrap(), d);
        assert_eq!(Outputs::parse(Some("scene,renders"), d).unwrap(), Outputs::of(&[Scene, Renders]));
        assert_eq!(Outputs::parse(Some("+specs"), d).unwrap(), Outputs::of(&[Snapshot, Scene, Renders, Metrics, Specs]));
        assert_eq!(Outputs::parse(Some("-renders,-fixture"), d).unwrap(), Outputs::of(&[Scene, Metrics]));
        assert_eq!(Outputs::parse(Some("none"), d).unwrap(), Outputs::NONE);
        assert_eq!(Outputs::parse(Some("all,-viewer"), d).unwrap(), Outputs::of(&[Snapshot, Scene, Renders, Metrics, Specs]));
        let err = Outputs::parse(Some("scene,pngs"), d).unwrap_err().to_string();
        assert!(err.contains("pngs") && err.contains("renders"), "{err}");
        assert!(Outputs::parse(Some(""), d).is_err());
    }
}
