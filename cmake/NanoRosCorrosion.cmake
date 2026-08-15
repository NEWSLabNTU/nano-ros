# NanoRosCorrosion.cmake — the ONE place nano-ros decides which Corrosion a
# configure uses, and the one place that says so out loud.
#
# issue 0493. Corrosion's version decides the cargo target-dir TOPOLOGY:
#
#   v0.5.1  Corrosion.cmake:751  target dir is the CONSTANT `<build>/cargo/build`
#                                -> two cargo workspace ROOTS configured into one
#                                   binary dir share one `deps/`, and their
#                                   `#[no_mangle]` exports collide at link.
#   v0.6.1  Corrosion.cmake:781  target dir is `<name>_<sha1(manifest path)[0:5]>`
#                                -> cannot collide.
#
# So "which Corrosion did this configure resolve" is not a detail — it is the
# difference between a tree that links and a tree that does not. Two
# investigations (issue 0493 and phase-340/344) measured contradictory
# topologies for days because NOTHING reported the resolution, and because the
# answer differed per BUILDER: `compile-check-fixtures.sh` put the SDK prefix on
# `CMAKE_PREFIX_PATH` and the other two builders did not, so one host with one
# install produced both topologies. Hence this module: one derivation, and a
# `message(STATUS)` naming the origin, the version and the resulting topology.
#
# Two install LAYOUTS exist and both are supported, because the two provisioning
# paths disagree:
#
#   just workspace install-corrosion   ->  $NROS_HOME/sdk/corrosion/          (flat)
#   nros setup --tool corrosion        ->  $NROS_HOME/sdk/corrosion/<version>/
#
# The pre-0493 root-CMakeLists block globbed `corrosion/*` only, which sees the
# VERSIONED layout and, under the FLAT one, yields `lib/` and `share/` — two
# prefixes `find_package` cannot resolve from. That is why the SDK install was
# missed on a host that had it: not a provisioning step anyone forgot, an
# unsupported layout. Measured on a host whose `.installed-version` is v0.6.1:
#
#   prefix $NROS_HOME/sdk/corrosion        -> FOUND  (lib/cmake/Corrosion)
#   prefix $NROS_HOME/sdk/corrosion/lib    -> NOT FOUND
#   prefix $NROS_HOME/sdk/corrosion/share  -> NOT FOUND
#
# The shell-side sibling is `scripts/build/cmake-prefix.sh`, which exports the
# same prefixes for a configure that does NOT include this checkout's root (a
# standalone template calling `find_package(Corrosion)` on its own). Keep the
# two candidate rules in step; `check-cmake-corrosion-prefix` gates the wiring.

include_guard(GLOBAL)

# The store root — the same `$NROS_HOME` the CLI writes (`sdk_store.rs`).
function(_nros_corrosion_store out_var)
    if(DEFINED ENV{NROS_HOME})
        set(${out_var} "$ENV{NROS_HOME}/sdk" PARENT_SCOPE)
    else()
        set(${out_var} "$ENV{HOME}/.nros/sdk" PARENT_SCOPE)
    endif()
endfunction()

# Candidate `find_package` prefixes for an SDK-provisioned Corrosion, NEWEST
# VERSION first. A candidate is kept only when a `CorrosionConfig.cmake` actually
# sits under it — the pre-0493 glob's failure was emitting prefixes that could
# not resolve, so filter rather than hope.
#
# The ordering is load-bearing (issue 0500). `find_package` takes the first
# prefix that resolves, and the store accumulates: a host provisioned months ago
# keeps `0.5.1-nros1` beside a freshly installed `0.6.1-nros1`. Glob order is
# lexicographic, so the OLD one won — `nros setup --tool corrosion` installed the
# pin, reported success, and the very next configure still resolved 0.5.1. That
# is the worst shape a provisioning step can have: it appears to work. And the
# two versions are not interchangeable — 0.5.1 gives every workspace one shared
# `cargo/build`, which is what put two `nros-rmw-zenoh` identities in one
# `libnros_ws_runtime.a` and made `examples/workspaces/mixed` unlinkable.
#
# NATURAL sort so `0.10.x` sorts above `0.9.x`; DESCENDING so newest wins. The
# flat prefix stays LAST — it is the fallback layout, and a versioned entry is
# the one a provisioning run just wrote.
function(_nros_corrosion_prefixes out_var)
    _nros_corrosion_store(_store)
    set(_versioned_dirs "")
    # `nros setup --tool corrosion` — $NROS_HOME/sdk/corrosion/<version>/
    file(GLOB _versioned LIST_DIRECTORIES true "${_store}/corrosion/*")
    foreach(_dir IN LISTS _versioned)
        if(IS_DIRECTORY "${_dir}")
            list(APPEND _versioned_dirs "${_dir}")
        endif()
    endforeach()
    if(_versioned_dirs)
        list(SORT _versioned_dirs COMPARE NATURAL ORDER DESCENDING)
    endif()
    set(_candidates "${_versioned_dirs}")
    # `just workspace install-corrosion` — $NROS_HOME/sdk/corrosion/ itself.
    list(APPEND _candidates "${_store}/corrosion")

    set(_kept "")
    foreach(_prefix IN LISTS _candidates)
        file(GLOB _cfg
            "${_prefix}/lib*/cmake/Corrosion/CorrosionConfig.cmake"
            "${_prefix}/lib/*/cmake/Corrosion/CorrosionConfig.cmake"
            "${_prefix}/share/cmake/Corrosion/CorrosionConfig.cmake")
        if(_cfg)
            list(APPEND _kept "${_prefix}")
        endif()
    endforeach()
    set(${out_var} "${_kept}" PARENT_SCOPE)
endfunction()

# The FetchContent fallback tag. Read from `nros-sdk-index.toml`'s
# `[tool.corrosion] upstream = "vX.Y.Z"` so the pin is not copied a third time
# into cmake — the index and `just/workspace.just`'s `CORROSION_VERSION` are the
# two spellings a provisioning run already uses. The literal below is the
# fallback for a consumer who vendored `cmake/` without the index.
function(_nros_corrosion_pin out_var)
    set(_pin "v0.6.1")
    set(_index "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../nros-sdk-index.toml")
    if(EXISTS "${_index}")
        file(READ "${_index}" _index_raw)
        # `[tool.corrosion]` … `upstream = "v0.6.1"`, up to the next section head.
        if(_index_raw MATCHES "\\[tool\\.corrosion\\][^[]*upstream[ \t]*=[ \t]*\"([^\"]+)\"")
            set(_pin "${CMAKE_MATCH_1}")
        endif()
    endif()
    set(${out_var} "${_pin}" PARENT_SCOPE)
endfunction()

# Record the resolution where any directory scope can read it. CACHE INTERNAL
# rather than a normal variable, because the consumers are SIBLING scopes: a
# workspace leaf calls `add_subdirectory(<nano-ros>)`, and nothing the root sets
# normally travels back out to it.
function(_nros_corrosion_remember origin version location)
    set(NROS_CORROSION_ORIGIN_CACHED "${origin}" CACHE INTERNAL "")
    set(NROS_CORROSION_VERSION_CACHED "${version}" CACHE INTERNAL "")
    set(NROS_CORROSION_LOCATION_CACHED "${location}" CACHE INTERNAL "")
endfunction()

# Report the resolution — origin, version, location, and what that version means
# for the cargo target-dir topology. Called once per configure by
# `nros_resolve_corrosion()`.
#
# This line is the whole point of the module for a reader: issue 0493 and
# phase-340/344 disagreed for days about the topology because a configure never
# said which Corrosion it used. `< 0.6.0` means the hashless shared
# `cargo/build` and a possible duplicate-`#[no_mangle]` link failure;
# `>= 0.6.0` means per-workspace hashed dirs.
function(nros_report_corrosion origin version location)
    if(version STREQUAL "")
        set(version "unknown")
    endif()
    if(version MATCHES "^v?0\\.[0-5]\\.")
        set(_topology "hashless shared cargo/build — issue 0493 link risk")
    elseif(version STREQUAL "unknown")
        set(_topology "topology unknown")
    else()
        set(_topology "hashed per-workspace cargo dirs")
    endif()
    message(STATUS
        "nano-ros: Corrosion ${version} via ${origin} [${_topology}] — ${location}")

    # Issue 0493 — REFUSE the broken topology, do not merely narrate it.
    #
    # This line classified `< 0.6.0` as a link risk and then configured anyway,
    # so the only thing standing between a host and the duplicate-symbol failure
    # was the SDK store happening to hold a newer copy. The store ACCUMULATES
    # (issue 0500) and `check-cmake-corrosion-prefix` enforces newest-FIRST
    # ordering, not a version FLOOR: a host with only 0.5.1 installed resolves
    # it, silently, and gets two `-C metadata` identities of every nros crate in
    # one `deps/`.
    #
    # #493 asked for exactly this and it was never built: "Enforcement, either
    # way. The invariant is one Rust staticlib exporting the nros symbol set per
    # configure ... Without it, the next consumer that imports a root-workspace
    # crate re-creates this silently." That prediction came true on 2026-08-16 as
    # issue 0616, one lane over, where the same class reappeared with no
    # Corrosion involved at all. 0616 answered it for the Zephyr lane with a
    # configure-time FATAL_ERROR when two workspace roots claim one target-dir;
    # this is the same invariant for the Corrosion lane.
    #
    # `unknown` is NOT fatal: a parent project may have loaded Corrosion without
    # exporting its version, and failing there would break consumers who are not
    # even at risk. The status line above still says the topology is unknown.
    if(version MATCHES "^v?0\\.[0-5]\\." AND NOT NROS_ALLOW_LEGACY_CORROSION)
        message(FATAL_ERROR
            "nano-ros: Corrosion ${version} shares ONE cargo target-dir across "
            "workspace roots.\n"
            "  resolved: ${version} via ${origin} — ${location}\n"
            "That topology gives the same crate two `-C metadata` identities in one "
            "`deps/`, so every `#[no_mangle]` export collides at link and "
            "`nros-platform`'s single `#[global_allocator]` is defined twice "
            "(issues 0493, 0616).\n"
            "Fix: provision the pinned copy and clear EVERY tree that cached the "
            "old topology in its CMakeCache —\n"
            "    nros setup --tool corrosion\n"
            "    rm -rf examples/workspaces/*/build-workspace-fixtures*\n"
            "    grep -rl 'corrosion/0\\.[0-5]' examples/*/*/*/build*/CMakeCache.txt "
            "| xargs -r rm -f\n"
            "  (issue 0622 — the example LEAF caches hold the resolved path too, 62 of "
            "them on the host that found this, and clearing only the workspace trees "
            "reproduces this error verbatim. Deleting the CMakeCache.txt is enough; the "
            "trees themselves need not go.)\n"
            "Override for a deliberate experiment: -DNROS_ALLOW_LEGACY_CORROSION=ON")
    endif()
endfunction()

# --------------------------------------------------------------------------
# nros_resolve_corrosion()
#
# Make `corrosion_import_crate()` available, preferring the provisioned SDK copy
# over the network, and REPORT what was resolved. Idempotent: a configure that
# already has Corrosion (a parent project, or a second call) reports and
# returns.
#
# A macro, not a function: `find_package` / `FetchContent_MakeAvailable` define
# commands and targets that must land in the CALLER's scope.
# --------------------------------------------------------------------------
macro(nros_resolve_corrosion)
    if(COMMAND corrosion_import_crate)
        if(NOT NROS_CORROSION_REPORTED)
            # `Corrosion_VERSION` is a NORMAL variable, so it does not reach a
            # sibling directory scope — a workspace leaf that imports nano-ros
            # via `add_subdirectory` sees the command and not the version. The
            # cache copies below are what make the already-loaded line say
            # something; without them it printed "unknown / topology unknown",
            # which is the non-answer this module exists to stop giving.
            if(DEFINED NROS_CORROSION_VERSION_CACHED)
                nros_report_corrosion("already-loaded (${NROS_CORROSION_ORIGIN_CACHED})"
                                      "${NROS_CORROSION_VERSION_CACHED}"
                                      "${NROS_CORROSION_LOCATION_CACHED}")
            else()
                nros_report_corrosion("already-loaded" "${Corrosion_VERSION}"
                                      "${Corrosion_DIR}")
            endif()
            set(NROS_CORROSION_REPORTED ON)
        endif()
    else()
        _nros_corrosion_prefixes(_nros_corrosion_candidates)
        if(_nros_corrosion_candidates)
            list(APPEND CMAKE_PREFIX_PATH ${_nros_corrosion_candidates})
        endif()
        find_package(Corrosion QUIET)
        if(Corrosion_FOUND)
            _nros_corrosion_remember("SDK store" "${Corrosion_VERSION}" "${Corrosion_DIR}")
            nros_report_corrosion("SDK store" "${Corrosion_VERSION}" "${Corrosion_DIR}")
        else()
            _nros_corrosion_pin(_nros_corrosion_tag)
            message(STATUS
                "nano-ros: Corrosion not provisioned — fetching ${_nros_corrosion_tag} "
                "from git. Install it offline-safe with:  nros setup --tool corrosion")
            include(FetchContent)
            FetchContent_Declare(Corrosion
                GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
                GIT_TAG        ${_nros_corrosion_tag}
            )
            FetchContent_MakeAvailable(Corrosion)
            _nros_corrosion_remember("FetchContent" "${_nros_corrosion_tag}"
                                     "${corrosion_SOURCE_DIR}")
            nros_report_corrosion("FetchContent" "${_nros_corrosion_tag}"
                                  "${corrosion_SOURCE_DIR}")
            unset(_nros_corrosion_tag)
        endif()
        set(NROS_CORROSION_REPORTED ON)
        unset(_nros_corrosion_candidates)
    endif()
endmacro()
