//! RFC-0104 D9 (phase-485 W4) — the drift check: what a build resolved,
//! against the pin set, kind by kind.
//!
//! One check replaces three mechanisms: the `scripts/bin/cargo` PATH shim's
//! leaf role, `check-leaf-lockfiles`'s tracked-lock invariant, and the client
//! side of the submodule-pin checks. It is a SET comparison after resolution,
//! so there is no way of invoking cargo that goes round it — which is the class
//! of every enforcement leak the shim had (issues 0384, 0386, 0676, 0873,
//! 1307, 1356, 1358).
//!
//! Three kinds are compared:
//!
//! * **cargo** — each registry entry of a cargo root's `Cargo.lock` must be in
//!   the pin set AT ITS VERSION. A different version in a pinned range, or a
//!   range the pin set does not hold at all, is drift.
//! * **source** — a vendored tree read from the CHECKOUT or the STORE must be at
//!   its pin (the index `ref`, which `check-source-refs` holds equal to the
//!   gitlink). A tree from an explicit rung (`env`, `arg`, `project`,
//!   `local-edit`) is the user's deliberate choice, so it is reported, never
//!   refused — RFC-0103 D5 is what makes those rungs outrank the pin.
//! * **tool** — a `[tool.*]` the project lock records at a version or sha256
//!   the index no longer names.

use std::{collections::BTreeMap, path::Path};

use crate::orchestration::registry_pins::{RegistryPkg, compat_range, parse_registry};

/// One way a build disagreed with the pin set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drift {
    /// A registry crate at a version the pin set does not hold.
    Crate {
        root: String,
        name: String,
        have: String,
        pinned: Option<String>,
    },
    /// A checkout/store source tree at another commit than its pin.
    Source {
        name: String,
        have: String,
        pinned: String,
    },
    /// A recorded tool whose version or sha256 the index no longer names.
    Tool {
        name: String,
        have: String,
        pinned: String,
    },
    /// Outside a checkout, a project's OWN lock pinned `name` at `pinned`, and
    /// the SDK it now builds against has `now` (phase-485 W10: an entry the
    /// project pinned is never rewritten without `nros update`).
    Pinned {
        kind: &'static str,
        name: String,
        pinned: String,
        now: String,
    },
}

impl std::fmt::Display for Drift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Drift::Crate {
                root,
                name,
                have,
                pinned: Some(p),
            } => write!(
                f,
                "{root}: `{name}` {have}, pinned at {p} — `nros update --crate {name}` moves \
                 the pin, or `nros update --crate {name} --precise {p}` here moves this lock back"
            ),
            Drift::Crate {
                root,
                name,
                have,
                pinned: None,
            } => write!(
                f,
                "{root}: `{name}` {have} is in no pinned range — `nros update --crate {name}` \
                 from this leaf adds it to the pin set"
            ),
            Drift::Source { name, have, pinned } => write!(
                f,
                "source `{name}` is at {have}, pinned at {pinned} — `git submodule update` \
                 restores the pin; `nros update --source {name} --ref {have}` moves it"
            ),
            Drift::Pinned {
                kind,
                name,
                pinned,
                now,
            } => write!(
                f,
                "{kind} `{name}`: this project's nros-sdk.lock pins {pinned}, the SDK now has \
                 {now} — `nros update --{kind} {name}` accepts the SDK's, or build against the \
                 SDK the lock was written with"
            ),
            Drift::Tool { name, have, pinned } => write!(
                f,
                "tool `{name}` is recorded as {have}, the index pins {pinned} — `nros setup` \
                 installs the pin; `nros update --tool {name}` moves it"
            ),
        }
    }
}

/// Registry entries of `lock` that the pin set does not hold at that version.
#[must_use]
pub fn crate_drift(root: &str, lock: &[RegistryPkg], pins: &[RegistryPkg]) -> Vec<Drift> {
    let pinned: BTreeMap<(String, String), &str> = pins
        .iter()
        .map(|p| {
            (
                (p.name.clone(), compat_range(&p.version)),
                p.version.as_str(),
            )
        })
        .collect();
    lock.iter()
        .filter_map(|p| {
            let want = pinned
                .get(&(p.name.clone(), compat_range(&p.version)))
                .copied();
            (want != Some(p.version.as_str())).then(|| Drift::Crate {
                root: root.to_string(),
                name: p.name.clone(),
                have: p.version.clone(),
                pinned: want.map(str::to_string),
            })
        })
        .collect()
}

/// The cargo kind for one root: its `Cargo.lock` against the pin file.
/// A root with no lock, or a tree with no pin set, has nothing to compare.
pub fn check_cargo_root(dir: &Path, label: &str, pins_file: &Path) -> eyre::Result<Vec<Drift>> {
    let (Ok(lock), Ok(pins)) = (
        std::fs::read_to_string(dir.join("Cargo.lock")),
        std::fs::read_to_string(pins_file),
    ) else {
        return Ok(Vec::new());
    };
    let pins = crate::orchestration::registry_pins::effective(
        &parse_registry(&pins)?,
        &crate::orchestration::registry_pins::project_overrides(dir)?,
    );
    Ok(crate_drift(label, &parse_registry(&lock)?, &pins))
}

/// The source kind: `rung` decides whether a moved tree is drift at all.
#[must_use]
pub fn source_drift(name: &str, rung: &str, have: &str, pinned: Option<&str>) -> Option<Drift> {
    let pinned = pinned?;
    if !matches!(rung, "checkout" | "store") || have == pinned || have.is_empty() {
        return None;
    }
    Some(Drift::Source {
        name: name.to_string(),
        have: have.to_string(),
        pinned: pinned.to_string(),
    })
}

/// Render a refusal: every drift, then what the check is.
#[must_use]
pub fn report(drifts: &[Drift]) -> String {
    let mut s = format!(
        "{} dependency drift(s) from the pin set (RFC-0104 D9):\n",
        drifts.len()
    );
    for d in drifts {
        s.push_str(&format!("  {d}\n"));
    }
    s.push_str(
        "A build resolves only what the pin set names, so a reviewed diff is the only way a \
         pin moves.",
    );
    s
}

/// The tool kind: each `[tool.*]` the project lock records, against the index.
#[must_use]
pub fn tool_drift(
    lock: &crate::orchestration::sdk_store::SdkLock,
    index: &crate::orchestration::sdk_index::SdkIndex,
    host: &str,
) -> Vec<Drift> {
    let mut out = Vec::new();
    for (name, e) in &lock.tool {
        let Some(t) = index.tool.get(name) else {
            continue;
        };
        if t.version != e.version {
            out.push(Drift::Tool {
                name: name.clone(),
                have: e.version.clone(),
                pinned: t.version.clone(),
            });
        } else if let (Some(have), Some(d)) = (&e.sha256, t.dist_for(host))
            && !have.eq_ignore_ascii_case(&d.sha256)
        {
            out.push(Drift::Tool {
                name: name.clone(),
                have: format!("{} (sha256 {have})", e.version),
                pinned: format!("{} (sha256 {})", t.version, d.sha256),
            });
        }
    }
    out
}

/// One crate to move in a lock: `name` from `have` onto `want`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Realign {
    pub name: String,
    pub have: String,
    pub want: String,
}

/// Move every crate in `moves` with `try_move`, repeating rounds until one
/// moves nothing (issue 1924). One pass in lock order is not enough: crates
/// released in lockstep (`futures-core`, `futures-util`, ...) require each
/// other at their own version, so `futures-core` 0.3.34 cannot go back to
/// 0.3.32 while `futures-util` 0.3.34 still needs it, and `futures-util` sorts
/// after it. Returns how many moved, and each crate that never did with the
/// last refusal.
pub fn realign<F>(moves: Vec<Realign>, mut try_move: F) -> (usize, Vec<(Realign, String)>)
where
    F: FnMut(&Realign) -> Result<(), String>,
{
    let mut pending: Vec<(Realign, String)> =
        moves.into_iter().map(|m| (m, String::new())).collect();
    let mut moved = 0usize;
    loop {
        let before = moved;
        let mut next = Vec::new();
        for (m, _) in pending {
            match try_move(&m) {
                Ok(()) => moved += 1,
                Err(why) => next.push((m, why)),
            }
        }
        pending = next;
        if pending.is_empty() || moved == before {
            return (moved, pending);
        }
    }
}

/// `cargo update -p <name>@<have> --precise <want>` in `dir`, for [`realign`].
pub fn cargo_precise(dir: &Path) -> impl FnMut(&Realign) -> Result<(), String> + '_ {
    move |m| {
        let out = std::process::Command::new("cargo")
            .current_dir(dir)
            .args([
                "update",
                "-q",
                "-p",
                &format!("{}@{}", m.name, m.have),
                "--precise",
                &m.want,
            ])
            .output()
            .map_err(|e| format!("run `cargo update`: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

/// The cargo kind for one root, made to hold: in a CHECKOUT a leaf lock is
/// seeded build output rather than a promise, so a crate merely off its pin is
/// moved back onto it (`cargo update --precise`) and said so. What cannot be
/// realigned — a crate in no pinned range, or a manifest the pin does not
/// satisfy — is returned as drift. Outside a checkout nothing is moved.
pub fn enforce_cargo_root(
    dir: &Path,
    label: &str,
    pins_file: &Path,
    in_checkout: bool,
) -> eyre::Result<Vec<Drift>> {
    let drifts = check_cargo_root(dir, label, pins_file)?;
    if !in_checkout || drifts.is_empty() {
        return Ok(drifts);
    }
    let mut left = Vec::new();
    let mut moves = Vec::new();
    for d in drifts {
        match &d {
            Drift::Crate {
                name,
                have,
                pinned: Some(p),
                ..
            } => moves.push(Realign {
                name: name.clone(),
                have: have.clone(),
                want: p.clone(),
            }),
            _ => left.push(d),
        }
    }
    let (moved, _stuck) = realign(moves, cargo_precise(dir));
    if moved > 0 {
        eprintln!("nros build:   realigned {moved} crate(s) in {label} to the pin set");
    }
    // A realignment can move a dependency in turn; what is still off is drift.
    let again = check_cargo_root(dir, label, pins_file)?;
    left.retain(|d| again.contains(d));
    for d in again {
        if !left.contains(&d) {
            left.push(d);
        }
    }
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lockstep_family_realigns_whatever_order_the_lock_lists_it_in() {
        // futures-core can move only once futures-util has: the shape of the
        // async leaves in issue 1924.
        let m = |n: &str| Realign {
            name: n.into(),
            have: "0.3.34".into(),
            want: "0.3.32".into(),
        };
        let mut done = std::collections::BTreeSet::new();
        let (moved, stuck) = realign(vec![m("futures-core"), m("futures-util")], |r| {
            if r.name == "futures-core" && !done.contains("futures-util") {
                return Err("required by futures-util 0.3.34".into());
            }
            done.insert(r.name.clone());
            Ok(())
        });
        assert_eq!((moved, stuck.len()), (2, 0));
    }

    #[test]
    fn a_crate_nothing_lets_move_is_returned_with_its_refusal() {
        let m = Realign {
            name: "futures-core".into(),
            have: "0.3.34".into(),
            want: "0.3.32".into(),
        };
        let (moved, stuck) = realign(vec![m.clone()], |_| Err("no".into()));
        assert_eq!(moved, 0);
        assert_eq!(stuck, vec![(m, "no".to_string())]);
    }

    fn pkg(name: &str, version: &str) -> RegistryPkg {
        RegistryPkg {
            name: name.into(),
            version: version.into(),
            source: "registry+https://github.com/rust-lang/crates.io-index".into(),
            checksum: None,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn a_crate_off_its_pin_or_outside_every_range_is_drift() {
        let pins = vec![pkg("heapless", "0.8.0"), pkg("libc", "0.2.186")];
        let lock = vec![
            pkg("heapless", "0.8.0"),
            pkg("libc", "0.2.190"),
            pkg("heapless", "0.9.3"),
        ];
        let d = crate_drift("leaf", &lock, &pins);
        assert_eq!(
            d,
            vec![
                Drift::Crate {
                    root: "leaf".into(),
                    name: "libc".into(),
                    have: "0.2.190".into(),
                    pinned: Some("0.2.186".into())
                },
                Drift::Crate {
                    root: "leaf".into(),
                    name: "heapless".into(),
                    have: "0.9.3".into(),
                    pinned: None
                },
            ]
        );
        assert!(crate_drift("leaf", &pins, &pins).is_empty());
    }

    #[test]
    fn the_drift_check_reads_the_projects_overrides() {
        let t = tempfile::tempdir().unwrap();
        let reg = "registry+https://github.com/rust-lang/crates.io-index";
        let lock = |v: &str| {
            format!(
                "version = 4\n\n[[package]]\nname = \"libc\"\nversion = \"{v}\"\nsource = \"{reg}\"\n"
            )
        };
        std::fs::write(t.path().join("pins.lock"), lock("0.2.186")).unwrap();
        std::fs::write(t.path().join("Cargo.lock"), lock("0.2.190")).unwrap();
        let pins = t.path().join("pins.lock");
        assert_eq!(
            check_cargo_root(t.path(), "p", &pins).unwrap().len(),
            1,
            "off the SDK pin"
        );
        std::fs::write(
            t.path().join("nros-sdk.lock"),
            format!("[cargo_override.libc]\nversion = \"0.2.190\"\nsource = \"{reg}\"\n"),
        )
        .unwrap();
        assert!(
            check_cargo_root(t.path(), "p", &pins).unwrap().is_empty(),
            "the project's own override is its pin"
        );
    }

    #[test]
    fn only_a_pinned_rung_can_drift() {
        assert!(source_drift("zp", "checkout", "b", Some("a")).is_some());
        assert!(source_drift("zp", "store", "b", Some("a")).is_some());
        for deliberate in ["env", "arg", "project", "local-edit"] {
            assert!(source_drift("zp", deliberate, "b", Some("a")).is_none());
        }
        assert!(source_drift("zp", "checkout", "a", Some("a")).is_none());
        assert!(source_drift("zp", "checkout", "b", None).is_none());
    }
}
