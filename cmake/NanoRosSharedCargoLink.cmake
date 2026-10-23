# Issue 1990 — a build dir's `cargo` link outlives its target.
#
# `nros_share_corrosion_cargo_dir()` (NanoRosCorrosion.cmake) creates the
# shared cargo directory and points `${CMAKE_BINARY_DIR}/cargo` at it, both at
# CONFIGURE time. The runner sweep (`scripts/ci/runner-sweep.sh`) evicts a
# whole `build/corrosion-cargo/<platform>` unit, and the leaves that link into
# it are not eviction units, so their links are left dangling. A build that
# does not reconfigure then reaches cargo, cargo `mkdir`s `<build>/cargo`, and
# `mkdir` on a dangling link is EEXIST:
#
#   error: failed to create directory `<build>/cargo`
#   Caused by: File exists (os error 17)
#
# This runs at BUILD time, before every `_cargo-build_*` edge of the configure
# (wired by `_nros_shared_cargo_link_order` in NanoRosCorrosion.cmake), and
# re-creates whatever the link names when it is missing. It reads the LINK, not
# a directory baked in at configure: the entity-facts flush re-keys the link at
# the end of the configure (issue 1700), so the link is the one place that is
# always current. An evicted cache then costs a rebuild, which is the sweep's
# own contract ("evicting a fixture makes the next job's staleness probe
# rebuild it"), never a failure.
#
#   cmake -DNROS_SHARED_CARGO_LINK=<build>/cargo -P cmake/NanoRosSharedCargoLink.cmake
#
# A link that is not a symlink (sharing off, or the degrade path's real
# directory) and a link whose target exists are both left alone.

if(NOT CMAKE_SCRIPT_MODE_FILE)
    message(FATAL_ERROR
        "NanoRosSharedCargoLink.cmake is a `cmake -P` script, not a module.")
endif()
if(NOT DEFINED NROS_SHARED_CARGO_LINK OR NROS_SHARED_CARGO_LINK STREQUAL "")
    message(FATAL_ERROR
        "usage: cmake -DNROS_SHARED_CARGO_LINK=<build>/cargo "
        "-P cmake/NanoRosSharedCargoLink.cmake")
endif()

if(NOT IS_SYMLINK "${NROS_SHARED_CARGO_LINK}")
    return()
endif()
file(READ_SYMLINK "${NROS_SHARED_CARGO_LINK}" _target)
if(NOT IS_ABSOLUTE "${_target}")
    get_filename_component(_link_dir "${NROS_SHARED_CARGO_LINK}" DIRECTORY)
    get_filename_component(_target "${_link_dir}/${_target}" ABSOLUTE)
endif()
if(IS_DIRECTORY "${_target}")
    return()
endif()
if(EXISTS "${_target}")
    message(FATAL_ERROR
        "nano-ros: ${NROS_SHARED_CARGO_LINK} points at ${_target}, which is "
        "not a directory. Cargo cannot use it as a target dir; this file was "
        "not created by nano-ros, so it is not removed here.")
endif()
file(MAKE_DIRECTORY "${_target}")
message(STATUS
    "nano-ros: re-created the shared cargo dir ${_target} — it was removed "
    "(runner sweep?) after this build dir was configured, so cargo rebuilds "
    "into it (issue 1990).")
