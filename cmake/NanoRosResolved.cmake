# NanoRosResolved.cmake -- phase-439 W2 (RFC-0094 D1/D2): the CONFIGURE-SIDE
# READER for `nros build`'s resolve phase.
#
# =============================================================================
# What stage 3.5 is, from this side
# =============================================================================
#
# `nros build` gains a phase between "is the toolchain present" (stage 3) and
# "emit a root build file" (stage 4). It reads DECLARATIONS -- the descriptors,
# the image block, and the wiring the launch tree states through its contract
# sidecar -- runs `EntityInventory::derive` once for the whole image, and writes
#
#   <NROS_RESOLVED_DIR>/resolved.toml    the answer, with [provenance]
#   <NROS_RESOLVED_DIR>/resolved.cmake   the same answer, include()able
#
# BEFORE any configure. `nros build` passes the directory as
# `-DNROS_RESOLVED_DIR=<dir>` on the cmake and west handoffs.
#
# =============================================================================
# Why a reader wants it: the producer-after-reader lag
# =============================================================================
#
# `${CMAKE_BINARY_DIR}/nros/entity_inventory.cmake` is written by
# `nano_ros_entry()`, which must run after every `nano_ros_node_register()` --
# and is READ by `nros_resolve_knobs()` inside `find_package(Zephyr)` and by
# `nros_find_interfaces()`, both of which run earlier. On a clean build dir the
# earliest reader therefore finds a PLACEHOLDER, and issue 0991's future-mtime
# arm exists to buy a second configure in which it finds the answer.
#
# A file written before the configure has no such lag. Seeding the fragment from
# it means the earliest reader of the FIRST pass sees a real number, which is
# the pass that arm was buying.
#
# =============================================================================
# It is a SEED, not an OVERRIDE -- and that is the safety property
# =============================================================================
#
# `nros_resolved_seed_entity_inventory` writes ONLY where
# `nros_entity_inventory_seed_knobs_file` would have written a placeholder: a
# fragment that does not exist yet. The mid-configure producer still runs, still
# composes over `nros-metadata.json` AND the model, and still arms a
# re-configure through `nros_reconfigure_on_change` if its answer differs.
#
# That ordering is deliberate and it is what makes this safe to land before the
# fixed point is deleted. The two composers do not have the same INPUTS:
#
#   * this phase reads the model alone, since `nros-metadata.json` is written
#     DURING a configure and reading it here would recreate the lag;
#   * the mid-configure producer also reads that metadata, whose component set
#     is what makes `derive()` REFUSE when a registered component is absent from
#     the launch declaration.
#
# So the seed can only be equal to, or more optimistic than, the producer's
# answer -- and when it is more optimistic the producer overwrites it and arms,
# exactly as it does today over a placeholder. Nothing here can make a build
# quieter than it is now.
#
# =============================================================================
# A lane that ran no resolve phase is UNCHANGED
# =============================================================================
#
# `NROS_RESOLVED_DIR` unset -- a bare `west build`, `just zephyr
# build-fixtures`, a user's own `cmake -S` -- means every function here is a
# no-op and the placeholder is written as before. That is the whole reason the
# seam is a passed variable rather than a search: a reader that GUESSED where a
# resolve might live would silently pick up another image's answer, which is
# issue 0616's shape one directory over.

include_guard(GLOBAL)

# nros_resolved_dir(<out_var>)
#
# The directory stage 3.5 wrote this image's answer into, or empty.
#
# Read from the cache/normal variable first and the ENVIRONMENT second, because
# the two carriers reach different lanes: `-D` reaches a configure `nros build`
# spawned directly, and the environment reaches one it spawned through a tool
# that owns its own command line. Never GUESSED from `CMAKE_BINARY_DIR` -- see
# the header.
function(nros_resolved_dir _out_var)
    if(DEFINED NROS_RESOLVED_DIR AND NOT "${NROS_RESOLVED_DIR}" STREQUAL "")
        set(${_out_var} "${NROS_RESOLVED_DIR}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED ENV{NROS_RESOLVED_DIR} AND NOT "$ENV{NROS_RESOLVED_DIR}" STREQUAL "")
        set(${_out_var} "$ENV{NROS_RESOLVED_DIR}" PARENT_SCOPE)
        return()
    endif()
    set(${_out_var} "" PARENT_SCOPE)
endfunction()

# nros_resolved_entity_fragment(<out_var>)
#
# The include()able projection stage 3.5 wrote, or empty when there is none.
#
# EMPTY rather than a path-that-does-not-exist, so a caller cannot accidentally
# `include()` a missing file and get cmake's own error about a name that means
# nothing to the reader. The file name is spelled ONCE, here, matching
# `nros_cli_core::resolve::RESOLVED_CMAKE_NAME`.
function(nros_resolved_entity_fragment _out_var)
    nros_resolved_dir(_dir)
    if("${_dir}" STREQUAL "")
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    set(_frag "${_dir}/resolved.cmake")
    if(NOT EXISTS "${_frag}")
        # A resolve that REFUSED writes `resolved.toml` and no projection, on
        # purpose: an empty projection would publish "every count is zero",
        # which is a substituted default wearing an answer's clothes.
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    set(${_out_var} "${_frag}" PARENT_SCOPE)
endfunction()

# nros_resolved_seed_entity_inventory(<path> [<out_used>])
#
# Seed the image's entity-inventory fragment from the resolve phase's answer,
# if there is one and the fragment does not exist yet.
#
# Call it IMMEDIATELY BEFORE `nros_entity_inventory_seed_knobs_file(<path>)`,
# which is a no-op on a file that now exists. Both are then correct in either
# order of arrival, and a lane with no resolve keeps today's placeholder.
#
# Call it from a READER only. The three call sites inside
# `nros_derive_entity_inventory_knobs()` are a PRODUCER recording its own
# refusal ("I could not compose here"), and seeding those with a different
# composer's answer would publish a number the producer did not stand behind.
# Two roles, two behaviours -- the sites are not siblings even though they call
# one function.
function(nros_resolved_seed_entity_inventory _path)
    if(ARGC GREATER 1)
        set(${ARGV1} FALSE PARENT_SCOPE)
    endif()
    if(EXISTS "${_path}")
        return()
    endif()
    nros_resolved_entity_fragment(_frag)
    if("${_frag}" STREQUAL "")
        return()
    endif()
    get_filename_component(_dir "${_path}" DIRECTORY)
    file(MAKE_DIRECTORY "${_dir}")
    # VERBATIM. Not a copy with a "seeded from ..." banner on top, which is what
    # this did first and which MEASURED as buying nothing: `nros_reconfigure_
    # snapshot` hashes CONTENT, so a seed that differs from the producer's
    # answer by one comment line arms exactly the re-configure the seed exists
    # to remove. Case B of `tests/cmake-resolved-seed-tests.sh` is that
    # measurement, and it reported `re-runs=1` until the banner came off.
    #
    # The provenance is not lost, it is somewhere a byte comparison cannot
    # reach: the STATUS line below, and `resolved.toml`'s `[provenance]` table
    # beside the file this reads.
    file(READ "${_frag}" _body)
    file(WRITE "${_path}" "${_body}")
    if(ARGC GREATER 1)
        set(${ARGV1} TRUE PARENT_SCOPE)
    endif()
    message(STATUS
        "nros: entity inventory seeded from the resolve phase (${_frag}); this "
        "configure's first reader sees a derived answer rather than a placeholder")
endfunction()
