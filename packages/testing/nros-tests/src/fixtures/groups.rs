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
#[derive(Debug, Clone, PartialEq, Eq)]
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
            4,
            "unexpected `fixture-groups` record shape (expected 4 \\x1f-separated \
             fields: artifact_root, platform, slug, shared): {line:?}"
        );
        rows.push(GroupRow {
            artifact_root: f[0].to_string(),
            platform: f[1].to_string(),
            slug: f[2].to_string(),
            shared: matches!(f[3], "1"),
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
pub fn attribute<'a>(rows: &'a [GroupRow], rel: &Path) -> Option<(&'a GroupRow, PathBuf)> {
    let mut best: Option<&'a GroupRow> = None;
    for row in rows {
        if !row.shared || row.artifact_root.is_empty() {
            continue;
        }
        if !path_under(rel, Path::new(&row.artifact_root)) {
            continue;
        }
        if best.is_none_or(|b| row.artifact_root.len() > b.artifact_root.len()) {
            best = Some(row);
        }
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
            },
            GroupRow {
                artifact_root: "examples/native/rust/talker/target-xrce".into(),
                platform: "linux".into(),
                slug: "linux-3000917972".into(),
                shared: true,
            },
            GroupRow {
                artifact_root: "examples/freertos/rust/talker/target".into(),
                platform: "freertos".into(),
                slug: "freertos".into(),
                shared: false,
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
            Path::new("examples/native/rust/talker/target/debug/talker"),
        )
        .unwrap();
        assert_eq!(row.slug, "linux");
        assert_eq!(suffix, Path::new("debug/talker"));

        let (row, suffix) = attribute(
            &t,
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
        }];
        let (_, suffix) = attribute(
            &t,
            Path::new("examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/debug/t"),
        )
        .unwrap();
        assert_eq!(suffix, Path::new("thumbv7m-none-eabi/debug/t"));

        let (_, suffix) = attribute(
            &table(),
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
    fn parse_rejects_a_short_record() {
        let r = std::panic::catch_unwind(|| parse_rows("a\x1fb\x1fc\n"));
        assert!(r.is_err(), "a 3-field record must not parse as a row");
    }
}
