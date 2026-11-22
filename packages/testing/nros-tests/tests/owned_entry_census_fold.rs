//! phase-583 (RFC-0071 D20, RFC-0100 D12) -- an owned entry that builds TWO
//! programs on a cross board is sized for the larger one.
//!
//! On a cross board an owned entry's pools are sized from a census of its
//! program, and every program links one runtime whose pool sizes are fixed at
//! build time. The fixture `fixtures/owned_two_programs_threadx` builds
//! `prog_small` (the talker: 1 timer, 1 publisher) and `prog_large` (3 timers,
//! 4 publishers) from one package. Its built sizing descriptor must state the
//! larger program's counts, not the first program's (which is what a census
//! of one program stated).

use nros_tests::{
    fixtures::{RequireFixture, Rmw, migrated_leaf::LeafVariant},
    matrix::PlatformId,
};

const DIR: &str = "packages/testing/nros-tests/fixtures/owned_two_programs_threadx";
const ROW: &str = "threadx-linux-c-zenoh-owned-two-programs";

#[test]
fn an_owned_entry_with_two_programs_is_sized_for_the_larger() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxLinux],
        "the owned-entry census fold",
    );
    // Both programs are built: the larger one is not dropped to fit the census.
    let large = nros_tests::fixtures::migrated_leaf_binary_named(
        DIR,
        Rmw::Zenoh,
        LeafVariant::Plain,
        "prog_large",
    )
    .unwrap_or_else(|| panic!("{DIR} is no migrated workspace leaf"))
    .require(ROW);
    let small = large.with_file_name("prog_small");
    assert!(
        small.is_file(),
        "prog_small was not built beside {}",
        large.display()
    );

    let build_dir = nros_tests::fixtures::groups::workspace_artifact_dir(ROW)
        .unwrap_or_else(|e| panic!("{ROW}: {e}"));
    let path = nros_sizing_descriptor::descriptor_path(&build_dir, "two_prog");
    let desc = nros_sizing_descriptor::read(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

    // The application's own timers: prog_large has 3, prog_small 1. (The
    // callback-slot and publisher counts also hold the stock node interfaces'
    // entities, the same for both programs.)
    assert_eq!(
        desc.image.timer_entities().stated().copied(),
        Some(3),
        "the image is sized from one program's census, not the larger one's"
    );
    // Every publisher of the larger program has its endpoint row.
    for topic in ["/chatter", "/chatter2", "/chatter3", "/chatter4"] {
        assert!(
            desc.endpoints.iter().any(|e| e.topic == topic),
            "no endpoint row for {topic}: the larger program's publishers were not counted"
        );
    }
}
