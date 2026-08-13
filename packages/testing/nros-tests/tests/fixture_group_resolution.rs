//! The shared cargo `--target-dir` resolver, exercised against a MIGRATION
//! that has not happened yet — phase-340 B2.
//!
//! B2 lands inert: `NROS_FIXTURE_SHARED_PLATFORMS` still names only
//! `qemu-arm-baremetal`, whose 20 rows are one group, so on the shipped tree
//! nothing distinguishes a resolver that understands the variant slug from the
//! platform-only one it replaces. A test over the live table would therefore
//! pass under both and gate nothing — the trap this phase paid for three times.
//!
//! So these drive the export with `NROS_FIXTURE_SHARED_PLATFORMS` widened to
//! include `linux`, which is exactly the one-line change B3 makes, and assert
//! the properties that decide whether B3 is safe:
//!
//! * `examples/native/rust/talker`'s four rows resolve to FOUR DIFFERENT group
//!   dirs. Under the refuted platform-grained key they resolve to one, and
//!   phase-340 W1 measured what that means: four distinct binaries (8616504 /
//!   8616504 / 6514392 / 9034536 bytes, four distinct sha256) at one path, the
//!   last invocation silently winning.
//! * the redirect never synthesises a `<triple>/` component. 0 of 65 `linux`
//!   rows carry `--target`, and the resolver this replaces hardcoded one.
//! * with the SHIPPED eligibility list, none of it happens — the inertness B2
//!   claims, asserted rather than described.
//!
//! Driving the real `fixtures-manifest.py` (not a fabricated table) is the
//! point: the manifest row, the shell `cksum` and the Rust inversion are three
//! parties, and this is the only place all three meet.

use std::{collections::BTreeSet, path::Path, process::Command};

use nros_tests::{
    build_dir,
    fixtures::{
        Rmw,
        groups::{FixtureVariant, GroupRow, attribute, group_dir, parse_rows, select_row_in},
    },
    project_root,
};

/// `fixtures-manifest.py fixture-groups` under a chosen eligibility list.
fn export_with(shared_platforms: &str) -> Vec<GroupRow> {
    let root = project_root();
    let out = Command::new("python3")
        .arg(root.join("scripts/build/fixtures-manifest.py"))
        .arg("fixture-groups")
        .env("NROS_FIXTURE_SHARED_PLATFORMS", shared_platforms)
        .current_dir(&root)
        .output()
        .expect("run fixtures-manifest.py");
    assert!(
        out.status.success(),
        "fixture-groups failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    parse_rows(&String::from_utf8_lossy(&out.stdout))
}

/// The leaf W1 measured, and its VARIANTS named literally — for the reason the
/// four authored artifact roots were named literally here before: a manifest
/// edit that drops one must fail here instead of quietly shrinking the test.
///
/// Issue 0517 step 3 deleted the `target_dir` column those roots were spelt
/// with, so all five rows now share `<leaf>/target` and NONE of them is
/// attributable by path — deliberately, and asserted positively by
/// `every_fixture_row_attributes_to_itself`'s multi-row half. The property this
/// test exists for is untouched by that: the variants must still land in
/// DIFFERENT group dirs. Only the vocabulary moves, from the authored path to
/// the selector the caller now names.
const TALKER_LEAF: &str = "examples/native/rust/talker";

fn talker_variants() -> Vec<(&'static str, FixtureVariant)> {
    vec![
        ("plain", FixtureVariant::plain()),
        ("link-tls", FixtureVariant::features(&["link-tls"])),
        ("rmw-zenoh", FixtureVariant::rmw(Rmw::Zenoh)),
        ("rmw-xrce", FixtureVariant::rmw(Rmw::Xrce)),
        ("rmw-cyclonedds", FixtureVariant::rmw(Rmw::Cyclonedds)),
    ]
}

#[test]
fn migrating_linux_gives_each_talker_variant_its_own_group_dir() {
    let rows = export_with("qemu-arm-baremetal linux");

    let mut dirs = BTreeSet::new();
    for (label, variant) in talker_variants() {
        let row = select_row_in(&rows, TALKER_LEAF, &variant)
            .unwrap_or_else(|e| panic!("talker variant {label} does not select a row: {e:?}"));
        assert!(
            row.shared,
            "talker variant {label} is not shared with `linux` migrated"
        );
        dirs.insert(build_dir(
            nros_tests::kind::CARGO_FIXTURES,
            &[row.slug.as_str()],
        ));
    }

    assert_eq!(
        dirs.len(),
        talker_variants().len(),
        "the {} talker variants collapsed to {} group dir(s): {dirs:?}. They are \
         DIFFERENT binaries at one artifact name, and cargo replaces the \
         flat artifact silently across invocations (the `output filename \
         collision` warning fires only when ONE invocation builds both).",
        talker_variants().len(),
        dirs.len()
    );
}

#[test]
fn migrating_linux_never_synthesises_a_triple_component() {
    // The recorded sub-blocker: `require_shared_fixture_binary` hardcoded a
    // `{triple}/` component and 0 of 65 `linux` rows carry `--target`. The
    // redirect is a prefix rewrite, so the component count below the group dir
    // must equal the component count below the leaf artifact root — always,
    // for every shared row on both a host and a cross platform.
    //
    // Over the SOLE-row leaves: issue 0517 step 3 made a multi-row leaf's
    // `<dir>/target` shared by all its rows and therefore ambiguous, so
    // `attribute` declines it by design and there is no path route left to
    // assert about. The sole-row leaves are also exactly the population
    // `require_shared_fixture_binary` — the function that hardcoded the triple —
    // serves, so the guard keeps its subject.
    for platforms in ["qemu-arm-baremetal", "qemu-arm-baremetal linux"] {
        let rows = export_with(platforms);
        let mut checked = 0usize;
        for row in rows.iter().filter(|r| r.shared) {
            if rows.iter().filter(|o| o.dir == row.dir).count() > 1 {
                continue;
            }
            for tail in ["debug/bin", "thumbv7m-none-eabi/debug/bin"] {
                let rel = Path::new(&row.artifact_root).join(tail);
                let (_, suffix) = attribute(&rows, &rel).unwrap_or_else(|| {
                    panic!(
                        "sole-row leaf {} does not attribute by path — step 3 keeps \
                         this half of the invariant",
                        row.artifact_root
                    )
                });
                assert_eq!(
                    suffix,
                    Path::new(tail),
                    "the redirect altered the path below {}'s artifact root",
                    row.artifact_root
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no sole-row shared leaf under {platforms:?} — every assertion above \
             is vacuous"
        );
    }
}

#[test]
fn the_shipped_eligibility_list_redirects_every_linux_row() {
    // phase-340 B3 landed 2026-08-10: `linux` IS migrated, so the assertion
    // inverts. B2 wrote this test to force exactly this edit — it read the LIVE
    // table rather than an injected list, so widening the shipped default could
    // not slip past it. Keeping that property: still the live table.
    // Issue 0517 step 3: the question is asked per VARIANT, not per authored
    // artifact root. Talker's five rows share `<leaf>/target` now, so `attribute`
    // is the wrong instrument here — the right one is the selector a caller
    // names, resolved against the LIVE table (the property B2 wrote this test
    // for: a widened shipped default must not slip past it).
    let rows = nros_tests::fixtures::groups::manifest_rows();
    let mut dirs = BTreeSet::new();
    for (label, variant) in talker_variants() {
        let row = select_row_in(rows, TALKER_LEAF, &variant).unwrap_or_else(|e| {
            panic!("talker variant {label} does not select a row on the shipped tree: {e:?}")
        });
        assert!(
            row.shared,
            "talker variant {label} is NOT redirected on the shipped tree — B3 added \
             `linux` to NROS_FIXTURE_SHARED_PLATFORMS, so every linux talker row must \
             resolve through a group"
        );
        assert!(
            row.slug == "linux" || row.slug.starts_with("linux-"),
            "talker variant {label} resolved to slug {:?}, which is not a linux group",
            row.slug
        );
        // The variant slug is the whole point: talker's variants must NOT all
        // land on one group, which is what the refuted coarse key did.
        dirs.insert(group_dir(&row.slug));
    }
    assert_eq!(
        dirs.len(),
        talker_variants().len(),
        "talker's variants collapsed to {} group dir(s) on the shipped tree: {dirs:?}",
        dirs.len()
    );

    // Vacuity guard, kept and widened: both migrated platforms must report.
    for p in ["qemu-arm-baremetal", "linux"] {
        assert!(
            rows.iter().any(|r| r.shared && r.platform == p),
            "no {p} row reports as shared — the export lost a migrated platform, \
             and every assertion above would then be vacuous"
        );
    }

    // The variants must occupy MORE THAN ONE group, or the coarse key has crept
    // back in under a different name.
    let slugs: std::collections::BTreeSet<&str> = rows
        .iter()
        .filter(|r| r.shared && r.platform == "linux")
        .map(|r| r.slug.as_str())
        .collect();
    assert!(
        slugs.len() > 1,
        "every linux row landed in ONE group ({slugs:?}) — that is the refuted \
         platform-grained key, which silently overwrites the flat artifact"
    );
}

#[test]
fn an_empty_eligibility_list_means_the_default_list_not_the_empty_one() {
    // There is NO "share nothing" spelling, and assuming one is a live trap.
    // `fixtures-target-dir.sh` writes
    // `${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal}`, and `:-` treats an
    // EMPTY value as unset — so clearing the variable restores the default
    // rather than disabling sharing.
    //
    // This is not a curiosity: `fixtures::groups::redirect` briefly carried a
    // "cheap gate" that read the same variable in Rust and concluded the
    // opposite, which is a second copy of the eligibility rule disagreeing with
    // the first — the #393 class the whole design removes. The gate is gone;
    // this keeps the reason on file.
    let rows = export_with("");
    assert!(
        !rows.is_empty(),
        "the export must still describe every cargo row"
    );
    // Compare against the LIVE table, not a hardcoded list. The first version
    // named `qemu-arm-baremetal` literally and so had to be edited when B3
    // widened the default to `qemu-arm-baremetal linux` — a test that pins the
    // value it is checking becomes a maintenance tax and, worse, a place where
    // someone "fixes" the test instead of noticing the change. `manifest_rows()`
    // already resolves through the shell's own `:-` default, so this derives.
    assert_eq!(
        rows.iter().filter(|r| r.shared).count(),
        nros_tests::fixtures::groups::manifest_rows()
            .iter()
            .filter(|r| r.shared)
            .count(),
        "an empty NROS_FIXTURE_SHARED_PLATFORMS must resolve to the shell's \
         default list; anything reading it as `share nothing` is a second \
         eligibility rule"
    );
}
