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
# two candidate rules in step. NOTE `check-cmake-corrosion-prefix` no longer
# exists — phase-365 W3a (`24519cac8`) retired it deliberately when the prefix
# became CONSTRUCTED from the pin rather than globbed, so there is no gate on
# this wiring and the two rules are kept in step by hand (issue 0625).

include_guard(GLOBAL)

# `nros_resolve_cli` — the SHARED CLI resolver (issues 0219 / 0325). Included at
# FILE scope: inside a function `CMAKE_CURRENT_LIST_DIR` names the CALLER's file,
# and the frame pop drops what the include defined.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")

# CACHE INTERNAL, not a plain `set`: this module is `include()`d from inside
# functions, and a normal variable set at file scope is gone when that frame
# pops (the `_NROS_ENTRY_DIR` pattern — it broke every freertos workspace
# member's `configure_file` once already).
set(_NROS_CORROSION_MODULE_DIR "${CMAKE_CURRENT_LIST_DIR}"
    CACHE INTERNAL "directory holding NanoRosCorrosion.cmake")

# _nros_corrosion_stale_caches(out_var)
#
# Issue 0622 — every `CMakeCache.txt` in this checkout that recorded a legacy
# Corrosion prefix. A cache is authoritative for the NEXT configure of its tree,
# so these are what keep a stale resolution alive after the pin is installed.
#
# This exists because the remedy used to hand the reader a glob to run. The gate
# already knows what it rejected and can look, so it should: 0622's own words
# are that an incomplete remedy at that moment "converts 'I do not know what to
# do' into 'I did what it said and it is still broken'". A LIST can be checked
# off; a glob has to be trusted.
#
# Both populations, deliberately: the workspace fixture trees AND the example
# leaves. The first version of the remedy named only the workspace trees, which
# is how 0622 was filed at all.
function(_nros_corrosion_stale_caches out_var)
    set(_root "${_NROS_CORROSION_MODULE_DIR}/..")
    set(_caches "")
    file(GLOB _found
        "${_root}/examples/workspaces/*/build*/CMakeCache.txt"
        "${_root}/examples/*/*/*/build*/CMakeCache.txt")
    foreach(_c IN LISTS _found)
        # LIMIT_COUNT 1 — presence is the question, and these files are large.
        file(STRINGS "${_c}" _hit REGEX "corrosion/v?0\\.[0-5]\\." LIMIT_COUNT 1)
        if(_hit)
            file(RELATIVE_PATH _rel "${_root}" "${_c}")
            list(APPEND _caches "${_rel}")
        endif()
    endforeach()
    set(${out_var} "${_caches}" PARENT_SCOPE)
endfunction()

# _nros_corrosion_stale_cache_report(out_var)
# The stale-cache paragraph for a remedy message: an explicit list when there is
# one, and an explicit "none" when there is not — so a reader whose problem is
# NOT a stale cache learns that here rather than after deleting 62 files.
function(_nros_corrosion_stale_cache_report out_var)
    _nros_corrosion_stale_caches(_stale)
    list(LENGTH _stale _n)
    if(_n EQUAL 0)
        # ONE quoted argument. Several arguments make a LIST, and `message()`
        # renders a list joined by `;` — which is what the first cut of this
        # printed, mid-sentence, in the middle of the remedy.
        set(${out_var}
            "  No CMakeCache.txt in this checkout names a legacy prefix, so a stale\n  cache is NOT what is pinning this resolution — look at the resolution\n  path itself (an `add_subdirectory` import never consults the SDK\n  prefixes).\n"
            PARENT_SCOPE)
        return()
    endif()
    set(_shown "${_stale}")
    set(_tail "")
    if(_n GREATER 10)
        list(SUBLIST _shown 0 10 _shown)
        math(EXPR _rest "${_n} - 10")
        set(_tail "    … and ${_rest} more\n")
    endif()
    string(REPLACE ";" "\n    " _lines "${_shown}")
    set(${out_var}
        "  ${_n} CMakeCache.txt in this checkout still name a legacy prefix. A cache\n  is authoritative for the next configure of its tree, so these must go —\n  deleting the CMakeCache.txt is enough, the trees themselves need not:\n    ${_lines}\n${_tail}"
        PARENT_SCOPE)
endfunction()

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
# phase-365 W3a — CONSTRUCT the prefix; never search the store.
#
# `nros sdk-path corrosion` joins the store root to the version this project
# PINS in `nros-sdk-index.toml`. That join is the same function `nros setup`
# used to write the directory (`sdk_store::tool_dir`), so consumption and
# provisioning cannot disagree.
#
# What this replaces, and why it had to go: a `file(GLOB)` of the store sorted
# `COMPARE NATURAL ORDER DESCENDING`. Its ordering was correct and verified —
# and it still lost, because the store is SHARED while the pin is PER-PROJECT,
# so "newest installed" answers a different question than "what this project
# wants", and a third route bypassed it entirely. Measured 2026-08-16 in a tree
# pinning 0.6.1-nros1: 155 resolutions of 0.5.1 against 28 of 0.6.1 (issue 0625).
#
# Empty output means the CLI could not answer (not on PATH, or no such tool);
# the caller falls through to FetchContent — the correct-version path, and since
# issue 1060 offline after its first run on the host: it fetches a COMMIT (not a
# tag) into the shared cache at `$NROS_HOME/fetch`, and every later build dir
# reuses that without touching the network. Still the fallback, not the
# supported path — the SDK store's `dist` assets are sha256-verified, a git
# clone is not.
function(_nros_corrosion_store_dir out_var)
    set(${out_var} "" PARENT_SCOPE)
    # Issue 0754 — prefer the CLI the build was HANDED (`-D_NANO_ROS_
    # CODEGEN_TOOL=<path>`, the canonical lane's spelling) over a fresh
    # PATH discovery: a second independent `find_program` re-introduces
    # the 0663/0625 shadowing class (a stale `nros` earlier on PATH
    # answering for the one the build validated).
    #
    # Issue 0726 — the fallback was a SIXTH bespoke `find_program`, and it could
    # never fire:
    #
    #     set(_NROS_CLI "")          # normal variable, now DEFINED
    #     find_program(_NROS_CLI nros)   # <-- no-op; `_NROS_CLI` stays ""
    #
    # `find_program` does nothing when a variable of that name is already
    # defined, and an empty string counts as defined. So every configure that
    # did NOT pre-set `_NANO_ROS_CODEGEN_TOOL` returned empty here, skipped
    # `find_package` entirely, and fell through to the FetchContent branch —
    # cloning Corrosion from GitHub at configure time while a provisioned copy
    # sat in the SDK store. That made the whole 0500 ordering apparatus dead
    # code on this path and made a configure REQUIRE the network.
    #
    # `nros_resolve_cli` (issues 0219 / 0325) is the shared resolver that
    # already owns this precedence — including `_NANO_ROS_CODEGEN_TOOL` first,
    # which is 0754's requirement — plus its own stale-path drop. Its own
    # comment records that it exists to stop the fifth bespoke copy; this was
    # the sixth. OPTIONAL because a missing CLI is a legitimate fall-through to
    # FetchContent here, not the FATAL_ERROR other callers want.
    set(_NROS_CLI "")
    if(DEFINED _NANO_ROS_CODEGEN_TOOL AND EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        set(_NROS_CLI "${_NANO_ROS_CODEGEN_TOOL}")
    else()
        nros_resolve_cli(_NROS_CLI OPTIONAL CONTEXT "nros_resolve_corrosion")
    endif()
    if(NOT _NROS_CLI OR _NROS_CLI STREQUAL "NOTFOUND")
        return()
    endif()
    execute_process(
        COMMAND "${_NROS_CLI}" sdk-path corrosion
        WORKING_DIRECTORY "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/.."
        OUTPUT_VARIABLE _dir
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
        RESULT_VARIABLE _rc)
    if(_rc EQUAL 0 AND IS_DIRECTORY "${_dir}")
        set(${out_var} "${_dir}" PARENT_SCOPE)
        return()
    endif()
    # Issue 0628 — say WHICH paths were tried before falling through.
    #
    # "not installed" and "installed where I did not look" printed the same
    # nothing here, and the difference is the whole bug: on a host carrying only
    # the legacy FLAT layout the configure fetched Corrosion from the network
    # while advising `nros setup --tool corrosion` — the step already done. The
    # CLI resolves both shapes now, so this branch means genuinely absent; a
    # STATUS line makes that checkable instead of inferred.
    if(_rc EQUAL 0 AND _dir)
        message(STATUS "nano-ros: no Corrosion at the pinned prefix (${_dir}) "
                       "— falling through to FetchContent")
    endif()
endfunction()

# The FetchContent fallback pin — a COMMIT, with the tag beside it (issue 1060).
#
# A tag is a ref on a server we do not control. If upstream retags, this build
# switches to a different tree and NO FILE HERE CHANGES — no diff, no review, no
# gate. So `GIT_TAG` below carries the digest. The tag stays because it is what a
# human reads, what `nros-sdk-index.toml` records, and what
# `just/workspace.just`'s `CORROSION_VERSION` and `nros setup --tool corrosion`
# both clone by.
#
# The commit was resolved honestly:
#
#   git ls-remote https://github.com/corrosion-rs/corrosion refs/tags/v0.6.1
#   1499b14e4906a2890f5cee1547c8848db261753d  refs/tags/v0.6.1
#
# There is NO peeled `refs/tags/v0.6.1^{}` line, which is `ls-remote` saying the
# tag is LIGHTWEIGHT — the ref is the commit, not a tag object wrapping one.
# Confirmed rather than assumed: `git cat-file -t 1499b14e…` answers `commit`,
# and that commit is "Prepare v0.6.1 release (#656)", 2026-01-17. Had it been an
# annotated tag, the tag object's own sha here would make CMake check out
# something that is not a commit.
#
# WHY THE COMMIT LIVES HERE AND THE TAG LIVES IN THE INDEX. The natural home for
# both is `[tool.corrosion]` in `nros-sdk-index.toml`, one line apart. That table
# deserialises into `ToolPackage`, which is
# `#[serde(deny_unknown_fields)]`
# (`packages/cli/nros-cli-core/src/orchestration/sdk_index.rs`), so an extra key
# fails EVERY `SdkIndex::load` — `nros setup`, `nros doctor`, the metadata build
# — and adding it means a schema change in a crate this module does not own.
# The two halves still cannot drift silently: a checkout whose index names a
# DIFFERENT tag than the commit below is a configure FATAL_ERROR, which is the
# same "both move together or neither does" a single table would have bought.
# And when the schema does grow `upstream_commit`, the index becomes the SSoT
# with no change here — the reader below already prefers it.
function(_nros_corrosion_pin out_commit out_tag)
    set(_tag "v0.6.1")
    set(_commit "1499b14e4906a2890f5cee1547c8848db261753d")
    set(_index "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../nros-sdk-index.toml")
    if(EXISTS "${_index}")
        file(READ "${_index}" _index_raw)
        # `[tool.corrosion]` … `upstream = "v0.6.1"`, up to the next section head.
        if(_index_raw MATCHES "\\[tool\\.corrosion\\][^[]*upstream[ \t]*=[ \t]*\"([^\"]+)\"")
            set(_index_tag "${CMAKE_MATCH_1}")
            if(_index_raw MATCHES
                    "\\[tool\\.corrosion\\][^[]*upstream_commit[ \t]*=[ \t]*\"([0-9a-fA-F]+)\"")
                # The index carries both — it is the SSoT and the literals above
                # are only the vendored-without-index fallback.
                set(_tag "${_index_tag}")
                set(_commit "${CMAKE_MATCH_1}")
            elseif(NOT "${_index_tag}" STREQUAL "${_tag}")
                message(FATAL_ERROR
                    "nano-ros: the Corrosion pin is half-applied. "
                    "${_index} says `upstream = \"${_index_tag}\"`, this module "
                    "pins ${_tag} = ${_commit}.\n"
                    "  A tag alone is not a pin (issue 1060), so both move "
                    "together: resolve the new tag with\n"
                    "    git ls-remote https://github.com/corrosion-rs/corrosion "
                    "refs/tags/${_index_tag}\n"
                    "  (take the peeled `^{}` line if there is one — that is the "
                    "commit; the bare line would be the tag object) and update "
                    "`_nros_corrosion_pin()` in this file.")
            endif()
        endif()
    endif()
    set(${out_commit} "${_commit}" PARENT_SCOPE)
    set(${out_tag} "${_tag}" PARENT_SCOPE)
endfunction()

# --------------------------------------------------------------------------
# The host-level FetchContent cache (RFC-0087 D5, issue 1060).
#
# CMake's default `FETCHCONTENT_BASE_DIR` is `<build>/_deps`, so a fetch is once
# per BUILD DIRECTORY. Issue 0500 measured 159 build dirs in one checkout each
# carrying their own resolved Corrosion (139 on 0.5.1, 20 on 0.6.1). A shared
# cache makes it once per HOST.
#
# WHERE. `$NROS_HOME/fetch`, default `~/.nros/fetch` — the sibling of the SDK
# store `$NROS_HOME/sdk` that `_nros_corrosion_store()` above already reads.
# `$NROS_HOME` is this tree's one convention for host-level state that outlives a
# checkout and is shared by every build dir in it (the SDK store, `bin/`), and a
# resolved upstream source tree is exactly that. A per-repo cache would still be
# N clones for N checkouts and would need a `.gitignore` row; the store keeps the
# worktree clean.
#
# OVERRIDE. `-DNROS_FETCH_CACHE=<dir>` (cache variable) beats `NROS_FETCH_CACHE`
# in the environment, which beats `$NROS_HOME/fetch`. `OFF`/`0`/`NO`/empty turns
# sharing off and restores CMake's per-build `<build>/_deps`. A location that
# cannot be created or written is NOT fatal: it says so and falls back to the
# same per-build default, because an unwritable `$HOME` is a legitimate CI shape
# and a cache is an optimisation, never a requirement.
function(_nros_corrosion_fetch_cache out_var)
    set(${out_var} "" PARENT_SCOPE)
    if(DEFINED NROS_FETCH_CACHE)
        set(_dir "${NROS_FETCH_CACHE}")
    elseif(DEFINED ENV{NROS_FETCH_CACHE})
        set(_dir "$ENV{NROS_FETCH_CACHE}")
    elseif(DEFINED ENV{NROS_HOME})
        set(_dir "$ENV{NROS_HOME}/fetch")
    elseif(DEFINED ENV{HOME})
        set(_dir "$ENV{HOME}/.nros/fetch")
    else()
        return()
    endif()
    if(NOT _dir OR _dir STREQUAL "OFF" OR _dir STREQUAL "0" OR _dir STREQUAL "NO")
        message(STATUS "nano-ros: shared fetch cache disabled — Corrosion will "
                       "be fetched into this build dir's _deps")
        return()
    endif()
    # Ask, do not assume. `file(MAKE_DIRECTORY)` and `file(TOUCH)` are FATAL on
    # failure, which would turn "no writable HOME" into a dead configure; the
    # `cmake -E` forms report a status instead.
    execute_process(COMMAND "${CMAKE_COMMAND}" -E make_directory "${_dir}"
                    RESULT_VARIABLE _rc OUTPUT_QUIET ERROR_QUIET)
    if(NOT _rc EQUAL 0)
        message(STATUS "nano-ros: fetch cache ${_dir} is not creatable — using "
                       "this build dir's _deps instead")
        return()
    endif()
    execute_process(COMMAND "${CMAKE_COMMAND}" -E touch "${_dir}/.nros-writable"
                    RESULT_VARIABLE _rc OUTPUT_QUIET ERROR_QUIET)
    if(NOT _rc EQUAL 0)
        message(STATUS "nano-ros: fetch cache ${_dir} is not writable — using "
                       "this build dir's _deps instead")
        return()
    endif()
    set(${out_var} "${_dir}" PARENT_SCOPE)
endfunction()

# Is the cache already holding the PINNED commit? Three answers, because the
# third is the one a version bump produces and it must not be confused with the
# second: `empty` (nothing there), `pinned` (the commit we want — reusable
# offline), `stale` (some other commit, from a pin that has since moved).
#
# The check is a digest comparison, not "a directory exists". That is the point
# of pinning a commit at all: the cache is the one copy every build dir in the
# host shares, so "which tree is this" has to be answerable without the network.
function(_nros_corrosion_cache_state cache commit out_state)
    set(_src "${cache}/corrosion-src")
    if(NOT EXISTS "${_src}/CMakeLists.txt")
        set(${out_state} "empty" PARENT_SCOPE)
        return()
    endif()
    find_package(Git QUIET)
    if(NOT GIT_EXECUTABLE)
        set(${out_state} "stale" PARENT_SCOPE)
        return()
    endif()
    execute_process(COMMAND "${GIT_EXECUTABLE}" -C "${_src}" rev-parse HEAD
                    OUTPUT_VARIABLE _head OUTPUT_STRIP_TRAILING_WHITESPACE
                    RESULT_VARIABLE _rc ERROR_QUIET)
    if(_rc EQUAL 0 AND "${_head}" STREQUAL "${commit}")
        set(${out_state} "pinned" PARENT_SCOPE)
    else()
        set(${out_state} "stale" PARENT_SCOPE)
    endif()
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
    # phase-365 — name the SOURCE DIR that resolved it. Three rounds of
    # narrowing-by-inspection failed to find the route that still resolves a
    # legacy copy (64 of 86 resolutions in the 2026-08-16 lane=all), because a
    # bare version line says WHAT was resolved and never WHO asked.
    message(STATUS
        "nano-ros: Corrosion ${version} via ${origin} [${_topology}] — ${location} "
        "(asked by ${CMAKE_CURRENT_SOURCE_DIR})")

    # Issue 0493 — REFUSE the broken topology, do not merely narrate it.
    #
    # This line classified `< 0.6.0` as a link risk and then configured anyway,
    # so the only thing standing between a host and the duplicate-symbol failure
    # was the SDK store happening to hold a newer copy. The store ACCUMULATES
    # (issue 0500), and the ordering rule it is sorted by is newest-FIRST, not
    # a version FLOOR: a host with only 0.5.1 installed resolves
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
    # DOWNGRADED TO A WARNING 2026-08-16, deliberately, hours after landing it
    # as FATAL_ERROR. As a hard failure it took out four fixture families
    # (nuttx, freertos, threadx_linux, native) in one sweep, because the legacy
    # copy is not rare here: a `lane=all` configure produced 155 resolutions of
    # 0.5.1 against 28 of 0.6.1.
    #
    # Those 155 are NOT stale caches — the leaf build dirs hold no
    # `Corrosion_DIR` at all. They arrive through an `add_subdirectory` path
    # ("Using Corrosion as a subdirectory") that never consults
    # `_nros_corrosion_prefixes`, whose own ordering is correct and verified:
    #
    #     candidate: ~/.nros/sdk/corrosion/0.6.1-nros1
    #     candidate: ~/.nros/sdk/corrosion/0.5.1-nros1
    #     candidate: ~/.nros/sdk/corrosion
    #
    # So the real defect is a second resolution path that bypasses the ordering,
    # and refusing to configure until it is fixed blocks every consumer for a
    # duplication that only bites when TWO workspace roots share the dir. The
    # warning keeps the finding visible — it is what surfaced the 155/28 split —
    # without holding the tree hostage to someone else's lane.
    #
    # Promote back to FATAL_ERROR once the `add_subdirectory` path resolves
    # newest-first; `-DNROS_STRICT_CORROSION=ON` opts in meanwhile.
    # Computed once for whichever arm fires; the scan only runs on the legacy
    # version, so a healthy configure never pays for it.
    if(version MATCHES "^v?0\\.[0-5]\\.")
        _nros_corrosion_stale_cache_report(_stale_report)
    endif()
    if(version MATCHES "^v?0\\.[0-5]\\." AND NROS_STRICT_CORROSION
            AND NOT NROS_ALLOW_LEGACY_CORROSION)
        message(FATAL_ERROR
            "nano-ros: Corrosion ${version} shares ONE cargo target-dir across "
            "workspace roots.\n"
            "  resolved: ${version} via ${origin} — ${location}\n"
            "That topology gives the same crate two `-C metadata` identities in one "
            "`deps/`, so every `#[no_mangle]` export collides at link and "
            "`nros-platform`'s single `#[global_allocator]` is defined twice "
            "(issues 0493, 0616).\n"
            "Fix: provision the pinned copy, then clear EVERY tree that cached the "
            "old topology in its CMakeCache —\n"
            "    nros setup --tool corrosion\n"
            "${_stale_report}"
            "Override for a deliberate experiment: -DNROS_ALLOW_LEGACY_CORROSION=ON")
    elseif(version MATCHES "^v?0\\.[0-5]\\.")
        # Issue 0622 fixed the remedy in the FATAL arm only — and this arm is the
        # one a reader actually reaches, since the fatal branch became opt-in
        # (`NROS_STRICT_CORROSION`) hours after landing. "Remove its build dir"
        # is the same incomplete instruction 0622 was filed about, so it gets
        # the same measured list rather than a second, shorter spelling of the
        # advice. Fix the CLASS, not the reported site.
        message(WARNING
            "nano-ros: Corrosion ${version} shares ONE cargo target-dir across workspace "
            "roots (issues 0493, 0616). Harmless for a single-root configure; a link "
            "with two roots duplicates every nros crate.\n"
            "Provision the pin: `nros setup --tool corrosion`\n"
            "${_stale_report}"
            "Make this fatal with -DNROS_STRICT_CORROSION=ON.")
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
# --------------------------------------------------------------------------
# nros_share_corrosion_cargo_dir()
#
# Issue 0805 — collapse the per-leaf cargo target dir onto a SHARED one.
#
# Corrosion computes its `--target-dir` as
#
#     ${CMAKE_BINARY_DIR}/cargo/<workspace-folder>_<hash-of-manifest-path>
#
# The hash is of the WORKSPACE MANIFEST PATH, so it is identical for every
# consumer of this repo (`nano-ros_1147c` everywhere). Only `CMAKE_BINARY_DIR`
# differs — and a C/C++ example leaf is a standalone cmake project, so every
# leaf gets its own and rebuilds the same staticlib. Measured on
# `threadx_riscv64`: 21 fresh `libnros_c.a` and 14 `libnros_cpp.a` in ONE stage
# run, five concurrent cargos with byte-identical arguments, ~1.2 GB per leaf.
#
# sccache cannot absorb it — it does not cache `--crate-type=staticlib`
# (`Non-cacheable reasons: crate-type`), which is exactly the artifact.
#
# Corrosion 0.6.1 exposes no knob for the directory (it is a plain local), and
# its `CARGO_FLAGS` hook is not a substitute: a second `--target-dir` would move
# cargo's OUTPUT while Corrosion kept looking for the artifact at the path it
# derived, so the build would fail to find its own byproduct. Redirecting the
# path Corrosion already computes is the only override point, hence a symlink.
#
# SAFETY — the caller asserts, this function does not guess.
#
# Two leaves may share a directory only if their cargo inputs are IDENTICAL.
# They are not merely "similar": cargo uplifts the final artifact to an
# UNHASHED name (`libnros_c.a`), so two different feature sets sharing a
# directory would overwrite each other's archive and a leaf would silently link
# the wrong one. That is the 0500 / 0616 failure mode, and it is diagnosed as a
# duplicate-symbol or wrong-arch link error a long way from its cause.
#
# Concretely, `nros_feature_set()` derives the crate's features from RMW,
# PLATFORM and **CAPABILITIES** — and capabilities are per-leaf. So two
# leaves on the same platform and RMW can still want different `nros-c`
# features, which is why keying on "platform + rmw" would be WRONG. The key
# below is the full input set that `nros_feature_set` is a function of, so
# equal keys imply equal features by construction rather than by inspection.
#
# This is OFF unless a caller passes `-DNROS_SHARED_CARGO_ROOT=<dir>`; the
# directory under it is chosen by KEY, which the caller must supply.
# phase-400 W5.b — `nros_shared_cargo_dir()` moved to its own module so the
# Zephyr lane can reach it without pulling in Corrosion. Included at FILE scope
# for the reason the header of this file gives.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosSharedCargoDir.cmake")

# Issue 0945 item 1 — captured at FILE scope, CACHE INTERNAL, because
# `nros_assert_shared_cargo_dir_used()` below is a function and a plain
# `set(_X ${CMAKE_CURRENT_LIST_DIR})` does not survive the frame pop (the
# `_NROS_ENTRY_DIR` pattern; it broke every freertos workspace member once).
set(_NROS_SHARED_CARGO_CHECK_SH
    "${CMAKE_CURRENT_LIST_DIR}/../scripts/check-shared-cargo-dir-used.sh"
    CACHE INTERNAL "issue 0945 — shared-cargo-dir redirect assertion")


function(nros_share_corrosion_cargo_dir)
    # Issue 0945 item 1 — report to the caller whether the redirect is ACTUALLY
    # in place, so it can arm `nros_assert_shared_cargo_dir_used()`. Empty by
    # default: every path below that does not end with a symlink at the chosen
    # directory leaves it empty, and the caller must not assert on those. The
    # degrade branch ("a real `cargo` directory from an earlier build") is a
    # supported state, not a violation, and arming the check there would turn
    # every pre-existing build dir red.
    set(NROS_SHARED_CARGO_DIR "" PARENT_SCOPE)
    # Delegate the keying to `nros_shared_cargo_dir()` above — ONE normalise,
    # hash and record. This function's own job is the part Corrosion forces:
    # redirecting a `--target-dir` it computes itself, which only a symlink can
    # do. See the safety notes above the helper.
    nros_shared_cargo_dir(NROS_SHARED_CARGO_DIR ${ARGN})
    if(NOT NROS_SHARED_CARGO_DIR)
        return()
    endif()
    set(_key_text "${NROS_SHARED_CARGO_DIR_KEY_TEXT}")
    set(_link "${CMAKE_BINARY_DIR}/cargo")
    # Record the key text BEFORE the checks below, not after the symlink is
    # created. The mismatch branch used to fire first and print two HASHES with
    # no way to see what differed — the same "two hex strings nobody can compare
    # by eye" problem CLAUDE.md records for submodule pins. Both sides of a
    # mismatch must be readable on disk.
    # The helper already created the target directory and recorded the key.
    # Both matter here: a symlink to a MISSING directory is dangling, and
    # `mkdir` on a dangling link fails with EEXIST — cargo then reports
    # `failed to create directory ... File exists (os error 17)`, naming the
    # link rather than the absent target.

    if(IS_SYMLINK "${_link}")
        # Re-configure of a build dir already sharing. Honour it only if it
        # points where THIS configure was told to point: a stale link is a leaf
        # silently building into another key's directory.
        file(READ_SYMLINK "${_link}" _current)
        if(NOT IS_ABSOLUTE "${_current}")
            get_filename_component(_current "${CMAKE_BINARY_DIR}/${_current}" ABSOLUTE)
        endif()
        get_filename_component(_want "${NROS_SHARED_CARGO_DIR}" ABSOLUTE)
        if(NOT _current STREQUAL _want)
            set(_prev_key "unknown")
        if(EXISTS "${_current}.key")
            file(READ "${_current}.key" _prev_key)
            string(STRIP "${_prev_key}" _prev_key)
        endif()
        # RE-POINT rather than fail. A symlink is not data: the old key's
        # directory keeps its contents for whoever still uses it, and this leaf
        # simply starts using the directory its CURRENT configuration names.
        # The ambiguity this guard exists to prevent — two keys' artifacts live
        # in one build dir — cannot arise, because the dir serves exactly one
        # key at a time.
        #
        # It used to FATAL and demand a wipe. That is wrong for the common
        # case: any change to a key input (a profile, a capability) would force
        # every leaf's build dir to be deleted, which is a large cost for a
        # rename. Keep the message, drop the demand.
        file(REMOVE "${_link}")
        file(CREATE_LINK "${NROS_SHARED_CARGO_DIR}" "${_link}" SYMBOLIC RESULT _rc)
        if(NOT _rc EQUAL 0)
            message(FATAL_ERROR
                "nano-ros: could not re-point ${_link} -> "
                "${NROS_SHARED_CARGO_DIR} (${_rc}).")
        endif()
        message(STATUS
            "nano-ros: shared-cargo key changed, re-pointing ${_link}\n"
            "  was: ${_prev_key}\n"
            "  now: ${_key_text}")
        set(NROS_SHARED_CARGO_DIR "${NROS_SHARED_CARGO_DIR}" PARENT_SCOPE)
        return()
        endif()
        set(NROS_SHARED_CARGO_DIR "${NROS_SHARED_CARGO_DIR}" PARENT_SCOPE)
        return()
    endif()

    if(EXISTS "${_link}")
        # A real directory from an earlier non-shared configure. DEGRADE, do not
        # fail: this is EVERY build dir that predates this feature, so a
        # FATAL_ERROR here would break incremental builds for everyone who pulls
        # it, in exchange for a speedup. Not sharing is exactly the old
        # behaviour — correct, just slower — which makes it a safe fallback, and
        # the STATUS line keeps it from being a silent one.
        #
        # Deleting it is not on the table either: it holds build output this
        # function did not create.
        message(STATUS
            "nano-ros: NOT sharing the Corrosion cargo dir — ${_link} is a real "
            "directory from an earlier build. Remove this build directory to "
            "enable sharing (issue 0805).")
        return()
    endif()

    file(CREATE_LINK "${NROS_SHARED_CARGO_DIR}" "${_link}" SYMBOLIC RESULT _rc)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: could not link ${_link} -> ${NROS_SHARED_CARGO_DIR} "
            "(${_rc}).")
    endif()
    message(STATUS
        "nano-ros: sharing Corrosion cargo dir -> ${NROS_SHARED_CARGO_DIR}")
    set(NROS_SHARED_CARGO_DIR "${NROS_SHARED_CARGO_DIR}" PARENT_SCOPE)
endfunction()

# --------------------------------------------------------------------------
# nros_assert_shared_cargo_dir_used(<shared-dir> <corrosion-target>)
#
# Issue 0945 item 1 — make a Corrosion move FAIL instead of silently degrading.
#
# The symlink above redirects a `--target-dir` Corrosion derives privately, and
# Corrosion offers no knob for it — still true on upstream `master` as of
# 2026-08-31, not merely on the pinned v0.6.1. So the redirect cannot be
# retired; what it can be given is a witness. If a future Corrosion moves that
# path, the link points where cargo no longer writes, the build SUCCEEDS, and
# the only symptom is six platforms getting slower with nobody watching the
# number.
#
# This asserts the RESULT rather than re-deriving the formula: after the target
# builds, the shared directory must hold this artifact, and the redirect must
# still be a symlink at the directory this configure chose. A second copy of
# Corrosion's path rule would drift from it silently, which is the defect and
# not the fix. See the script for what the check can and cannot catch.
#
# Only call this when `nros_share_corrosion_cargo_dir()` reported a live
# redirect — it leaves `NROS_SHARED_CARGO_DIR` empty on the supported degrade
# paths (sharing not requested; a real `cargo` directory from an earlier
# build), and asserting there would turn every pre-existing build dir red.
# --------------------------------------------------------------------------
function(nros_assert_shared_cargo_dir_used shared_dir target)
    if(NOT shared_dir)
        return()
    endif()
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_assert_shared_cargo_dir_used: no target '${target}'. Call "
            "this AFTER corrosion_import_crate().")
    endif()
    set(_stamp "${CMAKE_CURRENT_BINARY_DIR}/nros-shared-cargo-dir-${target}.checked")
    # A real OUTPUT with a file-level DEPENDS on the artifact, not a POST_BUILD:
    # issue 0268's rule. The check must re-run whenever cargo produces a new
    # archive, and only then — a cached build has nothing new to witness.
    add_custom_command(
        OUTPUT "${_stamp}"
        COMMAND bash "${_NROS_SHARED_CARGO_CHECK_SH}"
            --shared-dir "${shared_dir}"
            --link "${CMAKE_BINARY_DIR}/cargo"
            --artifact "$<TARGET_FILE:${target}>"
            --label "${target}"
        COMMAND "${CMAKE_COMMAND}" -E touch "${_stamp}"
        DEPENDS "$<TARGET_FILE:${target}>"
        COMMENT "nano-ros: checking cargo wrote ${target} into the shared dir (issue 0945)"
        VERBATIM)
    add_custom_target(nros_shared_cargo_dir_check_${target} ALL DEPENDS "${_stamp}")
endfunction()

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
        # NO_DEFAULT_PATH: look in the ONE constructed prefix and nowhere else.
        # Without it cmake would still consult the environment and could find a
        # copy this project never pinned — the defect, reintroduced by omission.
        _nros_corrosion_store_dir(_nros_corrosion_prefix)
        if(_nros_corrosion_prefix)
            # A CACHED `Corrosion_DIR` outranks PATHS and NO_DEFAULT_PATH:
            # `find_package` short-circuits on it and never looks at either. So
            # constructing the prefix is only HALF the principle — a build dir
            # configured before the pin moved keeps answering with the old
            # version forever, and nothing says so.
            #
            # Measured 2026-08-16 across this tree: 139 example build dirs
            # cached 0.5.1 against 20 on 0.6.1, which is the whole of the
            # 64-of-86 residue left after the constructor landed. Clearing 139
            # build dirs by hand is not a fix; the resolver self-heals instead.
            #
            # Drop a cached value that is not inside the constructed prefix and
            # let the search below re-cache the right one. We KNOW where the
            # tool is — a cache is not a second opinion worth honouring.
            if(DEFINED CACHE{Corrosion_DIR}
                    AND NOT "${Corrosion_DIR}" MATCHES "^${_nros_corrosion_prefix}/")
                message(STATUS
                    "nano-ros: dropping stale Corrosion_DIR (${Corrosion_DIR}) — "
                    "this project pins ${_nros_corrosion_prefix}")
                unset(Corrosion_DIR CACHE)
            endif()
            find_package(Corrosion QUIET
                PATHS "${_nros_corrosion_prefix}" NO_DEFAULT_PATH)
        endif()
        if(Corrosion_FOUND)
            _nros_corrosion_remember("SDK store" "${Corrosion_VERSION}" "${Corrosion_DIR}")
            nros_report_corrosion("SDK store" "${Corrosion_VERSION}" "${Corrosion_DIR}")
        else()
            _nros_corrosion_pin(_nros_corrosion_commit _nros_corrosion_tag)
            _nros_corrosion_fetch_cache(_nros_corrosion_cache)
            set(_nros_corrosion_fetch_args "")
            set(_nros_corrosion_fetch_origin "FetchContent")
            if(_nros_corrosion_cache)
                _nros_corrosion_cache_state("${_nros_corrosion_cache}"
                                            "${_nros_corrosion_commit}"
                                            _nros_corrosion_cache_state_v)
                if(_nros_corrosion_cache_state_v STREQUAL "stale")
                    # The pin moved (or something else wrote here). Clear BOTH
                    # halves: the download stamps live in the subbuild dir, so a
                    # source dir removed without them is never re-cloned.
                    message(STATUS
                        "nano-ros: fetch cache holds a Corrosion that is not "
                        "${_nros_corrosion_tag} (${_nros_corrosion_commit}) — repopulating")
                    file(REMOVE_RECURSE "${_nros_corrosion_cache}/corrosion-src"
                                        "${_nros_corrosion_cache}/corrosion-subbuild")
                    set(_nros_corrosion_cache_state_v "empty")
                endif()
                # What is shared, and what is deliberately not — both measured,
                # not assumed:
                #
                #  * SOURCE_DIR and SUBBUILD_DIR are shared, and they move
                #    together. ExternalProject's clone step keys on a stamp in
                #    the SUBBUILD dir and `rm -rf`s the source before cloning
                #    (`ExternalProject.cmake`, gitclone script). A shared source
                #    with a per-build subbuild would therefore re-clone — and
                #    destroy — the cache on every new build dir.
                #  * BINARY_DIR stays local. It is the `add_subdirectory` binary
                #    dir: two build trees sharing it overwrite each other's
                #    generated rules, and anything Corrosion compiles there
                #    (`CORROSION_NATIVE_TOOLING`) would be one artifact serving
                #    two toolchains — issue 0616 one layer over.
                list(APPEND _nros_corrosion_fetch_args
                    SOURCE_DIR   "${_nros_corrosion_cache}/corrosion-src"
                    SUBBUILD_DIR "${_nros_corrosion_cache}/corrosion-subbuild"
                    BINARY_DIR   "${CMAKE_CURRENT_BINARY_DIR}/_deps/corrosion-build")
                if(_nros_corrosion_cache_state_v STREQUAL "pinned")
                    # `FETCHCONTENT_SOURCE_DIR_<uc>` is the DOCUMENTED
                    # per-dependency form of "do not download": the populate step
                    # is skipped whole, no subbuild is configured, and the
                    # network is not touched. Proven by configuring against this
                    # cache with `GIT_REPOSITORY` pointing at a path that does
                    # not exist — the configure succeeds.
                    #
                    # The global `FETCHCONTENT_FULLY_DISCONNECTED` would do the
                    # same thing project-wide, and is deliberately NOT set: this
                    # module has no business disconnecting a parent project's
                    # other dependencies. The per-dependency form also sidesteps
                    # a hazard the global one keeps — a shared subbuild dir
                    # records the GENERATOR that populated it, so a second build
                    # tree on a different generator hard-fails ("CMake step for
                    # corrosion failed"). Skipping the subbuild skips that too.
                    set(FETCHCONTENT_SOURCE_DIR_CORROSION
                        "${_nros_corrosion_cache}/corrosion-src")
                    set(_nros_corrosion_fetch_origin "FetchContent (host cache)")
                    message(STATUS
                        "nano-ros: Corrosion not provisioned — reusing the host fetch cache "
                        "at ${_nros_corrosion_cache}/corrosion-src (${_nros_corrosion_tag}, "
                        "no network). Install it offline-safe with:  nros setup --tool corrosion")
                else()
                    set(_nros_corrosion_fetch_origin "FetchContent (host cache, populated)")
                    message(STATUS
                        "nano-ros: Corrosion not provisioned — fetching ${_nros_corrosion_tag} "
                        "(${_nros_corrosion_commit}) from git into the host fetch cache at "
                        "${_nros_corrosion_cache}. Later build dirs reuse it offline. "
                        "Install it offline-safe with:  nros setup --tool corrosion")
                endif()
            else()
                message(STATUS
                    "nano-ros: Corrosion not provisioned — fetching ${_nros_corrosion_tag} "
                    "(${_nros_corrosion_commit}) from git into this build dir. "
                    "Install it offline-safe with:  nros setup --tool corrosion")
            endif()
            include(FetchContent)
            # GIT_TAG is the DIGEST, never the tag (issue 1060) — a tag is a ref
            # on someone else's server, so a retag would move this build with no
            # local diff. The human-readable `${_nros_corrosion_tag}` is what the
            # messages and the report line carry.
            FetchContent_Declare(Corrosion
                GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
                GIT_TAG        ${_nros_corrosion_commit}
                ${_nros_corrosion_fetch_args}
            )
            FetchContent_MakeAvailable(Corrosion)
            _nros_corrosion_remember("${_nros_corrosion_fetch_origin}"
                                     "${_nros_corrosion_tag}"
                                     "${corrosion_SOURCE_DIR}")
            nros_report_corrosion("${_nros_corrosion_fetch_origin}"
                                  "${_nros_corrosion_tag}"
                                  "${corrosion_SOURCE_DIR}")
            unset(_nros_corrosion_tag)
            unset(_nros_corrosion_commit)
            unset(_nros_corrosion_cache)
            unset(_nros_corrosion_cache_state_v)
            unset(_nros_corrosion_fetch_args)
            unset(_nros_corrosion_fetch_origin)
        endif()
        set(NROS_CORROSION_REPORTED ON)
        unset(_nros_corrosion_candidates)
    endif()
endmacro()
