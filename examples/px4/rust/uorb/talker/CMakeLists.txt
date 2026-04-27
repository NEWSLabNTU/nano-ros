set(PX4_RS_DIR ${CMAKE_CURRENT_LIST_DIR}/../../../../../third-party/px4-rs)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     nros_talker
    CRATE    px4-rs-talker
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
    ENTRY    nros_talker_main
)
