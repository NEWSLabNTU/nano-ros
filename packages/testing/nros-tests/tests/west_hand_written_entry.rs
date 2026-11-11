//! Issue 2405 (and 2435) -- a HAND-WRITTEN C west application gets the seeds
//! `nros configure` writes, and its configure holds to them.
//!
//! The `west_hand_written_c_entry` west fixture (`scripts/build/west-fixtures.sh`)
//! configures `src/zephyr_entry`, an `[image.zephyr] entry = "zephyr_entry"`
//! application, with raw `west build` from a clean build dir. The module hook
//! runs `nros configure`, which writes the image's two seeds under the
//! workspace's `build/demo_bringup__zephyr/`; the configure copies each seed
//! VERBATIM into the fragment its readers read, and its producers must land
//! on the same bytes (`cmake/NanoRosPromise.cmake`).
//!
//! So each fragment equals its seed. That holds whether or not the image's
//! sizes derive, which is why this asserts the files rather than the
//! configure's exit: a missing seed is only a WARNING when nothing derives,
//! and this image's closure (all of `std_msgs`) derives no message sizes.
//! Before the fixes it failed both ways: no message-bound seed for a
//! hand-written entry (2405), none at all from a clean build dir whose probe
//! sidecar survived (2435), and an entity inventory composed without the
//! workspace when the entry spelled its bringup relative to itself (2435).

use std::path::Path;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()))
}

fn assert_held(build_dir: &Path, seed: &Path, fragment: &str, what: &str) {
    let configured = build_dir.join("nros").join(fragment);
    assert_eq!(
        read(seed),
        read(&configured),
        "{what}: the configure's `{}` differs from the seed `{}` `nros configure` wrote",
        configured.display(),
        seed.display()
    );
}

#[test]
fn a_hand_written_west_entry_configures_from_its_seeds() -> nros_tests::TestResult<()> {
    let cache =
        nros_tests::fixtures::require_west_fixture("west_hand_written_c_entry", "CMakeCache.txt")?;
    let build_dir = cache.parent().expect("west fixture build dir");
    let resolved = nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/west_hand_written_c_entry")
        .join("build/demo_bringup__zephyr");
    assert_held(
        build_dir,
        &resolved.join("resolved-message-bounds.cmake"),
        "message_bound_knobs.cmake",
        "message-bound sizes (issue 2405)",
    );
    assert_held(
        build_dir,
        &resolved.join("resolved.cmake"),
        "entity_inventory.cmake",
        "entity inventory (issue 2435)",
    );
    Ok(())
}
