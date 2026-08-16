# Cyclone DDS type-support codegen helpers (Phase 117.2 / 117.5).
#
# Two public functions:
#
#   nros_rmw_cyclonedds_idlc_compile(<output_var>
#       IDL_FILE     <path/to/foo.idl>
#       OUTPUT_DIR   <build/dir>
#       [TYPE_NAME   nros_test::msg::TestString]   # optional, for self-reg
#   )
#       Runs Cyclone DDS's `idlc` over a single IDL file and emits
#       `<base>.c` + `<base>.h` + (when TYPE_NAME is given)
#       `<base>_register.c` — a tiny static-init translation unit
#       that registers the generated `dds_topic_descriptor_t` with the
#       backend's runtime registry under TYPE_NAME. Sets <output_var>
#       to the list of generated source files.
#
#   nros_rmw_cyclonedds_add_idl_library(<target>
#       IDL_FILES    <a.idl> [<b.idl> ...]
#       [REGISTER_TYPES <name1=full::cpp::Type1> ...]
#   )
#       Convenience wrapper: produces a STATIC IMPORTED-style library
#       containing the descriptor table for every IDL_FILE, plus
#       optional auto-registration translation units.
#
# Notes:
#  - Cyclone 0.10.5's idlc currently fails when emitting XTypes
#    type-discovery metadata. The `-t` flag skips that section; the
#    produced descriptor still works for pub/sub + services against
#    `rmw_cyclonedds_cpp` peers (the metadata is optional on the wire
#    — peers fall back to typename matching). Phase 117.X.6 makes the
#    `-t` choice opt-out: define `NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO`
#    (cache var or env) to drop `-t` once Cyclone is upgraded past the
#    bug. The helper validates the option at configure time by running
#    `idlc -l c` on a synthetic minimal IDL — if the upstream bug is
#    still present the option is rejected with a clear error rather
#    than silently producing truncated descriptors at build time.
#  - Generated .c / .h are written to `${CMAKE_CURRENT_BINARY_DIR}` so
#    consumers don't have to manage their own scratch dirs.

# Standalone-POSIX consumers link `CycloneDDS::ddsc` (from
# find_package). Embedded consumers (the Zephyr nros module) compile
# the Cyclone DDS sources directly into the app and have no imported
# target — they only need idlc to generate descriptors, which they
# supply via a pre-set `IDLC_EXECUTABLE`. Accept either.
if(NOT TARGET CycloneDDS::ddsc AND NOT IDLC_EXECUTABLE)
    message(FATAL_ERROR
        "NrosRmwCycloneddsTypeSupport.cmake requires CycloneDDS::ddsc "
        "(include it after find_package(CycloneDDS)) or a pre-set "
        "IDLC_EXECUTABLE for direct-compile (embedded) builds.")
endif()

# Issue 0325 — defined BEFORE its first use: the two calls below run at
# file scope during include(), so a definition further down the file is not
# yet parsed and cmake fails with `Unknown CMake command "_nros_find_idlc"`.
# Issue 0601 — EXISTS is not RUNS.
#
# `find_program` searches PATH, and on a host with ROS installed that is
# `/opt/ros/humble/bin/idlc`. That binary links `libiceoryx_binding_c.so` from
# ROS's own lib/, which is on the loader path only when `setup.bash` has been
# sourced INTO THIS BUILD's environment — not merely into the shell that
# launched cmake. So a perfectly findable idlc fails to LOAD, and the first
# `.idl` dies mid-ninja with
#
#     idlc: error while loading shared libraries: libiceoryx_binding_c.so
#     FAILED: [code=127] …
#
# `code=127` reads as "command not found" for a tool that is right there, in a
# build the user cannot see the configure of. The block below already
# re-resolves when the cached path no longer EXISTS; this is the same idea for
# the case where it exists and cannot run.
#
# `-h`, not `--version`: idlc has no `--version`, and a WORKING one exits 1 on
# it. Measured:
#
#     working idlc:  -h -> 0     --version -> 1
#     broken  idlc:  -h -> 127   --version -> 127
#
# So `-h` separates "cannot load" from "loaded fine", and `--version` would
# reject every healthy install.
function(_nros_idlc_runs _path _out_why _out_env)
    # Try the tool as-is first. A healthy install needs nothing, and staying
    # empty there keeps the generated command byte-identical on hosts that were
    # never broken.
    execute_process(
        COMMAND "${_path}" -h
        RESULT_VARIABLE _rc
        OUTPUT_QUIET ERROR_VARIABLE _err)
    if(_rc EQUAL 0)
        set(${_out_why} "" PARENT_SCOPE)
        set(${_out_env} "" PARENT_SCOPE)
        return()
    endif()

    # It did not run. Before giving up, try the ONE thing that is actually
    # knowable from the path itself: a tool installed at `<prefix>/bin/idlc`
    # links libraries under `<prefix>/lib`. That is what ROS's `setup.bash`
    # would have put on the loader path, and deriving it from the tool's own
    # prefix means this works for any prefix, not just /opt/ros/humble.
    get_filename_component(_bin_dir "${_path}" DIRECTORY)
    get_filename_component(_prefix "${_bin_dir}" DIRECTORY)
    set(_lib_dirs "")
    foreach(_cand "${_prefix}/lib" "${_prefix}/lib/${CMAKE_LIBRARY_ARCHITECTURE}")
        if(IS_DIRECTORY "${_cand}")
            list(APPEND _lib_dirs "${_cand}")
        endif()
    endforeach()
    if(_lib_dirs)
        list(JOIN _lib_dirs ":" _ld)
        if(NOT "$ENV{LD_LIBRARY_PATH}" STREQUAL "")
            set(_ld "${_ld}:$ENV{LD_LIBRARY_PATH}")
        endif()
        execute_process(
            COMMAND "${CMAKE_COMMAND}" -E env "LD_LIBRARY_PATH=${_ld}" "${_path}" -h
            RESULT_VARIABLE _rc2
            OUTPUT_QUIET ERROR_QUIET)
        if(_rc2 EQUAL 0)
            message(STATUS
                "nano-ros: idlc at ${_path} needs its own prefix libs; "
                "running it with LD_LIBRARY_PATH=${_ld} (issue 0601)")
            set(${_out_why} "" PARENT_SCOPE)
            set(${_out_env} "LD_LIBRARY_PATH=${_ld}" PARENT_SCOPE)
            return()
        endif()
    endif()

    string(STRIP "${_err}" _err)
    if(_err STREQUAL "")
        set(_err "exit ${_rc}")
    endif()
    set(${_out_why} "${_err}" PARENT_SCOPE)
    set(${_out_env} "" PARENT_SCOPE)
endfunction()

# Issue 0601 — `$NROS_HOME/sdk/cyclonedds/<version>/bin`, NEWEST VERSION FIRST.
#
# The SDK store is what `nros setup --tool cyclonedds` provisions, and it is the
# copy THIS BUILD controls. Without it on the hint list, `find_program` takes the
# first `idlc` on PATH, which on any host with ROS installed is
# `/opt/ros/humble/bin/idlc` — a binary that cannot load its own libraries
# unless ROS's `setup.bash` reached the build's environment. Selection was by
# EXISTENCE where the property that matters is RUNNABILITY.
#
# NEWEST-first for the same reason issue 0500 orders the Corrosion prefixes: the
# store ACCUMULATES, `find_program` takes the first hit, and a provisioning run
# that installs a new version while an old one still wins is the worst shape a
# setup step can have — it reports success and changes nothing.
#
# HINTS, not PATHS: HINTS are searched BEFORE the system PATH and PATHS after
# (CLAUDE.md's `find_program` note). Preferring the provisioned tool is the
# entire point, so it has to be HINTS.
function(_nros_cyclonedds_sdk_bins _out)
    if(DEFINED ENV{NROS_HOME})
        set(_store "$ENV{NROS_HOME}/sdk")
    else()
        set(_store "$ENV{HOME}/.nros/sdk")
    endif()
    set(_dirs "")
    file(GLOB _versioned LIST_DIRECTORIES true "${_store}/cyclonedds/*")
    foreach(_d IN LISTS _versioned)
        if(IS_DIRECTORY "${_d}/bin")
            list(APPEND _dirs "${_d}/bin")
        endif()
    endforeach()
    list(SORT _dirs COMPARE NATURAL ORDER DESCENDING)
    # The flat layout stays LAST — it is the fallback, and a versioned entry is
    # what a provisioning run just wrote (same rule as the Corrosion prefixes).
    if(IS_DIRECTORY "${_store}/cyclonedds/bin")
        list(APPEND _dirs "${_store}/cyclonedds/bin")
    endif()
    set(${_out} "${_dirs}" PARENT_SCOPE)
endfunction()

function(_nros_find_idlc _out)
    _nros_cyclonedds_sdk_bins(_sdk_bins)
    find_program(${_out} idlc
        HINTS
            ${_sdk_bins}
            "${CycloneDDS_DIR}/../../../bin"
            "${CMAKE_INSTALL_PREFIX}/bin"
            "$ENV{CYCLONEDDS_INSTALL_DIR}/bin"
        NO_CMAKE_FIND_ROOT_PATH
        DOC "Cyclone DDS IDL compiler (host tool)")
endfunction()

# Locate idlc — Cyclone exports it as `CycloneDDS::idlc` when it's
# installed alongside ddsc.
if(NOT TARGET CycloneDDS::idlc)
    # Phase 186.3: a self-provisioned build with no `just` step resolves idlc
    # from PATH (e.g. a ROS 2 install) or a pre-set IDLC_EXECUTABLE.
    _nros_find_idlc(IDLC_EXECUTABLE)
    # `find_program` CACHES its hit, and returns it thereafter without
    # searching. So a build dir that once resolved a broken idlc keeps it even
    # after a working one is provisioned — the cache answers before the search
    # does. Probe, and on failure drop the cached hit and search once more
    # before giving up; otherwise the remedy ("provision one") cannot take
    # effect in the tree that needs it (issue 0633).
    if(IDLC_EXECUTABLE)
        _nros_idlc_runs("${IDLC_EXECUTABLE}" _nros_idlc_why _nros_idlc_env)
        if(_nros_idlc_why)
            message(STATUS
                "nano-ros: cached idlc ${IDLC_EXECUTABLE} cannot run "
                "(${_nros_idlc_why}); dropping it and searching again (issue 0633)")
            unset(IDLC_EXECUTABLE CACHE)
            _nros_find_idlc(IDLC_EXECUTABLE)
        endif()
    endif()
    if(IDLC_EXECUTABLE)
        _nros_idlc_runs("${IDLC_EXECUTABLE}" _nros_idlc_why _nros_idlc_env)
        set(NROS_RMW_CYCLONEDDS_IDLC_ENV "${_nros_idlc_env}"
            CACHE INTERNAL "env prefix idlc needs to run (issue 0601)")
        if(_nros_idlc_why)
            message(FATAL_ERROR
                "idlc was found but cannot run: ${IDLC_EXECUTABLE}\n"
                "  ${_nros_idlc_why}\n"
                "  (issue 0601 — this used to surface far away, mid-build, as "
                "`FAILED: [code=127]` on the first .idl, which reads as a "
                "missing tool rather than one that fails to LOAD.)\n"
                "  A ROS-provided idlc needs ROS's library path in THIS build's "
                "environment, not just in the launching shell. Either source "
                "ROS's setup into the build, or pass a working "
                "-DIDLC_EXECUTABLE=<path-to-idlc>.")
        endif()
    endif()
    if(NOT IDLC_EXECUTABLE)
        message(FATAL_ERROR
            "idlc (Cyclone DDS IDL compiler, a host tool) not found.\n"
            "  Put it on PATH (e.g. a ROS 2 / CycloneDDS install), or pass "
            "-DIDLC_EXECUTABLE=<path-to-idlc>.")
    endif()
endif()

# Resolve idlc to an absolute path *here*, where the imported
# `CycloneDDS::idlc` target is visible, and stash it in an INTERNAL
# cache var. Imported targets are directory-scoped, so a far-away
# consumer (e.g. an example calling `nros_generate_interfaces`) cannot
# expand `$<TARGET_FILE:CycloneDDS::idlc>` — the genex resolves to an
# empty string and idlc never runs. The cached absolute path is
# visible from every scope.
#
# Re-resolve when the cached path no longer exists, not just when it is
# unset: the value is a sticky INTERNAL cache entry, so a build dir
# configured under an older repo layout keeps a path that may now point
# through a deleted directory (e.g. Phase 180.B removed `examples/zephyr/
# cmake`, leaving stale `.../examples/zephyr/cmake/../../../build/install/
# bin/idlc` caches that fail to resolve → `idlc: not found` / exit 127).
# `NOT EXISTS` forces a fresh resolution from the current layout; the
# INTERNAL `set` below implies FORCE, so it overwrites the stale value.
#
# Issue 0633: the re-resolution gate below asks whether the cached tool RUNS,
# not whether it EXISTS. `NOT EXISTS` was the original spelling and it covers
# only a path that vanished; the far more common stale state on a host with ROS
# is a path that is still there and can no longer LOAD
# (`libiceoryx_binding_c.so: cannot open shared object file`). That file exists,
# so the whole block below — which is where BOTH the SDK preference and issue
# 0601's runnability probe live — was skipped, and the broken answer was reused
# forever. Measured: a plain reconfigure left 33 references to the unusable
# binary, and so did `-DIDLC_EXECUTABLE=<working>`, because that variable is
# consulted two levels INSIDE the block the cache short-circuits. Selection by
# existence where runnability is the property that matters — the same class
# 0601 named, fixed there at the point of SELECTION and left standing here at
# the point of REUSE.
set(_nros_idlc_reuse OFF)
if(NROS_RMW_CYCLONEDDS_IDLC AND EXISTS "${NROS_RMW_CYCLONEDDS_IDLC}")
    _nros_idlc_runs("${NROS_RMW_CYCLONEDDS_IDLC}" _nros_cached_why _nros_cached_env)
    if(_nros_cached_why)
        message(STATUS
            "nano-ros: cached idlc ${NROS_RMW_CYCLONEDDS_IDLC} no longer runs "
            "(${_nros_cached_why}); re-resolving (issue 0633)")
    else()
        set(_nros_idlc_reuse ON)
        set(NROS_RMW_CYCLONEDDS_IDLC_ENV "${_nros_cached_env}"
            CACHE INTERNAL "env prefix idlc needs to run (issue 0601)")
    endif()
endif()

if(NOT _nros_idlc_reuse)
    # Candidates in preference order; the first that RUNS wins. Probing every
    # rung rather than only the one `_nros_find_idlc` returns is what stops a
    # stale cached value at ANY rung from deciding the build: issue 0633 had
    # two of them (this variable and `find_program`'s own hit), and a fix that
    # invalidated one would have reported success while staying broken.
    set(_idlc_paths "")
    set(_idlc_origins "")
    if(TARGET CycloneDDS::idlc)
        foreach(_loc_prop
                IMPORTED_LOCATION
                IMPORTED_LOCATION_RELEASE
                IMPORTED_LOCATION_RELWITHDEBINFO
                IMPORTED_LOCATION_DEBUG
                IMPORTED_LOCATION_NOCONFIG)
            get_target_property(_p CycloneDDS::idlc ${_loc_prop})
            if(_p)
                list(APPEND _idlc_paths "${_p}")
                list(APPEND _idlc_origins "imported target CycloneDDS::idlc (${_loc_prop})")
            endif()
        endforeach()
    endif()
    if(IDLC_EXECUTABLE)
        list(APPEND _idlc_paths "${IDLC_EXECUTABLE}")
        list(APPEND _idlc_origins "IDLC_EXECUTABLE")
    endif()
    # A real on-disk search, so far consumers never depend on the imported
    # target being visible in their scope. The cached hit is dropped first for
    # the reason in the selection block above: `find_program` answers from its
    # cache without searching, so a provisioning run that installs a working
    # tool would otherwise change nothing here.
    unset(_idlc_found CACHE)
    _nros_find_idlc(_idlc_found)
    if(_idlc_found)
        list(APPEND _idlc_paths "${_idlc_found}")
        list(APPEND _idlc_origins "search (SDK store, then PATH)")
    endif()

    set(_idlc_loc "")
    set(_idlc_env "")
    set(_idlc_origin "")
    set(_idlc_idx 0)
    foreach(_cand IN LISTS _idlc_paths)
        if(NOT _idlc_loc)
            list(GET _idlc_origins ${_idlc_idx} _cand_origin)
            _nros_idlc_runs("${_cand}" _cand_why _cand_env)
            if(_cand_why)
                message(STATUS
                    "nano-ros: idlc candidate ${_cand} "
                    "(${_cand_origin}) cannot run: ${_cand_why}")
            else()
                set(_idlc_loc "${_cand}")
                set(_idlc_env "${_cand_env}")
                set(_idlc_origin "${_cand_origin}")
            endif()
        endif()
        math(EXPR _idlc_idx "${_idlc_idx}+1")
    endforeach()

    if(_idlc_loc)
        set(NROS_RMW_CYCLONEDDS_IDLC "${_idlc_loc}"
            CACHE INTERNAL "Absolute path to Cyclone DDS idlc")
        set(NROS_RMW_CYCLONEDDS_IDLC_ENV "${_idlc_env}"
            CACHE INTERNAL "env prefix idlc needs to run (issue 0601)")
        # Say which one was chosen and why. Issue 0500's lesson is that a
        # provisioning path which "prints success either way" is how the wrong
        # answer wins silently, and a sticky cache is a second way to win
        # silently: the reconfigures that changed nothing still printed
        # `Configuring done` / `Generating done`.
        message(STATUS "nano-ros: idlc ${_idlc_loc} via ${_idlc_origin}")
    endif()
endif()

# Phase 117.X.1: locate the .msg/.srv → mangled-IDL converter.
#
# Resolution order (no source-tree-relative HINTs — see CLAUDE.md
# "CMake Path Convention" — callers must pass absolute paths):
#   1. Cache var `NROS_RMW_CYCLONEDDS_MSG_TO_IDL` (e.g. set via
#      `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=…` on cmake configure).
#   2. Env var `NROS_RMW_CYCLONEDDS_SCRIPTS_DIR` containing the
#      installed `msg_to_cyclone_idl.py`.
#   3. `share/nros-rmw-cyclonedds/` next to the installed CMake
#      config (this is a CMake-install layout convention, not a
#      project-source-tree assumption — `CMAKE_CURRENT_LIST_DIR`
#      resolves to `<prefix>/lib/cmake/NrosRmwCyclonedds` for
#      installed consumers, and `share` is a sibling). For in-tree
#      development, the consumer (e.g. the project's own
#      `tests/CMakeLists.txt`) sets the cache var directly.
if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    find_program(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
        NAMES msg_to_cyclone_idl.py
        HINTS
            "$ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR}"
            "${CMAKE_CURRENT_LIST_DIR}/../../../share/nros-rmw-cyclonedds"
            # phase-292 W2 (ASI wall #7) — Zephyr-module / source-tree
            # consumption: this file lives at
            # packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/, the converter
            # at <repo>/scripts/cyclonedds/, so it is FIVE levels up (phase-321
            # W2.d moved the group one deeper). Without this hint the descriptor
            # codegen silently degrades to the legacy path and every
            # find_descriptor() fails at runtime (create_subscription -100).
            "${CMAKE_CURRENT_LIST_DIR}/../../../../../scripts/cyclonedds"
        DOC ".msg/.srv → Cyclone-shaped IDL converter"
    )
endif()
if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    # Soft warning — the legacy hand-authored-IDL path still works
    # without it; only callers of nros_rmw_cyclonedds_generate_from_msg
    # need it.
    message(STATUS
        "msg_to_cyclone_idl.py not found; "
        "nros_rmw_cyclonedds_generate_from_msg() will fail. "
        "Pass -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=<abs path> or set "
        "NROS_RMW_CYCLONEDDS_SCRIPTS_DIR.")
endif()

# Phase 117.X.6 — validate the type-info opt-in at configure time.
# Cyclone 0.10.5's idlc produces a truncated `.c` (just the ops
# array, no descriptor) when type-info emission is requested. If the
# consumer opts in we run idlc on a synthetic minimal IDL and check
# the descriptor symbol lands in the output; otherwise we error out
# with a clear pointer to the upstream bug rather than letting the
# build fail later with confusing link errors.
if(NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO OR
   "$ENV{NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO}")
    if(TARGET CycloneDDS::idlc)
        get_target_property(_probe_idlc CycloneDDS::idlc IMPORTED_LOCATION)
        if(NOT _probe_idlc)
            get_target_property(_probe_idlc CycloneDDS::idlc IMPORTED_LOCATION_RELEASE)
        endif()
    else()
        set(_probe_idlc "${IDLC_EXECUTABLE}")
    endif()
# Issue 0601 — the env idlc needs, as a command PREFIX.
#
# Empty on a healthy host, so the generated command stays byte-identical there
# and no build.ninja churns. When set, every place that RUNS idlc must use it —
# the xtypes probe and the codegen rule alike. A prefix applied at one site and
# not its sibling is issue 0442's shape, which is why this is one variable
# rather than two spellings.
# CACHE INTERNAL, not a normal var: `nros_rmw_cyclonedds_idlc_compile` is called
# from far-away directory scopes (an example's CMakeLists), and a normal
# file-scope variable does not survive that — the same trap CLAUDE.md records as
# the `_NROS_ENTRY_DIR` pattern, and the reason `NROS_RMW_CYCLONEDDS_IDLC` above
# is cached too. A launcher that silently evaporates would put the codegen rule
# back on the bare, unusable idlc.
if(NROS_RMW_CYCLONEDDS_IDLC_ENV)
    set(_NROS_IDLC_LAUNCHER "${CMAKE_COMMAND};-E;env;${NROS_RMW_CYCLONEDDS_IDLC_ENV}"
        CACHE INTERNAL "command prefix that makes idlc runnable (issue 0601)")
else()
    set(_NROS_IDLC_LAUNCHER "" CACHE INTERNAL
        "command prefix that makes idlc runnable (issue 0601)")
endif()

    set(_probe_dir "${CMAKE_CURRENT_BINARY_DIR}/_nros_rmw_cyclonedds_xtypes_probe")
    file(MAKE_DIRECTORY "${_probe_dir}")
    file(WRITE "${_probe_dir}/probe.idl"
        "@final struct NrosRmwCycloneddsTypeinfoProbe { long x; };\n")
    execute_process(
        COMMAND ${_NROS_IDLC_LAUNCHER} "${_probe_idlc}" -l c -o "${_probe_dir}" "${_probe_dir}/probe.idl"
        OUTPUT_QUIET ERROR_QUIET
        RESULT_VARIABLE _probe_rc
    )
    set(_probe_c "${_probe_dir}/probe.c")
    set(_probe_ok FALSE)
    if(_probe_rc EQUAL 0 AND EXISTS "${_probe_c}")
        file(READ "${_probe_c}" _probe_contents)
        if(_probe_contents MATCHES
                "NrosRmwCycloneddsTypeinfoProbe_desc[ \t]*=")
            set(_probe_ok TRUE)
        endif()
    endif()
    if(NOT _probe_ok)
        message(FATAL_ERROR
            "NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO is ON but the bundled "
            "Cyclone DDS idlc fails to emit XTypes type-info "
            "(produces a truncated descriptor). This is a known upstream "
            "bug in Cyclone 0.10.5. Either upgrade the Cyclone pin past "
            "the fixed release or unset the option. See "
            "docs/reference/cyclonedds-known-limitations.md.")
    endif()
    message(STATUS "Cyclone idlc XTypes type-info probe: OK")
endif()

#
# nros_rmw_cyclonedds_idlc_compile
#
# An IDL file may contain multiple `@topic`-eligible structs. Pass
# one TYPE_NAME (single-type) or TYPE_NAMES (one per struct, all
# registered) — both forms emit one constructor per name.
#
# issue 0325 — ONE definition of where a host idlc lives. This three-entry
# search was copy-pasted twice inside this file; the copies were identical, but
# a fix applied to one would have silently missed the other.
#
# These stay in HINTS (searched BEFORE the host PATH) deliberately, unlike the
# retired in-tree dirs in zephyr/cmake/nros_rmw_cyclonedds.cmake which moved to
# PATHS. They point at the idlc shipped WITH the Cyclone this build links
# (`CycloneDDS_DIR`-relative, the install prefix, or an explicit
# `CYCLONEDDS_INSTALL_DIR`), so preferring them over an arbitrary PATH idlc is
# what keeps the emitted descriptors ABI-matched to the linked ddsc. A
# version-mismatched idlc is precisely the `find_descriptor() -> nullptr`
# failure the issue is about.
#
# idlc is a HOST build tool (it runs on the build machine to emit C
# descriptors), so search the host even in a cross build —
# NO_CMAKE_FIND_ROOT_PATH ignores the toolchain's find-root mode (some set
# MODE_PROGRAM=ONLY, which would otherwise hide host idlc).
function(nros_rmw_cyclonedds_idlc_compile output_var)
    set(_options "")
    set(_one    IDL_FILE OUTPUT_DIR TYPE_NAME PKG_NAME)
    set(_multi  TYPE_NAMES INCLUDE_DIRS EXTRA_DEPENDS)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_IDL_FILE)
        message(FATAL_ERROR "nros_rmw_cyclonedds_idlc_compile: IDL_FILE required")
    endif()
    if(NOT _arg_OUTPUT_DIR)
        set(_arg_OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-types")
    endif()
    file(MAKE_DIRECTORY "${_arg_OUTPUT_DIR}")

    get_filename_component(_idl_abs "${_arg_IDL_FILE}" ABSOLUTE)
    get_filename_component(_idl_stem "${_arg_IDL_FILE}" NAME_WE)
    set(_gen_c "${_arg_OUTPUT_DIR}/${_idl_stem}.c")
    set(_gen_h "${_arg_OUTPUT_DIR}/${_idl_stem}.h")

    # Use the absolute path cached at module-load (see top of file) so
    # this works from any scope, not only where CycloneDDS::idlc is
    # visible. `$<TARGET_FILE:…>` would expand to "" for far consumers.
    if(NROS_RMW_CYCLONEDDS_IDLC)
        set(_idlc "${NROS_RMW_CYCLONEDDS_IDLC}")
    elseif(TARGET CycloneDDS::idlc)
        set(_idlc "$<TARGET_FILE:CycloneDDS::idlc>")
    else()
        set(_idlc "${IDLC_EXECUTABLE}")
    endif()

    # Phase 117.X.6 — opt-in XTypes type-info emission. Default keeps
    # the `-t` flag (omits type-info) because Cyclone 0.10.5's idlc
    # produces truncated descriptors when type-info is requested.
    # Downstream consumers on a fixed Cyclone build flip the flag via
    # `-DNROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=ON` (cache var) or
    # `NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=1` (env).
    set(_idlc_flags "-t" "-l" "c")
    if(NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO OR
       "$ENV{NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO}")
        set(_idlc_flags "-l" "c")
    endif()

    # Composite messages `#include` sibling / cross-package IDLs using
    # the rosidl-style `<pkg>/msg/<Type>.idl` path. idlc resolves those
    # against `-I <root>` dirs where the package-nested layout lives.
    foreach(_inc IN LISTS _arg_INCLUDE_DIRS)
        list(APPEND _idlc_flags "-I" "${_inc}")
    endforeach()

    add_custom_command(
        OUTPUT  "${_gen_c}" "${_gen_h}"
        COMMAND ${_NROS_IDLC_LAUNCHER} "${_idlc}" ${_idlc_flags} -o "${_arg_OUTPUT_DIR}" "${_idl_abs}"
        DEPENDS "${_idl_abs}" ${_arg_EXTRA_DEPENDS}
        COMMENT "idlc ${_idl_stem}.idl"
        VERBATIM
    )

    set(_out_files "${_gen_c}")

    # Normalise the single + multi forms into one list.
    set(_all_types "")
    if(_arg_TYPE_NAME)
        list(APPEND _all_types "${_arg_TYPE_NAME}")
    endif()
    if(_arg_TYPE_NAMES)
        list(APPEND _all_types ${_arg_TYPE_NAMES})
    endif()

    set(_idx 0)
    foreach(_tn IN LISTS _all_types)
        # Per-type self-registration TU. The descriptor symbol is
        # `<TYPE_NAME with :: → _>_desc`; matches Cyclone idlc's
        # mangling. `_<idx>` keeps each register TU's filename
        # unique when multiple types share the same IDL.
        string(REPLACE "::" "_" _desc_sym "${_tn}_desc")
        # Sanitise the constructor's symbol name — descriptor symbol
        # has only A-Za-z0-9_ already so it's safe to reuse.
        # Issue #177 — namespace the ctor by package when the caller says
        # which one: ROS ships the SAME type stem in several packages
        # (std_msgs/Int32 vs example_interfaces/Int32, String, the whole
        # *MultiArray family), so bare `register_<stem>_<idx>` symbols
        # collide at link the moment a fixture pulls both ts archives.
        # Callers without PKG_NAME (the hand-IDL graph TU, legacy
        # add_idl_library users) keep the historical name.
        if(_arg_PKG_NAME)
            set(_ctor "register_${_arg_PKG_NAME}_${_idl_stem}_${_idx}")
        else()
            set(_ctor "register_${_idl_stem}_${_idx}")
        endif()
        set(_reg "${_arg_OUTPUT_DIR}/${_idl_stem}_register_${_idx}.c")
        file(WRITE "${_reg}.in"
"/* Auto-generated by nros_rmw_cyclonedds_idlc_compile() — do not edit. */
#include \"dds/dds.h\"
#include \"${_idl_stem}.h\"

extern const dds_topic_descriptor_t ${_desc_sym};

void nros_rmw_cyclonedds_register_descriptor(
    const char *type_name, const dds_topic_descriptor_t *desc);

void ${_ctor}(void) {
    nros_rmw_cyclonedds_register_descriptor(
        \"${_tn}\", &${_desc_sym});
}

__attribute__((constructor))
static void ${_ctor}_constructor(void) {
    ${_ctor}();
}
")
        configure_file("${_reg}.in" "${_reg}" COPYONLY)
        # The register TU `#include`s the idlc-generated `<stem>.h`.
        # idlc emits `.c` + `.h` from one custom_command, but only the
        # `.c` is a tracked source — nothing makes the register TU's
        # compile wait for the header, so a parallel build races and
        # fails with "<stem>.h: No such file or directory". Pin the
        # ordering with an explicit object dependency on the header.
        set_source_files_properties("${_reg}" PROPERTIES
            OBJECT_DEPENDS "${_gen_h}")
        list(APPEND _out_files "${_reg}")
        math(EXPR _idx "${_idx} + 1")
    endforeach()

    set(${output_var} "${_out_files}" PARENT_SCOPE)
endfunction()

#
# nros_rmw_cyclonedds_add_idl_library
#
function(nros_rmw_cyclonedds_add_idl_library tgt)
    set(_options "")
    set(_one    "")
    set(_multi  IDL_FILES REGISTER_TYPES)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_IDL_FILES)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_add_idl_library: IDL_FILES required")
    endif()

    set(_all_sources "")
    foreach(_idl IN LISTS _arg_IDL_FILES)
        get_filename_component(_idl_stem "${_idl}" NAME_WE)
        set(_type_for_this "")
        # REGISTER_TYPES is a list of "<idl_stem>=<full::cpp::Type>" pairs.
        foreach(_pair IN LISTS _arg_REGISTER_TYPES)
            string(REGEX MATCH "^([^=]+)=(.*)$" _m "${_pair}")
            if(_m)
                set(_lhs "${CMAKE_MATCH_1}")
                set(_rhs "${CMAKE_MATCH_2}")
                if(_lhs STREQUAL "${_idl_stem}")
                    set(_type_for_this "${_rhs}")
                endif()
            endif()
        endforeach()
        if(_type_for_this)
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl}"
                OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl"
                TYPE_NAME "${_type_for_this}"
            )
        else()
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl}"
                OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl"
            )
        endif()
        list(APPEND _all_sources ${_gen})
    endforeach()

    add_library(${tgt} STATIC ${_all_sources})
    target_include_directories(${tgt}
        PUBLIC "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl")
    target_link_libraries(${tgt} PUBLIC CycloneDDS::ddsc)
    set_target_properties(${tgt} PROPERTIES POSITION_INDEPENDENT_CODE ON)
endfunction()

#
# nros_rmw_cyclonedds_generate_from_msg
#
# Phase 117.X.1: drive `.msg` / `.srv` → mangled IDL → idlc → static-
# init self-registration. Output type names match what stock
# `rmw_cyclonedds_cpp` emits, so a nano-ros publisher / service-server
# matches an `rclcpp` subscriber / client by `(topic_name, type_name)`.
#
#   nros_rmw_cyclonedds_generate_from_msg(<output_var>
#       PKG_NAME    <my_msgs>
#       PKG_DIR     <path/to/pkg-with-package.xml>
#       INTERFACES  <Foo.msg> <Bar.srv> ...
#       [OUTPUT_DIR <build/dir>]
#       [IDL_DEPENDS <dep-pkg .idl paths...>]
#       [IDL_FILES_VAR <out-var>]
#   )
#
# Sets <output_var> to the list of generated `.c` (descriptor +
# self-registration) source files. IDL_DEPENDS adds dependency
# packages' generated `.idl` files to every pass-2 idlc command's
# DEPENDS (file-level cross-package include ordering, phase-306 W2);
# IDL_FILES_VAR returns this package's generated `.idl` paths so the
# caller can feed the NEXT package's IDL_DEPENDS.
#
# For each `.msg` Foo:
#     descriptor name: <PKG>::msg::dds_::Foo_
#     registry key:    "<PKG>::msg::dds_::Foo_"
# For each `.srv` Foo:
#     two descriptors registered:
#       <PKG>::srv::dds_::Foo_Request_
#       <PKG>::srv::dds_::Foo_Response_
#
function(nros_rmw_cyclonedds_generate_from_msg output_var)
    set(_options "")
    set(_one    PKG_NAME PKG_DIR OUTPUT_DIR INCLUDE_ROOT GEN_ROOT IDL_FILES_VAR)
    set(_multi  INTERFACES IDL_DEPENDS)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_PKG_NAME OR NOT _arg_PKG_DIR OR NOT _arg_INTERFACES)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_generate_from_msg: PKG_NAME, PKG_DIR, "
            "and INTERFACES are required.")
    endif()
    if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_generate_from_msg requires "
            "msg_to_cyclone_idl.py — set NROS_RMW_CYCLONEDDS_SCRIPTS_DIR "
            "or check `find_program(NROS_RMW_CYCLONEDDS_MSG_TO_IDL …)` "
            "above.")
    endif()
    if(NOT _arg_OUTPUT_DIR)
        set(_arg_OUTPUT_DIR
            "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-from-msg/${_arg_PKG_NAME}")
    endif()
    # Composite messages cross-reference sibling / cross-package IDLs
    # via `#include "<pkg>/msg/<Type>.idl"`. When the caller supplies a
    # shared INCLUDE_ROOT, write each package's IDLs into the nested
    # `<root>/<pkg>/msg/` layout those includes expect and hand idlc
    # `-I <root>`. Packages generated earlier (declared DEPENDENCIES)
    # populate the same root, so cross-package includes resolve too.
    # Without INCLUDE_ROOT we keep the flat layout (legacy hand-IDL
    # callers register one self-contained type at a time).
    if(_arg_INCLUDE_ROOT)
        set(_idl_dir "${_arg_INCLUDE_ROOT}/${_arg_PKG_NAME}/msg")
        set(_idlc_includes "${_arg_INCLUDE_ROOT}")
    else()
        set(_idl_dir "${_arg_OUTPUT_DIR}/idl")
        set(_idlc_includes "")
    endif()
    # idlc emits each descriptor `.h` with `#include "<pkg>/msg/<Dep>.h"`
    # lines for composite members, so the generated `.c`/`.h` must also
    # live in the package-nested layout and compile with `-I <GEN_ROOT>`.
    # Without GEN_ROOT, keep the flat per-package gen dir (legacy path,
    # self-contained types only).
    if(_arg_GEN_ROOT)
        set(_gen_dir "${_arg_GEN_ROOT}/${_arg_PKG_NAME}/msg")
    else()
        set(_gen_dir "${_arg_OUTPUT_DIR}/gen")
    endif()
    file(MAKE_DIRECTORY "${_idl_dir}")
    file(MAKE_DIRECTORY "${_gen_dir}")

    # Resolve absolute interface paths so the script + custom_command
    # see the same files regardless of caller's CMAKE_CURRENT_SOURCE_DIR.
    set(_iface_args "")
    set(_iface_abs_list "")
    foreach(_iface IN LISTS _arg_INTERFACES)
        if(IS_ABSOLUTE "${_iface}")
            set(_abs "${_iface}")
        else()
            set(_abs "${_arg_PKG_DIR}/${_iface}")
        endif()
        list(APPEND _iface_args "--interface" "${_abs}")
        list(APPEND _iface_abs_list "${_abs}")
    endforeach()

    set(_all_outputs "")

    # Pass 1 — convert every .msg/.srv to mangled IDL first and collect
    # the .idl paths. idlc reads `#include`d sibling / cross-package
    # IDLs at generation time, so every idlc command in pass 2 must wait
    # for *all* of this package's .idl files (and, via the ts-lib target
    # ordering set up by the caller, the dependency packages' files in
    # the shared INCLUDE_ROOT).
    set(_pkg_idl_paths "")
    foreach(_iface IN LISTS _arg_INTERFACES)
        get_filename_component(_iface_stem "${_iface}" NAME_WE)
        set(_idl_path "${_idl_dir}/${_iface_stem}.idl")

        if(IS_ABSOLUTE "${_iface}")
            set(_iface_abs "${_iface}")
        else()
            set(_iface_abs "${_arg_PKG_DIR}/${_iface}")
        endif()
        add_custom_command(
            OUTPUT  "${_idl_path}"
            COMMAND "${CMAKE_COMMAND}" -E env
                    "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL}"
                    --pkg-name "${_arg_PKG_NAME}"
                    --pkg-dir  "${_arg_PKG_DIR}"
                    --output-dir "${_idl_dir}"
                    --interface "${_iface_abs}"
            DEPENDS "${_iface_abs}" "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL}"
            COMMENT "msg_to_cyclone_idl ${_arg_PKG_NAME}/${_iface}"
            VERBATIM
        )
        list(APPEND _pkg_idl_paths "${_idl_path}")
    endforeach()

    # Pass 2 — run idlc on each .idl, gated on all sibling .idl files.
    foreach(_iface IN LISTS _arg_INTERFACES)
        get_filename_component(_iface_stem "${_iface}" NAME_WE)
        get_filename_component(_iface_ext  "${_iface}" EXT)
        set(_idl_path "${_idl_dir}/${_iface_stem}.idl")

        # Decide which type name(s) to register based on the
        # extension. .msg → one name, .srv → two (Request + Response).
        if(_iface_ext STREQUAL ".msg")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAME "${_arg_PKG_NAME}::msg::dds_::${_iface_stem}_"
            )
        elseif(_iface_ext STREQUAL ".srv")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAMES
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Request_"
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Response_"
            )
        elseif(_iface_ext STREQUAL ".action")
            # `msg_to_cyclone_idl.py` synthesizes the eight action wrapper
            # types into one IDL (base Goal/Result/Feedback +
            # SendGoal/GetResult Request/Response + FeedbackMessage),
            # matching the nros action layer's wire framing. Register all
            # eight; the backend derives which one a given sub-service /
            # topic needs from its keyexpr role (Phase 171.0.b Piece 1).
            set(_act "${_arg_PKG_NAME}::action::dds_::${_iface_stem}")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAMES
                    "${_act}_Goal_"
                    "${_act}_Result_"
                    "${_act}_Feedback_"
                    "${_act}_SendGoal_Request_"
                    "${_act}_SendGoal_Response_"
                    "${_act}_GetResult_Request_"
                    "${_act}_GetResult_Response_"
                    "${_act}_FeedbackMessage_"
            )
        else()
            message(FATAL_ERROR
                "nros_rmw_cyclonedds_generate_from_msg: unsupported "
                "extension ${_iface_ext} on ${_iface}")
        endif()

        list(APPEND _all_outputs ${_gen})
    endforeach()

    set(${output_var} "${_all_outputs}" PARENT_SCOPE)
    # phase-306 W2 (issue 0258): hand the caller this package's generated
    # .idl paths so it can thread them into DEPENDENT packages' IDL_DEPENDS
    # — cross-package includes then carry FILE-level custom-command deps
    # (an unpopulated dep root fails at generate time with a clear "no rule
    # to make <dep>.idl" instead of an idlc preprocessor error).
    if(_arg_IDL_FILES_VAR)
        set(${_arg_IDL_FILES_VAR} "${_pkg_idl_paths}" PARENT_SCOPE)
    endif()
endfunction()

# phase-347 W5 — the per-message typesupport hook, MOVED HERE from
# `cmake/NanoRosGenerateInterfaces.cmake`.
#
# It used to sit in the shared codegen pipeline behind
# `if(NANO_ROS_RMW STREQUAL "cyclonedds" AND COMMAND ...)` — 27 mentions of one
# backend in a file every backend goes through, and the single largest reason
# cyclonedds looked "special". Nothing in it is generic: it knows that Cyclone
# 0.10.5's idlc aborts on `wstring`, that descriptors are C so the consumer must
# enable the C language, how the shared IDL include root is laid out, and how to
# force-load a static-init registration TU past `--gc-sections`.
#
# So it belongs to the backend, and `nros_generate_interfaces()` now reaches it
# through the descriptor's `[rmw.codegen].per_message` (RFC-0071 D4) rather than
# by name. The body below is unchanged apart from becoming a function and taking
# what it used to read from the caller's scope as arguments.
#
# Contract (the same one a cargo-rooted consumer satisfies through
# `nros codegen cyclonedds-descriptors`): given a target, its interface files,
# its dependencies and the message library to attach the result to, generate the
# per-message typesupport and wire it in. A backend that declares no
# `per_message` hook pays nothing.
function(nros_rmw_cyclonedds_typesupport_for_target)
    set(_one    TARGET LIB_TARGET)
    set(_multi  INTERFACE_FILES DEPENDENCIES)
    cmake_parse_arguments(_HK "" "${_one}" "${_multi}" ${ARGN})
    if(NOT _HK_TARGET OR NOT _HK_LIB_TARGET)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_typesupport_for_target: TARGET and LIB_TARGET "
            "are required")
    endif()
    # Names the moved body reads from what used to be enclosing scope.
    set(target "${_HK_TARGET}")
    set(_lib_target "${_HK_LIB_TARGET}")
    set(_interface_files ${_HK_INTERFACE_FILES})
    set(_ARG_DEPENDENCIES ${_HK_DEPENDENCIES})

    # .msg / .srv / .action all carry data types. Actions are
    # synthesized into their eight wrapper descriptors by
    # `msg_to_cyclone_idl.py` (see generate_from_msg's `.action` branch).
    set(_cyc_ifaces "")
    foreach(_if ${_interface_files})
      if(_if MATCHES "\\.(msg|srv|action)$")
        # Cyclone DDS 0.10.5's idlc crashes on `wstring` (wide-string)
        # fields — it parses the type then aborts in delete_const_expr.
        # The full ROS `example_interfaces` (resolved via
        # AMENT_PREFIX_PATH) ships `WString[MultiArray]`, which no
        # example uses as a topic. Skip any interface declaring a
        # wstring field rather than letting one unused type abort the
        # whole package's descriptor build. Documented upstream limit.
        file(READ "${_if}" _if_body)
        if(_if_body MATCHES "(\n|^)[ \t]*wstring[ \t<\\[]")
          message(STATUS
            "nros_generate_interfaces(${target}): skipping cyclonedds "
            "descriptor for ${_if} — `wstring` is unsupported by the "
            "bundled Cyclone DDS 0.10.5 idlc.")
        else()
          list(APPEND _cyc_ifaces "${_if}")
        endif()
      endif()
    endforeach()
    if(_cyc_ifaces)
      # NOTE: idlc emits the topic descriptors as C source, so the
      # consuming project must enable the C language. C++ examples
      # therefore declare `project(... LANGUAGES CXX C)` — see the
      # native cpp/cyclonedds examples. (enable_language() from inside
      # this function does not reliably register the C toolchain in the
      # caller's directory scope, hence the project()-level requirement.)
      # PKG_DIR = the package root (parent of msg/ or srv/). All
      # interface files for one `target` share a package root.
      list(GET _cyc_ifaces 0 _cyc_first)
      get_filename_component(_cyc_ifdir "${_cyc_first}" DIRECTORY)
      get_filename_component(_cyc_pkgdir "${_cyc_ifdir}" DIRECTORY)
      # Shared IDL include root for the whole build. Composite messages
      # (`std_msgs/Header` → `builtin_interfaces/Time`, the `*MultiArray`
      # family → `MultiArrayLayout`) `#include` sibling / cross-package
      # IDLs; idlc resolves those against `-I <root>` with each package
      # laid out as `<root>/<pkg>/msg/<Type>.idl`. Anchor the root at the
      # binary dir of the call that first creates it so every package in
      # one example shares it.
      set(_cyc_idl_root "${CMAKE_BINARY_DIR}/cyclonedds-ts/_idlroot")
      set(_cyc_gen_root "${CMAKE_BINARY_DIR}/cyclonedds-ts/_genroot")
      # phase-306 W2 (issue 0258) — cross-package includes are FILE-level:
      # a package's lowered IDL `#include`s dep-package IDLs (`Odometry.idl`
      # → `std_msgs/msg/Header.idl`), which idlc reads at generate time.
      # Target-level `add_dependencies` below orders SIBLING ts libs, but a
      # dep whose ts lib never materializes (or a scope where the target is
      # not visible) left the include unresolved → cryptic idlc preprocessor
      # error. Thread each dep's generated .idl list (stashed in the CACHE,
      # same multi-scope pattern as `_NROS_PKG_<pkg>_GENERATED_RS_FILES` in
      # NanoRosCodegenCore.cmake) into IDL_DEPENDS: every idlc command then
      # carries file-level deps on the dep IDLs, and a missing dep root
      # fails the build with a clear "no rule to make <dep>.idl". The stash
      # holds each pkg's CLOSURE (deps' stashes + own files), so transitive
      # includes are covered without re-walking the graph here.
      set(_cyc_dep_idls "")
      foreach(_dep ${_ARG_DEPENDENCIES})
        if(DEFINED CACHE{_NROS_PKG_${_dep}_CYC_IDL_FILES})
          list(APPEND _cyc_dep_idls "$CACHE{_NROS_PKG_${_dep}_CYC_IDL_FILES}")
        endif()
      endforeach()
      if(_cyc_dep_idls)
        list(REMOVE_DUPLICATES _cyc_dep_idls)
      endif()
      nros_rmw_cyclonedds_generate_from_msg(_cyc_sources
        PKG_NAME   "${target}"
        PKG_DIR    "${_cyc_pkgdir}"
        INTERFACES ${_cyc_ifaces}
        INCLUDE_ROOT "${_cyc_idl_root}"
        GEN_ROOT     "${_cyc_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/${target}"
        IDL_DEPENDS ${_cyc_dep_idls}
        IDL_FILES_VAR _cyc_own_idls)
      # Stash this package's IDL closure for consumers generated later —
      # possibly in a DIFFERENT directory scope (the phase-219 idempotency
      # guard early-returns there, but the CACHE stash written on the first,
      # generating call persists globally, so guard-skipped packages still
      # contribute their files to dependents' IDL_DEPENDS).
      set(_cyc_stash "${_cyc_dep_idls}")
      list(APPEND _cyc_stash ${_cyc_own_idls})
      if(_cyc_stash)
        list(REMOVE_DUPLICATES _cyc_stash)
      endif()
      set(_NROS_PKG_${target}_CYC_IDL_FILES "${_cyc_stash}" CACHE INTERNAL
        "phase-306 W2: ${target}'s generated cyclone .idl closure")
      if(_cyc_sources)
        add_library(${target}__cyclonedds_ts STATIC ${_cyc_sources})
        # idlc lays the descriptor `.c`/`.h` out as
        # `<gen-root>/<pkg>/msg/<Type>.{c,h}`; the register TUs `#include`
        # their sibling `<Type>.h`, and composite descriptors cross-
        # `#include "<pkg>/msg/<Dep>.h"`. Both resolve against the shared
        # gen root.
        target_include_directories(${target}__cyclonedds_ts PRIVATE
          "${_cyc_gen_root}")
        # The descriptor `.c` files `#include "dds/dds.h"`, so the ts
        # lib needs Cyclone's ddsc *headers*. Pull only the backend's
        # INTERFACE include dirs — do NOT link the backend library.
        # Linking it (even PUBLIC) makes `libnros_rmw_cyclonedds.a`
        # reappear as a plain transitive dependency on the final exe
        # link line; CMake then de-duplicates it out of the
        # `--whole-archive` group NanoRos sets up, so the backend's
        # `.nros_rmw_init` self-registration entry gets GC'd and the
        # RMW registry comes up empty (`nros_support_init -> -3`). The
        # `nros_rmw_cyclonedds_register_descriptor` symbol the register
        # TUs call is resolved at exe link via NanoRos's whole-archived
        # backend, so the ts lib never needs to link it directly.
        if(TARGET nros_rmw_cyclonedds)
          target_include_directories(${target}__cyclonedds_ts PRIVATE
            "$<TARGET_PROPERTY:nros_rmw_cyclonedds,INTERFACE_INCLUDE_DIRECTORIES>")
        endif()
        if(TARGET freertos_kernel)
          target_link_libraries(${target}__cyclonedds_ts PRIVATE freertos_kernel)
        endif()
        # Cross-package include ordering: a dependency package's IDLs
        # must populate the shared root before this package's idlc runs.
        # idlc reads them at generate-time, so order the ts-lib targets.
        foreach(_dep ${_ARG_DEPENDENCIES})
          if(TARGET ${_dep}__cyclonedds_ts)
            add_dependencies(${target}__cyclonedds_ts ${_dep}__cyclonedds_ts)
          endif()
        endforeach()
        # The descriptor self-registration is a static-init TU with no
        # symbol the app references directly, so a plain static-lib link
        # GC's it. Force-load it through the interface message lib so
        # any consumer of `${_lib_target}` keeps the registrations. Do
        # the same for dependency descriptor libs: action endpoints need
        # action_msgs service/status descriptors even when the app only
        # references the concrete user action type.
        if(CMAKE_VERSION VERSION_GREATER_EQUAL "3.24"
           AND NOT CMAKE_SYSTEM_NAME STREQUAL "Generic")
          foreach(_dep ${_ARG_DEPENDENCIES})
            if(TARGET ${_dep}__cyclonedds_ts)
              target_link_libraries(${_lib_target} INTERFACE
                "$<LINK_LIBRARY:WHOLE_ARCHIVE,${_dep}__cyclonedds_ts>")
            endif()
          endforeach()
          target_link_libraries(${_lib_target} INTERFACE
            "$<LINK_LIBRARY:WHOLE_ARCHIVE,${target}__cyclonedds_ts>")
        else()
          set(_cyc_force_load_libs "")
          foreach(_dep ${_ARG_DEPENDENCIES})
            if(TARGET ${_dep}__cyclonedds_ts)
              list(APPEND _cyc_force_load_libs ${_dep}__cyclonedds_ts)
            endif()
          endforeach()
          list(APPEND _cyc_force_load_libs ${target}__cyclonedds_ts)
          # issue #193 — CMake < 3.24 has no $<LINK_LIBRARY:WHOLE_ARCHIVE>.
          # Emitting the group via target_link_LIBRARIES lets CMake de-dupe the
          # ts lib out of the `--whole-archive` group (it keeps a bare, GC-able
          # copy), so the descriptor static-init ctors get GC'd →
          # `find_descriptor -> nullptr -> register_subscription -> -1`. The
          # de-dup-safe pre-3.24 idiom (per CMake's marc.chevrier,
          # discourse.cmake.org/t/5883) is target_link_OPTIONS with a `SHELL:`
          # group: link options are raw flags, not library items, so they are
          # never de-duped, and `$<TARGET_FILE:…>` carries the build-order edge.
          # Link the target normally too (ordinary archive membership) — the
          # documented cost is the lib appearing twice on the link line.
          foreach(_wl ${_cyc_force_load_libs})
            target_link_libraries(${_lib_target} INTERFACE ${_wl})
            target_link_options(${_lib_target} INTERFACE
              "SHELL:-Wl,--whole-archive $<TARGET_FILE:${_wl}> -Wl,--no-whole-archive")
          endforeach()
        endif()
      endif()
    endif()
endfunction()
