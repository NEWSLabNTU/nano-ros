//! Shared cargo `--target-dir` group resolution — phase-340 B2.
//!
//! # The problem
//!
//! Phase 226.D gave compatible Rust fixture rows ONE `--target-dir` per group,
//! so the shared nano-ros crates compile once for a group instead of once per
//! example dir. Measured, the mechanism is worth 46.08 GiB -> ~6.95 GiB on
//! `linux` alone. It has been deployed on exactly ONE platform since, because
//! the test resolver could not follow the build.
//!
//! The blocker was never the group MECHANISM; it was the KEY. A group's members
//! share a flat `<group>/[<triple>/]<profile>/` artifact namespace and cargo
//! does not hash the final artifact name, so the key has to separate feature /
//! env variants of one package. Phase-340 W1 measured what happens without
//! that: `examples/native/rust/talker`'s four rows (default, `rmw-zenoh`,
//! `rmw-xrce`, `link-tls`) are four DIFFERENT binaries — 8616504 / 8616504 /
//! 6514392 / 9034536 bytes, four distinct sha256 — and N sequential cargo
//! invocations into one target dir keep both identities in `deps/` while
//! SILENTLY replacing the flat artifact. No warning fires (cargo's `output
//! filename collision` diagnostic only fires when ONE invocation builds both).
//! So the coarse platform-grained key is refuted, and the variant-grained slug
//! is the only path.
//!
//! # Why the variant cannot come from the call site
//!
//! `build_example("native/rust/talker", "talker", …)` and
//! `build_example_rmw(…, Rmw::Zenoh)` — the two funnels — distinguish variants
//! only by the authored dir STRING (`target/` vs `target-zenoh/`), and that
//! string is exactly what a group strips
//! (`nros_fixture_strip_authored_target_dir`). The variant therefore has to
//! come from the manifest ROW.
//!
//! # Why the slug cannot be re-derived here
//!
//! It is a `cksum` over the row's variant signature, owned by
//! `nros_fixture_group_slug` in `scripts/build/fixtures-target-dir.sh` — the
//! function the fixture BUILD and the staleness PROBE both call. A second
//! spelling of a checksum in Rust is the R3 drift phase-340 keeps deleting (a
//! private `project_root()` in `qemu.rs` was removed for exactly that reason).
//!
//! # The join, and where it lives
//!
//! `fixtures-manifest.py fixture-groups` pairs each cargo row's
//! `row_artifact_root()` with the slug the SHELL derives for it, and this
//! module inverts the first half the way [`crate::fixtures::lane`] already
//! does: **leaf artifact path -> manifest row -> group dir**. Manifest row and
//! shell key each keep exactly one computation; this is only the join. It is
//! the same shape, the same `row_artifact_root`, and the same longest-match /
//! component-wise containment rule as the lane narrowing, deliberately — those
//! properties are already gated by `tests/lane_run_narrowing.rs`.
//!
//! # The redirect is a PREFIX REWRITE, and that is what dissolves the triple
//!
//! `--target-dir` changes the ROOT of the artifact tree and nothing below it.
//! Cargo still writes `<root>/<triple>/<profile>/<bin>` when a target is in
//! effect and `<root>/<profile>/<bin>` when none is, and it decides that from
//! `--target` or the leaf's `.cargo/config.toml [build] target` — neither of
//! which the group touches. So the redirect replaces the leaf artifact ROOT and
//! carries the remaining components verbatim.
//!
//! That matters because the recorded sub-blocker was that
//! `require_shared_fixture_binary` hardcodes a `{triple}/` component while 0 of
//! 65 `linux` rows carry `--target` — one directory too deep for a host build.
//! A rewrite never synthesises the component: a `qemu-arm-baremetal` leaf path
//! has `thumbv7m-none-eabi/` in it (from that leaf's `.cargo/config.toml`) and
//! keeps it; a `linux` leaf path has none and does not gain one. No triple
//! table, no per-platform knowledge, nothing to keep in step.
//!
//! # Inertness
//!
//! Eligibility is still `NROS_FIXTURE_SHARED_PLATFORMS`, read by the SHELL and
//! reported per row in the export — this module has no mirror of it. With the
//! shipped default (`qemu-arm-baremetal`) every other platform's rows report
//! `shared = 0` and nothing is redirected, which is why B2 lands inert and B3
//! is a one-line change to that list plus a native-lane rebuild.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{TestError, TestResult, build_dir, fixtures::lane::path_under, project_root};

/// One buildable cargo `[[fixture]]` row, as `fixtures-manifest.py
/// fixture-groups` emits it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRow {
    /// Repo-relative dir the row's artifacts land in ABSENT any sharing —
    /// `row_artifact_root()`, the same value `coords` exports.
    pub artifact_root: String,
    pub platform: String,
    /// `nros_fixture_group_slug` for this row. Always populated: the shell
    /// separates "which group WOULD this row land in?" from "is it shared
    /// today?" and so does this table.
    pub slug: String,
    /// Whether the fixture BUILD actually redirects this row today
    /// (`nros_fixture_group` non-empty, i.e. the platform is in
    /// `NROS_FIXTURE_SHARED_PLATFORMS`).
    pub shared: bool,
    /// The row's leaf dir, repo-relative and without a trailing slash.
    pub dir: String,
    /// The row's AUTHORED variant identity — `row_selector` in
    /// `fixtures-manifest.py`. RAW: `rmw` is empty when the row authored none,
    /// NOT `row_coord`'s defaulted value. issue 0517.
    pub selector: Selector,
    /// The row's RESOLVED coordinate (`row_coord`), carried so a resolver that
    /// already selected the row can ask the lane about it directly rather than
    /// handing back a path for `attribute_path` to re-derive the row from.
    pub coord: crate::fixtures::lane::Coord,
}

/// A row's authored configuration, as [`FixtureVariant`] must name it to select
/// that row. Ordered/normalised by the exporter, never here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pub rmw: String,
    /// Sorted, comma-joined.
    pub features: String,
    pub no_default_features: bool,
    /// `k=v`, sorted by key, comma-joined.
    pub env: String,
}

static ROWS: OnceLock<Vec<GroupRow>> = OnceLock::new();

/// Every buildable cargo row's `(artifact_root, platform, slug, shared)`.
///
/// Shelling into the manifest reader is the point — see the module docs.
/// Measured 111 ms on the shipped manifest (122 cargo rows, 19 distinct slugs),
/// read at most once per test process and only when [`redirect`] is actually
/// reached.
pub fn manifest_rows() -> &'static [GroupRow] {
    ROWS.get_or_init(|| {
        let root = project_root();
        let out = std::process::Command::new("python3")
            .arg(root.join("scripts/build/fixtures-manifest.py"))
            .arg("fixture-groups")
            .current_dir(&root)
            .output();
        let out = match out {
            Ok(o) if o.status.success() => o,
            // A table we cannot read must NOT degrade to "redirect nothing".
            // That reads as "this row is not shared", so the resolver would
            // look in the leaf dir the build stopped writing to and report the
            // fixture missing — or, worse, find a pre-migration binary still
            // sitting there and run it. Panic naming the cause.
            Ok(o) => panic!(
                "fixtures-manifest.py fixture-groups failed ({}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            ),
            Err(e) => panic!("could not run fixtures-manifest.py fixture-groups: {e}"),
        };
        parse_rows(&String::from_utf8_lossy(&out.stdout))
    })
}

/// Parse the `fixture-groups` record stream. Split out so the shape assertions
/// are testable without a subprocess.
pub fn parse_rows(text: &str) -> Vec<GroupRow> {
    let mut rows = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\x1f').collect();
        assert_eq!(
            f.len(),
            12,
            "unexpected `fixture-groups` record shape (expected 12 \\x1f-separated \
             fields: artifact_root, platform, slug, shared, dir, rmw, features, \
             no_default_features, env, coord_platform, coord_lang, coord_rmw): {line:?}"
        );
        rows.push(GroupRow {
            artifact_root: f[0].to_string(),
            platform: f[1].to_string(),
            slug: f[2].to_string(),
            shared: matches!(f[3], "1"),
            dir: f[4].to_string(),
            selector: Selector {
                rmw: f[5].to_string(),
                features: f[6].to_string(),
                no_default_features: matches!(f[7], "1"),
                env: f[8].to_string(),
            },
            coord: (f[9].to_string(), f[10].to_string(), f[11].to_string()),
        });
    }
    rows
}

/// The SHARED row whose artifact root contains `rel`, plus the path components
/// below that root — the whole decision, minus the environment and the build
/// root.
///
/// Longest match, component-wise, exactly as
/// [`crate::fixtures::lane::attribute_path`]: a textual prefix would let
/// `…/talker/target` claim `…/talker/target-xrce`, which is a DIFFERENT row in
/// a DIFFERENT group — i.e. the artifact-overwrite failure this whole design
/// exists to avoid, reintroduced in the resolver instead of in cargo.
///
/// Rows with `shared = false` are skipped rather than matched-and-ignored: an
/// unmigrated platform must resolve to its leaf path unchanged.
///
/// An ambiguous longest match is `None` — issue 0517. Two shared rows at one
/// artifact root belong to DIFFERENT groups, and picking either would redirect
/// the caller onto a real binary built with different features. `None` here
/// means "no redirect", so the caller resolves the leaf path, finds nothing, and
/// fails loudly. That is the outcome to prefer; the same rule and the same
/// reasoning live in [`crate::fixtures::lane::attribute_path_in`].
pub fn attribute<'a>(rows: &'a [GroupRow], rel: &Path) -> Option<(&'a GroupRow, PathBuf)> {
    let mut best: Option<&'a GroupRow> = None;
    let mut ambiguous = false;
    for row in rows {
        if !row.shared || row.artifact_root.is_empty() {
            continue;
        }
        if !path_under(rel, Path::new(&row.artifact_root)) {
            continue;
        }
        match best {
            Some(b) if row.artifact_root.len() > b.artifact_root.len() => {
                best = Some(row);
                ambiguous = false;
            }
            Some(b) if row.artifact_root.len() == b.artifact_root.len() => {
                ambiguous |= row.slug != b.slug;
            }
            Some(_) => {}
            None => best = Some(row),
        }
    }
    if ambiguous {
        return None;
    }
    let row = best?;
    let suffix = rel.strip_prefix(&row.artifact_root).ok()?.to_path_buf();
    Some((row, suffix))
}

/// Where a leaf-local fixture artifact path ACTUALLY lands, or `None` when the
/// row's platform is not migrated (or the path belongs to no cargo row).
///
/// This module reads NOTHING from the environment. It had a
/// `sharing_possible()` short-circuit for one commit — "`NROS_FIXTURE_SHARED_
/// PLATFORMS` empty ⇒ no row can be shared, so skip the subprocess" — and
/// `tests/fixture_group_resolution.rs` immediately falsified it: the shell
/// spells the default `${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal}`,
/// and `:-` treats EMPTY as unset, so an empty value means the DEFAULT list, not
/// the empty one. A 4-line "cheap gate" was already a second, wrong copy of the
/// eligibility rule — the exact class the mirror it replaced belonged to.
///
/// The cost of having no gate is one `fixtures-manifest.py fixture-groups` (111
/// ms measured) per test process, paid lazily on the first fixture resolution.
/// If a sweep shows that mattering, the fix is a cheaper EXPORT, never a second
/// eligibility rule here.
pub fn redirect(leaf_path: &Path) -> Option<PathBuf> {
    let root = project_root();
    let rel = leaf_path.strip_prefix(&root).ok()?;
    let (row, suffix) = attribute(manifest_rows(), rel)?;
    Some(group_dir(&row.slug).join(suffix))
}

/// [`redirect`], falling back to the path as authored.
pub fn resolved(leaf_path: &Path) -> PathBuf {
    redirect(leaf_path).unwrap_or_else(|| leaf_path.to_path_buf())
}

/// WHICH of a leaf's `[[fixture]]` rows a caller means — issue 0517.
///
/// A leaf dir can carry several rows (`examples/native/rust/talker` has five),
/// and today the only thing separating them anywhere is the authored
/// `target_dir` string. That string is a directory somebody had to invent, it is
/// what a shared group strips before cargo sees it, and phase-340 W2.d wants it
/// deleted — so a resolver keyed on it is keyed on the wrong thing.
///
/// This names the row's CONFIGURATION instead, which the caller genuinely knows:
/// a test resolving "the ThreadX talker with zenoh selected by feature" is
/// naming something real, where `target-zenoh` was naming a side effect.
///
/// The four constructors are the four shapes the manifest actually contains —
/// see `row_selector` in `fixtures-manifest.py`, which counts them. They are
/// constructors rather than free-form fields so a caller cannot invent a fifth
/// shape that matches no row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureVariant(Selector);

impl FixtureVariant {
    /// A plain row: no authored `rmw`, no features, default features on (64
    /// rows). Note this is NOT "the default RMW" — a row whose RMW is baked by
    /// the platform authors the `rmw` key and needs [`Self::platform_rmw`].
    pub fn plain() -> Self {
        Self(Selector::default())
    }

    /// RMW chosen by cargo FEATURE: `rmw = "<x>"`, `features = ["rmw-<x>"]`,
    /// `no_default_features = true` (37 rows). The shape every
    /// `build_example_rmw` call site means.
    pub fn rmw(rmw: super::Rmw) -> Self {
        Self(Selector {
            rmw: rmw.cmake_value().to_string(),
            features: rmw.cargo_feature().to_string(),
            no_default_features: true,
            env: String::new(),
        })
    }

    /// RMW baked by the platform / board / Kconfig rather than by a feature:
    /// `rmw = "<x>"` and no features (144 rows).
    pub fn platform_rmw(rmw: super::Rmw) -> Self {
        Self(Selector {
            rmw: rmw.cmake_value().to_string(),
            ..Selector::default()
        })
    }

    /// A feature variant with no authored RMW — `link-tls`, `zero-copy`,
    /// `large-buf` (3 rows). Features are sorted here to match the exporter.
    pub fn features(features: &[&str]) -> Self {
        let mut f: Vec<&str> = features.to_vec();
        f.sort_unstable();
        Self(Selector {
            features: f.join(","),
            ..Selector::default()
        })
    }

    /// Add the row's authored `env`, as `("KEY", "value")` pairs. The one row
    /// that needs it is `stress-zenoh`'s large-buffer variant, which is
    /// otherwise identical to its plain sibling — the single collision in the
    /// selector before `env` was included.
    pub fn with_env(mut self, env: &[(&str, &str)]) -> Self {
        let mut e: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        e.sort();
        self.0.env = e.join(",");
        self
    }
}

/// Selects by leaf dir and configuration rather than by a hand-spelled path —
/// issue 0517.
///
/// # Fails closed, twice
///
/// No matching row, or more than one, is an error rather than a guess. A wrong
/// answer here is not a missing file — it is a real binary built with different
/// features, and the test would run against it and pass. `(dir, selector)` is
/// injective over the shipped manifest, so >1 match means a row was added whose
/// configuration no caller can name; the fix is to make the rows distinguishable,
/// not to relax this.
/// Where a `[[workspace_fixture]]` row's artifacts land — issue 0517, the
/// workspace half.
///
/// Workspace rows never reach `nros_fixture_target_dir_flag`:
/// `workspace-fixtures-build.sh` passes the row's authored `target_dir` /
/// `build_subdir` to cargo and cmake directly, so no group can redirect them and
/// the answer is always the row's own artifact root. That is `row_artifact_root`
/// — the same computation the lane narrowing and the build already use — reached
/// through the `id` these rows are attributed by anyway
/// ([`crate::fixtures::lane::attribute_workspace_id`]).
///
/// This exists so the ~8 workspace resolvers stop spelling `target-fixtures`,
/// `target-fixtures/nuttx-riscv`, `build-workspace-fixtures` by hand. Those
/// literals are the same defect as the plain-row `target-<rmw>` ones: a
/// directory standing in for a row's identity, which phase-340 W2.d is about to
/// stop authoring.
pub fn workspace_artifact_dir(fixture_id: &str) -> TestResult<PathBuf> {
    let row = crate::fixtures::lane::attribute_workspace_id(fixture_id).ok_or_else(|| {
        TestError::BuildFailed(format!(
            "no [[workspace_fixture]] row with id {fixture_id:?} in \
             examples/fixtures.toml — the manifest is the SSoT for where this \
             fixture's artifacts land, so an id it does not carry cannot be resolved."
        ))
    })?;
    Ok(project_root().join(&row.artifact_root))
}

/// Where the row's artifacts actually are, redirected BY ROW.
///
/// The leaf root when the platform does not share, the group dir when it does —
/// decided from the row's own `shared`/`slug` rather than by pattern-matching a
/// path. The path route (`resolved`) stays for the resolvers that still spell a
/// leaf `target/` inline; a caller that went through [`select_row`] has no
/// reason to round-trip through a path it would only have to be parsed back out
/// of.
pub fn row_resolved_dir(row: &GroupRow) -> PathBuf {
    if row.shared {
        group_dir(&row.slug)
    } else {
        project_root().join(&row.artifact_root)
    }
}

/// Does `examples/fixtures.toml` carry ANY row for this leaf?
///
/// The distinction [`row_artifact_dir`] cannot make on its own, and the two
/// cases deserve opposite answers:
///
/// * leaf HAS rows, none matching the variant → fail closed. The leaf is
///   manifest-managed and the caller named a configuration it does not build.
/// * leaf has NO rows → not this manifest's business. `px4/rust/companion/*` is
///   the live case: those fixtures are built by `just px4 build-fixtures`, which
///   is its own lane with its own SDK prerequisites, and nothing in
///   `fixtures.toml` mentions px4. Refusing there would turn a working resolver
///   into an error on every host that has the px4 SDK.
pub fn leaf_has_rows(dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    manifest_rows().iter().any(|r| r.dir == dir)
}

/// The row a caller means, by leaf dir and configuration — the selection half of
/// [`row_artifact_dir`], exposed so a resolver can also ask the lane about the
/// row it just selected (issue 0517 step 1).
pub fn select_row(dir: &str, variant: &FixtureVariant) -> TestResult<&'static GroupRow> {
    let dir = dir.trim_end_matches('/');
    let hits: Vec<&GroupRow> = manifest_rows()
        .iter()
        .filter(|r| r.dir == dir && r.selector == variant.0)
        .collect();
    match hits.as_slice() {
        [row] => Ok(row),
        [] => {
            let near: Vec<String> = manifest_rows()
                .iter()
                .filter(|r| r.dir == dir)
                .map(|r| format!("{:?}", r.selector))
                .collect();
            Err(TestError::BuildFailed(format!(
                "no [[fixture]] row for {dir} with {:?}.\n  rows at that dir: {}\n\
                 Either the variant is spelt differently in examples/fixtures.toml \
                 or the row does not exist.",
                variant.0,
                if near.is_empty() {
                    "<none>".to_string()
                } else {
                    near.join("; ")
                }
            )))
        }
        many => Err(TestError::BuildFailed(format!(
            "{} [[fixture]] rows at {dir} share the variant {:?} — no caller can \
             name one of them. Give them distinguishable configuration in \
             examples/fixtures.toml (issue 0517).",
            many.len(),
            variant.0
        ))),
    }
}

/// `build/fixtures-cargo/<slug>` — the shell's `nros_fixture_target_dir_flag`
/// spelt through [`build_dir`], so `NROS_BUILD_ROOT` moves both halves
/// (RFC-0070 R3). `tests/build_root_derivation.sh` asserts the two agree.
pub fn group_dir(slug: &str) -> PathBuf {
    build_dir("fixtures-cargo", &[slug])
}

/// The one shared group dir a platform produces, for resolvers that know a
/// platform and a binary name but no manifest row.
///
/// This is the replacement for `fixture_shared_target_dir(platform)`, which
/// hardcoded `build/fixtures-cargo/<platform>` and so could express only the
/// DEFAULT group. It now asks the export, and — critically — FAILS when the
/// platform produces more than one group instead of silently answering for the
/// default one. `check-fixture-groups`'s A2 arm made that same guarantee from
/// the outside; this is its twin on the inside, for the case the gate cannot
/// see (a caller that never names a row).
///
/// A platform with several groups is not a bug, it is the normal shape after a
/// migration — its resolvers just have to route through a leaf path, where the
/// row is known. `linux` will have seven.
pub fn sole_group_dir(platform: &str) -> TestResult<PathBuf> {
    let mut slugs: Vec<&str> = manifest_rows()
        .iter()
        .filter(|r| r.shared && r.platform == platform)
        .map(|r| r.slug.as_str())
        .collect();
    slugs.sort_unstable();
    slugs.dedup();
    match slugs.as_slice() {
        [] => Err(TestError::BuildFailed(format!(
            "platform {platform:?} is not migrated to a shared fixture target dir \
             (no row reports one; see NROS_FIXTURE_SHARED_PLATFORMS in \
             scripts/build/fixtures-target-dir.sh)"
        ))),
        [one] => Ok(group_dir(one)),
        many => Err(TestError::BuildFailed(format!(
            "platform {platform:?} produces {} shared groups ({}), so \"the\" group \
             dir is ambiguous. A resolver for this platform must derive the dir \
             from a leaf artifact path (which names the manifest row, and hence \
             the variant) — see fixtures::groups::redirect.",
            many.len(),
            many.join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic table: two variants of ONE leaf in a shared platform, plus an
    /// unmigrated row. Synthetic on purpose — the properties below must hold
    /// for the shape B3 creates, and today's manifest has exactly one migrated
    /// platform with exactly one group, so a test driven only by the real table
    /// would pass under a resolver that ignored the slug entirely.
    fn table() -> Vec<GroupRow> {
        vec![
            GroupRow {
                artifact_root: "examples/native/rust/talker/target".into(),
                platform: "linux".into(),
                slug: "linux".into(),
                shared: true,
                ..Default::default()
            },
            GroupRow {
                artifact_root: "examples/native/rust/talker/target-xrce".into(),
                platform: "linux".into(),
                slug: "linux-3000917972".into(),
                shared: true,
                ..Default::default()
            },
            GroupRow {
                artifact_root: "examples/freertos/rust/talker/target".into(),
                platform: "freertos".into(),
                slug: "freertos".into(),
                shared: false,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn two_variants_of_one_leaf_land_in_different_groups() {
        // The refutation of the platform-grained key, as an assertion. Under
        // it both of these resolve to `fixtures-cargo/linux/debug/talker` and
        // the second silently overwrote the first.
        let t = table();
        let (row, suffix) = attribute(
            &t,
            // profile-literal-ok: dir vocabulary: synthetic leaf paths exercising the
            // artifact-root rewrite; no build is invoked from this test data
            Path::new("examples/native/rust/talker/target/debug/talker"),
        )
        .unwrap();
        assert_eq!(row.slug, "linux");
        assert_eq!(suffix, Path::new("debug/talker"));

        let (row, suffix) = attribute(
            &t,
            // profile-literal-ok: dir vocabulary: synthetic leaf paths exercising the
            // artifact-root rewrite; no build is invoked from this test data
            Path::new("examples/native/rust/talker/target-xrce/debug/talker"),
        )
        .unwrap();
        assert_eq!(
            row.slug, "linux-3000917972",
            "`target-xrce` must not be swallowed by the `target` row: they are \
             different binaries at one artifact name"
        );
        assert_eq!(suffix, Path::new("debug/talker"));
    }

    #[test]
    fn an_unmigrated_platform_is_never_redirected() {
        // Inertness, as a property rather than as a comment: B2 changes where
        // NOTHING resolves until B3 flips the eligibility list.
        assert!(
            attribute(
                &table(),
                // profile-literal-ok: dir vocabulary: synthetic leaf paths exercising the
                // artifact-root rewrite; no build is invoked from this test data
                Path::new("examples/freertos/rust/talker/target/debug/talker")
            )
            .is_none()
        );
    }

    #[test]
    fn the_triple_component_is_carried_never_synthesised() {
        // The recorded sub-blocker, both directions. A cross row's leaf path
        // carries its triple (from `--target` or the leaf's `.cargo/config.toml
        // [build] target`) and keeps it; a host row has none and must not gain
        // one, which is the "one directory too deep" bug for all 65 `linux`
        // rows.
        let t = vec![GroupRow {
            artifact_root: "examples/qemu-arm-baremetal/rust/talker/target".into(),
            platform: "qemu-arm-baremetal".into(),
            slug: "qemu-arm-baremetal".into(),
            shared: true,
            ..Default::default()
        }];
        let (_, suffix) = attribute(
            &t,
            // profile-literal-ok: dir vocabulary: synthetic leaf paths exercising the
            // artifact-root rewrite; no build is invoked from this test data
            Path::new("examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/debug/t"),
        )
        .unwrap();
        assert_eq!(suffix, Path::new("thumbv7m-none-eabi/debug/t"));

        let (_, suffix) = attribute(
            &table(),
            // profile-literal-ok: dir vocabulary: synthetic leaf paths exercising the
            // artifact-root rewrite; no build is invoked from this test data
            Path::new("examples/native/rust/talker/target/debug/talker"),
        )
        .unwrap();
        assert_eq!(
            suffix,
            Path::new("debug/talker"),
            "a host row must not acquire a triple component"
        );
    }

    #[test]
    fn the_real_manifest_table_parses_and_agrees_with_the_shell() {
        // The live table, end to end: the export must be readable, non-empty,
        // and every row must carry a slug that starts with its platform (the
        // shape `nros_fixture_group_slug` guarantees: `<platform>` or
        // `<platform>-<cksum>`).
        let rows = manifest_rows();
        assert!(
            !rows.is_empty(),
            "the fixture-groups export selected nothing"
        );
        for r in rows {
            assert!(
                r.slug == r.platform || r.slug.starts_with(&format!("{}-", r.platform)),
                "slug {:?} is not a group of platform {:?}",
                r.slug,
                r.platform
            );
            assert!(
                !r.artifact_root.is_empty(),
                "row {:?} has no artifact root, so no resolver can find it",
                r.slug
            );
        }
    }

    #[test]
    fn every_shared_row_has_a_unique_artifact_root() {
        // The precondition that makes the inversion a FUNCTION. Two shared rows
        // sharing an artifact root cannot be told apart from a binary path, and
        // the resolver would pick one of their groups arbitrarily — a green
        // test on the wrong artifact. Fix by giving one its own `target_dir`,
        // never by loosening the rule. (`check-fixture-groups`'s A2 arm
        // enforces the same thing from the gate side, before a platform is
        // migrated; this catches it after.)
        let mut seen: std::collections::BTreeMap<&str, &str> = Default::default();
        for r in manifest_rows().iter().filter(|r| r.shared) {
            if let Some(prev) = seen.insert(&r.artifact_root, &r.slug) {
                assert_eq!(
                    prev, r.slug,
                    "artifact root {:?} is claimed by two different groups",
                    r.artifact_root
                );
            }
        }
    }

    #[test]
    fn a_multi_group_platform_refuses_to_name_one_dir() {
        // `sole_group_dir` must fail loudly rather than answer for the default
        // group — the exact silent-wrong-artifact shape phase-340 W1 measured.
        // Driven through the same code on a synthetic table would need the
        // static; assert the live one instead, which today has ONE group and so
        // must succeed.
        let dir = sole_group_dir("qemu-arm-baremetal").expect("one group today");
        assert_eq!(dir, group_dir("qemu-arm-baremetal"));
        assert!(
            sole_group_dir("no-such-platform").is_err(),
            "an unmigrated platform must be an error, not a guessed dir"
        );
    }

    #[test]
    fn two_groups_at_one_artifact_root_do_not_redirect() {
        // issue 0517 — the shape phase-340 W2.d's column deletion creates. Two
        // shared rows land on ONE artifact root and belong to DIFFERENT groups;
        // the longest-match rule cannot separate them, and picking either
        // redirects onto a real binary built with different features. Measured
        // over the shipped manifest this cannot happen (0 plain-row roots are
        // shared by more than one row), which is exactly why the input is
        // synthetic: the real table would pass under the tie-breaking rule too.
        let rows = vec![
            GroupRow {
                artifact_root: "examples/native/rust/talker/target".into(),
                platform: "linux".into(),
                slug: "linux".into(),
                shared: true,
                ..Default::default()
            },
            GroupRow {
                artifact_root: "examples/native/rust/talker/target".into(),
                platform: "linux".into(),
                slug: "linux-553222167".into(),
                shared: true,
                ..Default::default()
            },
        ];
        assert!(
            attribute(
                &rows,
                Path::new("examples/native/rust/talker/target/x/talker")
            )
            .is_none(),
            "an ambiguous root must not redirect: no redirect resolves the leaf \
             path, finds nothing and fails loudly, which is the outcome to prefer \
             over a silently wrong binary"
        );

        // Same root, SAME group: not ambiguous — the question this function
        // answers is which group, and both rows answer it identically.
        let mut same = rows.clone();
        same[1].slug = "linux".into();
        let (row, suffix) = attribute(
            &same,
            Path::new("examples/native/rust/talker/target/x/talker"),
        )
        .expect("one group, so the answer is unambiguous");
        assert_eq!(row.slug, "linux");
        assert_eq!(suffix, Path::new("x/talker"));

        // A LONGER match still wins outright — the tie rule must not make
        // longest-match give up whenever two shorter roots collide.
        let mut deeper = rows.clone();
        deeper.push(GroupRow {
            artifact_root: "examples/native/rust/talker/target/x".into(),
            platform: "linux".into(),
            slug: "linux-deeper".into(),
            shared: true,
            ..Default::default()
        });
        let (row, _) = attribute(
            &deeper,
            Path::new("examples/native/rust/talker/target/x/talker"),
        )
        .expect("the deeper root is unambiguous");
        assert_eq!(row.slug, "linux-deeper");
    }

    #[test]
    fn the_row_route_and_the_path_route_reach_the_same_directory() {
        // The equivalence the whole of #517 rests on: for EVERY row, selecting
        // it by (dir, selector) and resolving BY ROW must land where resolving
        // its leaf artifact root through the path redirect lands. Same table,
        // two routes, one answer — which is what makes each call-site conversion
        // verifiable without a fixture rebuild.
        let mut bad = Vec::new();
        for row in manifest_rows() {
            let via_path = resolved(&project_root().join(&row.artifact_root));
            match select_row(&row.dir, &FixtureVariant(row.selector.clone())) {
                Ok(got) if row_resolved_dir(got) == via_path => {}
                Ok(got) => bad.push(format!(
                    "{} {:?}: row -> {}, path -> {}",
                    row.dir,
                    row.selector,
                    row_resolved_dir(got).display(),
                    via_path.display()
                )),
                Err(e) => bad.push(format!("{} {:?}: {e}", row.dir, row.selector)),
            }
        }
        assert!(
            bad.is_empty(),
            "{} row(s) resolve differently by row than by path:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    #[test]
    fn the_carried_coordinate_matches_what_the_path_route_derived() {
        // The lane check moved from "attribute the path, read its coordinate" to
        // "read the coordinate off the row we already selected" (issue 0517 step
        // 1). Those must be the same verdict, or a narrowed run changes what it
        // skips. Checked against `lane`'s own table, which is a DIFFERENT export
        // (`coords`) of the same `row_coord` — so this also catches the two
        // exports disagreeing.
        let mut bad = Vec::new();
        for row in manifest_rows() {
            let probe = Path::new(&row.artifact_root).join("some/binary");
            let Some(lane_row) = crate::fixtures::lane::attribute_path_in(
                crate::fixtures::lane::manifest_rows(),
                &probe,
            ) else {
                continue;
            };
            if lane_row.coord != row.coord {
                bad.push(format!(
                    "{}: fixture-groups says {:?}, coords says {:?}",
                    row.dir, row.coord, lane_row.coord
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "{} row(s) carry a coordinate the path route disagrees with:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    #[test]
    fn the_four_constructors_each_select_real_rows() {
        // A constructor that matches NOTHING is worse than no constructor: the
        // call site would fail closed at run time, in a lane, on a machine that
        // has the fixture. Assert each shape the exporter counts is reachable.
        let rows = manifest_rows();
        let n = |v: &FixtureVariant| rows.iter().filter(|r| r.selector == v.0).count();
        assert!(n(&FixtureVariant::plain()) > 0, "plain rows exist");
        assert!(
            n(&FixtureVariant::rmw(crate::fixtures::Rmw::Zenoh)) > 0,
            "feature-selected RMW rows exist"
        );
        assert!(
            n(&FixtureVariant::platform_rmw(crate::fixtures::Rmw::Zenoh)) > 0,
            "platform-baked RMW rows exist"
        );
        assert!(
            n(&FixtureVariant::features(&["link-tls"])) > 0,
            "feature-variant rows exist"
        );
    }

    #[test]
    fn an_unnameable_variant_is_an_error_not_a_guess() {
        assert!(
            select_row(
                "examples/native/rust/talker",
                &FixtureVariant::features(&["nope"])
            )
            .is_err(),
            "a variant no row authors must fail closed"
        );
        assert!(
            select_row("examples/no/such/leaf", &FixtureVariant::plain()).is_err(),
            "an unknown leaf must fail closed"
        );
    }

    #[test]
    fn parse_rejects_a_short_record() {
        let r = std::panic::catch_unwind(|| parse_rows("a\x1fb\x1fc\n"));
        assert!(r.is_err(), "a 3-field record must not parse as a row");
    }
}
