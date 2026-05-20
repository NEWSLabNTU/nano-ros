cmake_minimum_required(VERSION 3.22)
project(cyclonedds_rust_listener VERSION 0.1.0 LANGUAGES C)

# Phase 171.C.1.rust — native rust cyclonedds is cmake-driven: the
# Rust app is a `staticlib` (`rustapp`) imported via corrosion; this
# project supplies the Cyclone DDS IDL typesupport (idlc, cmake-time)
# + the C++ backend that pure-cargo cannot.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Cyclone DDS IDL typesupport for the message packages (idlc descriptors
# + auto-register TUs), same as the c/cpp cyclonedds examples.
nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

# Import the Rust app staticlib (`[lib] name = "rustapp"`). Corrosion
# is pulled into scope by the nano-ros add_subdirectory above.
corrosion_import_crate(MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")

add_executable(rust_listener src/main.c)
# rustapp owns the Rust nros runtime (+ nros-rmw-cffi registry +
# posix platform port). NanoRos here contributes the C++ cyclonedds
# backend + libddsc + stdc++ + the idlc typesupport. Both transitively
# pull nros-rmw-cffi; the `#[no_mangle]` REGISTRY static collapses to
# one instance, but the duplicate object defs need
# `--allow-multiple-definition` (same reconciliation as
# nros-rmw-zenoh-staticlib).
target_link_libraries(rust_listener PRIVATE
    rustapp
    std_msgs__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    # The C++ Cyclone DDS backend (force-loaded via the message lib's
    # WHOLE_ARCHIVE interface) lands late on the link line, after the
    # `-lstdc++` that NanoRos's INTERFACE injects. This executable is
    # linked with the C driver (no .cpp → no automatic C++ runtime), so
    # static left-to-right resolution leaves `std::nothrow` et al.
    # undefined. Re-list stdc++ last; pass it as an opaque `-Wl,` flag
    # so CMake does not de-duplicate it against the early occurrence and
    # it stays after the backend archive.
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(rust_listener)
