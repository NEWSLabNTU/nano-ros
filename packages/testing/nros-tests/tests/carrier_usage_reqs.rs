//! Issue 2817 -- the native carrier of an EXISTING_TARGET registration links
//! the library it registers.
//!
//! `nano_ros_auto_add_library(cur_lib)` + `nros_components_register_node(cur_lib
//! ... TYPED)` builds, on the linux board, the native carrier
//! (`<pkg>__<exe>`, program `cur_probe`) beside the image's generated entry. The library has a
//! PUBLIC include directory outside `cur_pkg/include` and a PUBLIC link to
//! `cur_vendor`, both stated after the registration. The carrier used to
//! recompile the library's sources with only `cur_pkg/include` and
//! `cur_pkg/src`, and stopped the image's whole build on the missing header.
//!
//! Compiles nothing: the image is the prebuilt `workspace-carrier-usage-reqs`
//! row. Its build succeeding is half the verdict; the carrier then runs and
//! prints the answer it can only get from `cur_vendor`.

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures::{self, ManagedProcess, RequireFixture, ZenohRouter, require_zenohd, zenohd_unique},
    lane_scope,
    matrix::PlatformId,
};
use rstest::rstest;

const ROW: &str = "workspace-carrier-usage-reqs";
const MARKER: &str = "CARRIER_USAGE_REQS answer=42";

#[rstest]
fn the_native_carrier_takes_the_usage_requirements_of_the_library_it_links(
    #[from(zenohd_unique)] router: ZenohRouter,
) {
    if !lane_scope::admits(PlatformId::Linux) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::Linux, "carrier-usage-reqs")
        );
    }
    require_zenohd();
    // The generated entry is the row's artifact; the carrier is built in the
    // same cmake build, in its package's binary dir, named for its EXECUTABLE.
    let entry = fixtures::build_workspace_cmake_generated_entry(ROW, "linux_entry")
        .require("workspace-carrier-usage-reqs linux_entry");
    let carrier = entry
        .parent()
        .expect("the entry's build dir")
        .join("pkg/cur_pkg/cur_probe");
    assert!(
        carrier.is_file(),
        "the image built but its native carrier is missing at {}",
        carrier.display()
    );

    let mut cmd = Command::new(&carrier);
    cmd.env("NROS_LOCATOR", router.locator());
    let mut probe = ManagedProcess::spawn_image(cmd, "cur_probe").expect("spawn the carrier");
    let out = probe.wait_for_output_count(MARKER, 1, Duration::from_secs(15));
    probe.kill();
    out.expect("the carrier prints the answer from the library's PUBLIC link");
}
