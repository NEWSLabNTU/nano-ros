# Thin wrapper pulling the nros_talker example crate into the PX4 build.
#
# The crate at examples/px4/rust/uorb/talker/ is the source of truth;
# this file only tells PX4's build system how to invoke it via
# px4-rs's px4_rust_module() helper.

set(PX4_RS_DIR  ${CMAKE_CURRENT_LIST_DIR}/../../../../../../../third-party/px4/px4-rs)
set(CRATE_DIR   ${CMAKE_CURRENT_LIST_DIR}/../../../talker)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     nros_talker
    CRATE    px4_rs_talker
    MANIFEST ${CRATE_DIR}/Cargo.toml
    ENTRY    nros_talker_main
)
