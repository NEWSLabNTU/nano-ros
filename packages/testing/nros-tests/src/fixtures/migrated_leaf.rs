//! Phase-531 W6 (RFC-0121 D8) — which `[[workspace_fixture]]` row answers for
//! a leaf that `nros dev migrate-bringup --fixtures` turned into a workspace.
//!
//! A migrated leaf keeps its directory: `examples/linux/rust/talker/` becomes a
//! single-node workspace (`src/<pkg>/`, `src/<pkg>_bringup/`) at the same
//! path, and every `[[fixture]]` row naming it becomes a
//! `[[workspace_fixture]]` row with the same `dir`, `platform`, `lang` and
//! `rmw`, so `row_coord()` and lane membership do not move. The resolvers that
//! ask for "the talker leaf built with xrce" keep asking that; this module is
//! the one place that turns the question into a row. A resolver must not
//! re-derive it per call site, because every batch of W6 moves more leaves and
//! a per-site fix would be one more spelling per batch.
//!
//! The rule is a fallback, taken only when the leaf has NO `[[fixture]]` row
//! left. A leaf that still has one answers for itself, exactly as before, so a
//! partly migrated leaf is never resolved through two tables at once.
//!
//! # Selecting the row
//!
//! The caller names the RMW (the row's coordinate) and a [`LeafVariant`]:
//!
//! - [`LeafVariant::Plain`] — no `env`. When several images of one RMW have
//!   none (a `linux` and a `linux_tls`, say), the bringup's
//!   `[system] default_images` decides, because that is what "the plain build"
//!   meant for the leaf too: the image a bare `nros build` builds.
//! - [`LeafVariant::Env`] — the row with exactly this `env`.
//! - [`LeafVariant::Image`] — the row building this image.
//!
//! None, or more than one, is an error naming the candidates. A wrong row is
//! not a missing file: it is a real binary built differently, and the test
//! would run against it and pass.

use crate::{TestError, TestResult, project_root};

/// One `[[workspace_fixture]]` row at a leaf directory: the fields selection
/// reads, joined from the lane export (coordinate) and the workspace record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafRow {
    pub id: String,
    pub lang: String,
    /// The row's coordinate RMW (`row_coord`).
    pub rmw: String,
    /// Relative to the leaf directory.
    pub bringup: String,
    /// The entry package, i.e. the binary name (`entry_name` in the manifest
    /// reader: authored, or `<image>_entry`).
    pub entry: String,
    /// Empty for an owned (hand-written) entry.
    pub image: String,
    /// `k=v`, sorted, comma-joined.
    pub env: String,
    /// The road the row builds on, `cargo` or `cmake`: the row's authored
    /// `builder` (the record's 15th column, phase-535 W6), else its language's
    /// (`rust` is cargo, everything else cmake) — the rule
    /// `workspace-fixtures-build.sh` builds it by.
    pub builder: String,
}

impl LeafRow {
    /// Does this row's image build an OWNED entry (RFC-0071 D19), the
    /// package's own hand-written `main`, rather than the generated
    /// `<image>_entry`? The one spelling of that test: the binary locator and
    /// [`owns_its_entry`] both ask it.
    pub fn builds_owned_entry(&self) -> bool {
        self.entry != format!("{}_entry", self.image.replace(['-', '.', '/'], "_"))
    }

    /// Does this row build with cargo? A C/C++ row does when its owned entry
    /// is a cargo package (the mps2-an385 bare-metal C/C++ examples).
    pub fn builds_with_cargo(&self) -> bool {
        self.builder == "cargo"
    }
}

/// Does this migrated leaf build `rmw` as its OWN program (an owned entry)?
/// Such a leaf answers for itself even where its case name is also a pair
/// role a language workspace builds (`linux/cpp/service-client`): its
/// hand-written `main` is what its callers ask for, and the workspace pair
/// image is a different program. A row lookup that fails answers `true`, so
/// the leaf's own selection reports the error instead of a different binary
/// answering quietly.
pub fn owns_its_entry(dir: &str, rmw: &str) -> bool {
    is_migrated(dir)
        && rows_at(dir).map_or(true, |rows| {
            rows.iter().any(|r| r.rmw == rmw && r.builds_owned_entry())
        })
}

/// Which row of a migrated leaf a caller means. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafVariant<'a> {
    Plain,
    Env(&'a [(&'a str, &'a str)]),
    Image(&'a str),
}

fn normalise_env(pairs: impl IntoIterator<Item = String>) -> String {
    let mut v: Vec<String> = pairs.into_iter().filter(|s| !s.is_empty()).collect();
    v.sort();
    v.join(",")
}

/// Does this leaf answer through its workspace rows? True when it has at least
/// one `[[workspace_fixture]]` row and no `[[fixture]]` row.
pub fn is_migrated(dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    let rows = crate::fixtures::lane::manifest_rows();
    !rows.iter().any(|r| r.kind == "fixture" && r.dir == dir)
        && rows
            .iter()
            .any(|r| r.kind == "workspace_fixture" && r.dir == dir)
}

/// Every `[[workspace_fixture]]` row whose `dir` is this leaf.
pub fn rows_at(dir: &str) -> TestResult<Vec<LeafRow>> {
    let dir = dir.trim_end_matches('/');
    crate::fixtures::lane::manifest_rows()
        .iter()
        .filter(|r| r.kind == "workspace_fixture" && r.dir == dir)
        .map(|r| {
            let record = crate::fixtures::current_workspace_fixture_record(&r.id)?;
            let f: Vec<&str> = record.split('\x1f').collect();
            let field = |i: usize| f.get(i).copied().unwrap_or("").to_string();
            Ok(LeafRow {
                id: r.id.clone(),
                lang: field(1),
                rmw: r.coord.2.clone(),
                bringup: field(3).trim_end_matches('/').to_string(),
                entry: field(4),
                image: field(13),
                env: normalise_env(field(9).split_whitespace().map(str::to_string)),
                builder: road(&field(1), &field(14)),
            })
        })
        .collect()
}

/// The road a workspace record builds on: its authored `builder`, else the
/// language default (see [`LeafRow::builder`]).
fn road(lang: &str, builder: &str) -> String {
    if !builder.is_empty() {
        builder.to_string()
    } else if lang == "rust" {
        "cargo".to_string()
    } else {
        "cmake".to_string()
    }
}

/// `[system] default_images` of the bringup a row builds, read from the
/// checkout. Empty when the file or the key is absent.
pub fn default_images(dir: &str, row: &LeafRow) -> Vec<String> {
    let path = project_root()
        .join(dir.trim_end_matches('/'))
        .join(&row.bringup)
        .join("system.toml");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(doc) = toml::from_str::<toml::Table>(&text) else {
        return Vec::new();
    };
    doc.get("system")
        .and_then(|s| s.get("default_images"))
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Does a row's (normalised) `env` leave the image unconfigured? Only a
/// toolchain pin does: `RUSTUP_TOOLCHAIN` picks the compiler that builds the
/// image (a NuttX row's nightly for `build-std`), not a variant of it, so a row
/// whose env is only that is still the plain build.
fn configures_nothing(env: &str) -> bool {
    env.split(',')
        .filter(|kv| !kv.is_empty())
        .all(|kv| kv.starts_with("RUSTUP_TOOLCHAIN="))
}

/// The one row of `rows` (all at `dir`) that builds `rmw` as `variant`.
/// `defaults` answers `[system] default_images` for a row's bringup.
pub fn select_in<'r>(
    rows: &'r [LeafRow],
    dir: &str,
    rmw: &str,
    variant: LeafVariant<'_>,
    defaults: &dyn Fn(&LeafRow) -> Vec<String>,
) -> TestResult<&'r LeafRow> {
    let of_rmw = rows.iter().filter(|r| r.rmw == rmw);
    let mut hits: Vec<&LeafRow> = match variant {
        LeafVariant::Image(id) => of_rmw.filter(|r| r.image == id).collect(),
        LeafVariant::Env(pairs) => {
            let want = normalise_env(pairs.iter().map(|(k, v)| format!("{k}={v}")));
            of_rmw.filter(|r| r.env == want).collect()
        }
        LeafVariant::Plain => of_rmw.filter(|r| configures_nothing(&r.env)).collect(),
    };
    if variant == LeafVariant::Plain && hits.len() > 1 {
        let narrowed: Vec<&LeafRow> = hits
            .iter()
            .copied()
            .filter(|r| defaults(r).contains(&r.image))
            .collect();
        if narrowed.len() == 1 {
            hits = narrowed;
        }
    }
    match hits.as_slice() {
        [row] => Ok(row),
        none_or_many => Err(TestError::BuildFailed(format!(
            "{dir} (migrated to a workspace, phase-531 W6): {} of its \
             [[workspace_fixture]] rows build {rmw} as {variant:?} (want exactly 1). \
             Rows: {}",
            none_or_many.len(),
            rows.iter()
                .map(|r| format!(
                    "{} (rmw {}, image {:?}, env {:?})",
                    r.id, r.rmw, r.image, r.env
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ))),
    }
}

/// [`select_in`] over the shipped manifest, or `None` when `dir` is not a
/// migrated leaf ([`is_migrated`]).
pub fn select(dir: &str, rmw: &str, variant: LeafVariant<'_>) -> Option<TestResult<LeafRow>> {
    if !is_migrated(dir) {
        return None;
    }
    Some(
        rows_at(dir).and_then(|rows| {
            select_in(&rows, dir, rmw, variant, &|r| default_images(dir, r)).cloned()
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toolchain_pin_alone_is_still_the_plain_build() {
        let rows = [row(
            "pinned",
            "zenoh",
            "nuttx",
            "RUSTUP_TOOLCHAIN=nightly-2026-04-11",
        )];
        let hit = select_in(&rows, "d", "zenoh", LeafVariant::Plain, &|_| Vec::new()).unwrap();
        assert_eq!(hit.id, "pinned");
        let rows = [row(
            "configured",
            "zenoh",
            "nuttx",
            "NROS_ZENOH_CONNECT_FOREVER=1,RUSTUP_TOOLCHAIN=nightly-2026-04-11",
        )];
        assert!(select_in(&rows, "d", "zenoh", LeafVariant::Plain, &|_| Vec::new()).is_err());
    }

    fn row(id: &str, rmw: &str, image: &str, env: &str) -> LeafRow {
        LeafRow {
            id: id.into(),
            lang: "rust".into(),
            rmw: rmw.into(),
            bringup: "src/talker_bringup".into(),
            entry: nros_orchestration_ir::entry_names::entry_package_name(image),
            image: image.into(),
            env: env.into(),
            builder: "cargo".into(),
        }
    }

    fn talker() -> Vec<LeafRow> {
        vec![
            row("t", "zenoh", "linux", ""),
            row("t-tls", "zenoh", "linux_tls", ""),
            row("t-peer", "zenoh", "linux", "ZPICO_MULTICAST_TRANSPORT=1"),
            row("t-xrce", "xrce", "linux_xrce", ""),
        ]
    }

    const DIR: &str = "examples/linux/rust/talker";

    fn pick(rmw: &str, v: LeafVariant<'_>, defaults: &[&str]) -> TestResult<String> {
        let rows = talker();
        let d: Vec<String> = defaults.iter().map(|s| s.to_string()).collect();
        select_in(&rows, DIR, rmw, v, &|_| d.clone()).map(|r| r.id.clone())
    }

    #[test]
    fn the_rmw_picks_the_row() {
        assert_eq!(pick("xrce", LeafVariant::Plain, &[]).unwrap(), "t-xrce");
    }

    #[test]
    fn plain_ties_are_broken_by_default_images_and_only_by_them() {
        assert_eq!(pick("zenoh", LeafVariant::Plain, &["linux"]).unwrap(), "t");
        assert!(
            pick("zenoh", LeafVariant::Plain, &[]).is_err(),
            "two env-free zenoh images and no default must fail closed"
        );
    }

    #[test]
    fn env_and_image_variants_select_exactly() {
        let env = [("ZPICO_MULTICAST_TRANSPORT", "1")];
        assert_eq!(
            pick("zenoh", LeafVariant::Env(&env), &[]).unwrap(),
            "t-peer"
        );
        assert_eq!(
            pick("zenoh", LeafVariant::Image("linux_tls"), &[]).unwrap(),
            "t-tls"
        );
    }

    #[test]
    fn an_unbuilt_configuration_is_an_error_not_a_guess() {
        assert!(pick("cyclonedds", LeafVariant::Plain, &["linux"]).is_err());
        assert!(pick("xrce", LeafVariant::Image("linux"), &[]).is_err());
        assert!(pick("zenoh", LeafVariant::Env(&[("X", "1")]), &[]).is_err());
    }

    #[test]
    fn an_unmigrated_leaf_is_not_this_modules_answer() {
        // A leaf that still has a [[fixture]] row answers for itself; a dir
        // with no row at all is no migrated leaf either.
        assert!(select("examples/no/such/leaf", "zenoh", LeafVariant::Plain).is_none());
    }
}
