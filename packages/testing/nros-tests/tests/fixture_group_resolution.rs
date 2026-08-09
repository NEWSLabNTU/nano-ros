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
    fixtures::groups::{GroupRow, attribute, parse_rows},
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

/// The four authored artifact roots of `examples/native/rust/talker`, which is
/// the leaf W1 measured. Named literally so a manifest edit that drops one
/// fails here instead of quietly shrinking the test.
const TALKER_ROOTS: [&str; 4] = [
    "examples/native/rust/talker/target",
    "examples/native/rust/talker/target-zenoh",
    "examples/native/rust/talker/target-xrce",
    "examples/native/rust/talker/target-tls",
];

#[test]
fn migrating_linux_gives_each_talker_variant_its_own_group_dir() {
    let rows = export_with("qemu-arm-baremetal linux");

    let mut dirs = BTreeSet::new();
    for root in TALKER_ROOTS {
        let rel = Path::new(root).join("debug/talker");
        let (row, suffix) = attribute(&rows, &rel).unwrap_or_else(|| {
            panic!("{root} did not attribute to a shared group — is the row still in the manifest?")
        });
        assert_eq!(
            row.artifact_root, root,
            "{root} attributed to a different row's artifact root"
        );
        assert_eq!(suffix, Path::new("debug/talker"));
        dirs.insert(build_dir("fixtures-cargo", &[row.slug.as_str()]));
    }

    assert_eq!(
        dirs.len(),
        TALKER_ROOTS.len(),
        "the four talker variants collapsed to {} group dir(s): {dirs:?}. They are \
         four DIFFERENT binaries at one artifact name, and cargo replaces the \
         flat artifact silently across invocations (the `output filename \
         collision` warning fires only when ONE invocation builds both).",
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
    for platforms in ["qemu-arm-baremetal", "qemu-arm-baremetal linux"] {
        let rows = export_with(platforms);
        for row in rows.iter().filter(|r| r.shared) {
            for tail in ["debug/bin", "thumbv7m-none-eabi/debug/bin"] {
                let rel = Path::new(&row.artifact_root).join(tail);
                let (_, suffix) = attribute(&rows, &rel).expect("a shared row attributes");
                assert_eq!(
                    suffix,
                    Path::new(tail),
                    "the redirect altered the path below {}'s artifact root",
                    row.artifact_root
                );
            }
        }
    }
}

#[test]
fn the_shipped_eligibility_list_redirects_no_linux_row() {
    // Inertness. B2 is the resolver; B3 is the migration, and it needs a
    // native-lane rebuild because #393's failure mode is the build, the
    // staleness probe and the test resolver disagreeing.
    //
    // Deliberately the LIVE table (`manifest_rows`, no env override), not
    // `export_with("qemu-arm-baremetal")`. The first version passed the list in
    // — and so kept passing when the shipped default was widened to include
    // `linux`, which is the one change it exists to notice. B3 must edit this
    // test; that is the point of it.
    let rows = nros_tests::fixtures::groups::manifest_rows();
    for root in TALKER_ROOTS {
        assert!(
            attribute(rows, &Path::new(root).join("debug/talker")).is_none(),
            "{root} is redirected on the shipped tree — B2 must land inert. If \
             this is B3, update this test along with NROS_FIXTURE_SHARED_PLATFORMS."
        );
    }
    assert!(
        rows.iter()
            .any(|r| r.shared && r.platform == "qemu-arm-baremetal"),
        "no qemu-arm-baremetal row reports as shared — the export lost the one \
         migrated platform, and this test would then be vacuous"
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
    assert_eq!(
        rows.iter().filter(|r| r.shared).count(),
        export_with("qemu-arm-baremetal")
            .iter()
            .filter(|r| r.shared)
            .count(),
        "an empty NROS_FIXTURE_SHARED_PLATFORMS must resolve to the shell's \
         default list; anything reading it as `share nothing` is a second \
         eligibility rule"
    );
}
