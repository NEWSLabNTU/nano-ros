//! The in-firmware PX4 uORB→RMW bridge's generated-message path compiles —
//! issue 0738.
//!
//! Issue 0362 delivered `nros generate-px4-msgs --lang cpp` and the bridge that
//! consumes it (`examples/px4/cpp/bridge/`). Nothing built either:
//! `just px4 build-bridge-example` had exactly one grep hit in the whole tree,
//! its own definition. So the emitter, the headers it writes, the
//! `_types.rs`/`_exports.rs` FFI bodies and the crate that `include!`s them were
//! all unexercised — and issue 0360 already flags that output as a per-variant
//! artifact that must stay paired with the archive it was built against.
//!
//! The build stage (`compile-check-fixtures.sh`, id `px4_bridge_ffi`) runs
//! stages [1/4] and [2/4] of that recipe and one thing the recipe does not:
//!
//!   1. generate for the bridge's topic set — the emitter runs;
//!   2. compile each generated `.hpp` standalone, one TU, `-fsyntax-only` —
//!      the header parses without the bridge's own translation unit around it;
//!   3. `cargo check` the FFI crate against that output — the Rust bodies still
//!      match the headers.
//!
//! Stage [4/4], the PX4 SITL `make`, is deliberately NOT here: it needs PX4's
//! build system and is far too heavy for a per-change tier. The codegen risk is
//! not in the link, it is in the emitter and the header shape, and that is what
//! this covers. The full module build stays `just px4 build-bridge-example`.
//!
//! Prerequisite: `just build-test-fixtures` with the PX4-Autopilot submodule
//! checked out. Absent submodule → no stamp → this fails with that instruction
//! rather than passing quietly, which is the whole point of the issue.

/// The generated C++ `px4_msgs` headers and the bridge's FFI crate compile
/// against each other.
#[test]
fn px4_cpp_bridge_generated_messages_compile() {
    // issue 0700 — state the coordinate. These px4 fixtures have no
    // `[[fixture]]` row, so `attribute_path` cannot place them and "cannot
    // attribute" reads as "not out of lane", i.e. run it. The companion suite
    // does the same for the same reason.
    nros_tests::fixtures::lane::require_coord_in_lane(
        &("linux".to_string(), "cpp".to_string(), "zenoh".to_string()),
        "px4_bridge_ffi",
    )
    .expect("lane check");

    let stamp = nros_tests::fixtures::require_compile_check("px4_bridge_ffi")
        .expect("px4 bridge compile-check stamp");
    assert!(
        stamp.is_file(),
        "expected the build stage's .compile-ok stamp at {}",
        stamp.display()
    );
}
