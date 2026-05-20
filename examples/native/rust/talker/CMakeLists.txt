cmake_minimum_required(VERSION 3.22)
# Phase 170.A — Cyclone DDS build path for the collapsed pure-Rust
# talker. `cargo build --features rmw-cyclonedds` cannot link Cyclone
# (the backend is a C++ CMake project + needs idlc-generated
# `dds_topic_descriptor_t`s at cmake time). This CMakeLists drives the
# cyclonedds variant: it generates the Cyclone typesupport, imports the
# talker crate's `staticlib` (`--features rmw-cyclonedds`, exposing
# `rust_main`), and links the C++ backend + descriptors into a thin C
# `main`. The zenoh / xrce variants stay pure `cargo build` (no cmake).
#
# Corrosion can't link the C++ backend into a corrosion-imported *bin*
# (its `corrosion_link_libraries` can't pass whole-archive / transitive
# / raw-flag link items), but for a `-static` target it delegates to
# `target_link_libraries`, which handles the `NanoRos::NanoRos`
# INTERFACE genexes — hence the staticlib + C-main shape. idlc emits
# the descriptors as C source, so the project enables C.
project(native_rust_talker_cyclonedds VERSION 0.1.0 LANGUAGES C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Cyclone DDS idlc typesupport for the message packages the talker uses.
nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

# Import the talker crate's staticlib variant (the `[lib]` exposing
# `rust_main`), built with `--features rmw-cyclonedds`.
corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES native-rs-talker
    # Import ONLY the staticlib — not the `talker` bin. The bin's bare
    # cargo link can't resolve the C++ backend's `nros_rmw_cyclonedds_register`;
    # the descriptor + backend are linked by cmake into the C `main` below.
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(talker_cyclonedds src/cyclonedds_main.c)
# The staticlib owns the Rust nros runtime (+ nros-rmw-cffi registry +
# posix platform port); NanoRos contributes the C++ Cyclone backend +
# libddsc + stdc++ + the idlc typesupport. `std_msgs__nano_ros_c`
# force-loads the descriptor register TUs (WHOLE_ARCHIVE interface).
# `--allow-multiple-definition` reconciles the duplicate nros-rmw-cffi
# REGISTRY object defs; stdc++ trails the C++ archives (opaque `-Wl`
# flag, not deduped) so the C-driver link resolves `std::nothrow`.
target_link_libraries(talker_cyclonedds PRIVATE
    native_rs_talker
    std_msgs__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(talker_cyclonedds)
