# The shared-cargo-directory key, computed by the PRODUCTION code — phase-439 W1.
#
# `check-cargo-dir-knob-key.sh` drives this in cmake SCRIPT mode (`cmake -P`) so
# the thing under test is `cmake/NanoRosSharedCargoDir.cmake` itself: the same
# `nros_knob_key_fields()` the `nros-c`, NuttX and Zephyr lanes call, and the
# same normalise-and-hash. A probe that re-derived the key would pass against
# its own copy of the rule, which is the defect and not the test.
#
#   -DNROS_PROBE_ROOT=<dir>     where the keyed directories are created
#   -DNROS_PROBE_MODE=with-knobs   the key as it is built today
#                     base-only    the key WITHOUT the knob fields — the
#                                  pre-phase-439 shape, kept as the gate's
#                                  negative control (revert the change and the
#                                  collision must reappear)
#                     inventory    print the harvested knob NAMES and stop
#
# Prints `DIR=` and `KEY=` on stdout, one per line.

cmake_minimum_required(VERSION 3.20)

include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosSharedCargoDir.cmake")

if(NOT DEFINED NROS_PROBE_MODE)
    set(NROS_PROBE_MODE "with-knobs")
endif()

if(NROS_PROBE_MODE STREQUAL "inventory")
    _nros_knob_inventory(_names)
    foreach(_n IN LISTS _names)
        message("KNOB=${_n}")
    endforeach()
    return()
endif()

if(NOT NROS_PROBE_ROOT)
    message(FATAL_ERROR "shared-cargo-key-probe: -DNROS_PROBE_ROOT is required")
endif()
set(NROS_SHARED_CARGO_ROOT "${NROS_PROBE_ROOT}")

# A fixed stand-in for the six fields the `nros-c` lane already keyed on. Their
# VALUES are irrelevant to what this probe measures — every run holds them
# constant, so the only thing that can move the hash is the knob half.
set(_key
    "features=alloc,platform-posix,rmw-cffi"
    "rmw=zenoh"
    "board=host"
    "caps="
    "profile=release"
    "target=x86_64-unknown-linux-gnu")

if(NOT NROS_PROBE_MODE STREQUAL "base-only")
    nros_knob_key_fields(_knob_fields)
    list(APPEND _key ${_knob_fields})
endif()

nros_shared_cargo_dir(_dir KEY ${_key})
message("DIR=${_dir}")
message("KEY=${_dir_KEY_TEXT}")
