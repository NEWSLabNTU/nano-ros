# cmake/NanoRosWorkspace.cmake — Phase 219.I
#
# Workspace-root + per-pkg cmake-fn pair that gives a pure-C/C++
# multi-pkg workspace the same `[workspace]` discipline cargo gives a
# Rust workspace:
#
# Workspace-root CMakeLists.txt:
#
#     cmake_minimum_required(VERSION 3.22)
#     project(my_ws LANGUAGES C CXX)
#     nano_ros_workspace(
#         NANO_ROS_ROOT /path/to/nano-ros     # or set
#                                             # -DNANO_ROS_ROOT=… or
#                                             # let the auto-walk find
#                                             # `nros-sdk-index.toml`
#         BACKEND       zenoh                 # zenoh | xrce | cyclonedds
#         PLATFORM      posix                 # posix | … (default posix)
#         SUBDIRS       src/talker_pkg
#                       src/listener_pkg
#                       src/cpp_entry
#     )
#
# Per-pkg subdir CMakeLists.txt (Node + Entry pkgs):
#
#     cmake_minimum_required(VERSION 3.22)
#     project(talker_pkg LANGUAGES C CXX)
#     nano_ros_workspace_pkg_guard()
#     nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
#     nano_ros_node_register(NAME talker
#                            CLASS talker_pkg::Talker
#                            SOURCES src/Talker.cpp
#                            DEPLOY native)
#
# `nano_ros_workspace_pkg_guard()` is the dual:
#
#   * Inside a workspace — returns immediately (the workspace root
#     already imported nano-ros + included the cmake-fn helpers).
#   * Standalone — replicates the workspace-root body so the same
#     subdir CMakeLists builds solo (preserves the single-pkg
#     copy-out path).
#
# Net effect: every Node/Entry pkg CMakeLists is one canonical shape;
# users decide between "workspace" and "standalone" at the root, not
# in every leaf.
#
# Phase 219 workflow review Gaps 1 + 2 closed.

if(DEFINED _NROS_WORKSPACE_INCLUDED)
    return()
endif()
set(_NROS_WORKSPACE_INCLUDED TRUE)

# CACHE INTERNAL, not a normal var: this file's functions `include()` sibling
# modules, and inside a function body `CMAKE_CURRENT_LIST_DIR` names the
# CALLER's file. A plain `set(_X ${CMAKE_CURRENT_LIST_DIR})` at file scope is
# also dropped when an including frame pops — the `_NROS_ENTRY_DIR` pattern,
# which broke every freertos workspace member in 287-W6.
set(_NROS_WORKSPACE_DIR "${CMAKE_CURRENT_LIST_DIR}"
    CACHE INTERNAL "dir of NanoRosWorkspace.cmake")

# ---------------------------------------------------------------------------
# Helper — walk up from <start> looking for the `nros-sdk-index.toml`
# sentinel that marks every nano-ros checkout root. Writes the
# discovered path to <out_var> (PARENT_SCOPE), or `_NROS_ROOT-NOTFOUND`
# when nothing matches.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRosEdition.cmake")

function(_nros_find_root start out_var)
    set(_dir "${start}")
    set(_max_walk 16)            # bounded — never walk past `/`
    while(_max_walk GREATER 0)
        if(EXISTS "${_dir}/nros-sdk-index.toml")
            set(${out_var} "${_dir}" PARENT_SCOPE)
            return()
        endif()
        get_filename_component(_parent "${_dir}" DIRECTORY)
        if(_parent STREQUAL _dir)
            break()             # reached `/`
        endif()
        set(_dir "${_parent}")
        math(EXPR _max_walk "${_max_walk} - 1")
    endwhile()
    set(${out_var} "_NROS_ROOT-NOTFOUND" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# Resolve the nano-ros root from (in priority order):
#   1. explicit `<NANO_ROS_ROOT>` arg (workspace-root call),
#   2. `-DNANO_ROS_ROOT=…` cache var,
#   3. `NANO_ROS_ROOT` env var,
#   4. auto-walk from <start_dir> for `nros-sdk-index.toml`.
# Writes resolved path to <out_var> (PARENT_SCOPE) or errors via
# FATAL_ERROR with a hint when nothing resolves.
# ---------------------------------------------------------------------------
function(_nros_resolve_root explicit start_dir out_var)
    if(explicit AND NOT explicit STREQUAL "")
        set(${out_var} "${explicit}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED NANO_ROS_ROOT AND NOT NANO_ROS_ROOT STREQUAL "")
        set(${out_var} "${NANO_ROS_ROOT}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED ENV{NANO_ROS_ROOT} AND NOT "$ENV{NANO_ROS_ROOT}" STREQUAL "")
        set(${out_var} "$ENV{NANO_ROS_ROOT}" PARENT_SCOPE)
        return()
    endif()
    _nros_find_root("${start_dir}" _walked)
    if(NOT _walked STREQUAL "_NROS_ROOT-NOTFOUND")
        set(${out_var} "${_walked}" PARENT_SCOPE)
        return()
    endif()
    message(FATAL_ERROR
        "nano-ros: cannot locate nano-ros root from '${start_dir}'.\n"
        "  Pass NANO_ROS_ROOT to `nano_ros_workspace()` or set the\n"
        "  -DNANO_ROS_ROOT=<path> cache var, or run from inside a tree\n"
        "  that contains `nros-sdk-index.toml`.")
endfunction()

# ---------------------------------------------------------------------------
# Internal one-shot import: `add_subdirectory(<nano-ros>)` + include the
# cmake-fn helpers. Idempotent — second call is a no-op.
# ---------------------------------------------------------------------------
function(_nros_import_once nano_ros_root)
    if(TARGET NanoRos::NanoRosCpp OR TARGET NanoRos::NanoRos)
        return()
    endif()
    # The cmake-fn modules need NANO_ROS_PLATFORM / NANO_ROS_RMW visible
    # to the `add_subdirectory()` body; callers must have set them on
    # PARENT_SCOPE before calling _nros_import_once (workspace-root does
    # this in nano_ros_workspace(); standalone does this in the guard).
    # We rely on directory-scope visibility — both call sites set them
    # at top of their function bodies, so child directories inherit.
    add_subdirectory("${nano_ros_root}" "${CMAKE_BINARY_DIR}/nano_ros")
    include("${nano_ros_root}/cmake/NanoRosNodeRegister.cmake")
    include("${nano_ros_root}/cmake/NanoRosEntry.cmake")
endfunction()

# ---------------------------------------------------------------------------
# _nano_ros_order_subdirs(<ws_root> <subdirs> <out_var>) — phase-348 W4
#
# Reorder `subdirs` so each package follows the workspace packages it depends
# on, via `nros ws order --subdir …`. Asks the CLI rather than parsing
# package.xml here: the dependency scan already exists in one place, and cmake
# growing a second `<depend>` reader is the two-derivations defect.
#
# The CLI filters the requested set out of the FULL workspace order, so a
# package that sits between two requested ones still orders them correctly even
# though it is not itself in the list (a bringup package, typically — it is
# passed as SYSTEM rather than as a subdir).
# ---------------------------------------------------------------------------
function(_nano_ros_order_subdirs ws_root subdirs out_var)
    if(NOT _NANO_ROS_CODEGEN_TOOL)
        include("${_NROS_WORKSPACE_DIR}/NanoRosBootstrapCodegen.cmake")
        nros_bootstrap_codegen()
    endif()
    if(NOT _NANO_ROS_CODEGEN_TOOL)
        message(FATAL_ERROR
            "nano_ros_workspace(ORDER_FROM_DEPENDS): no `nros` binary — the "
            "order is derived by the CLI, not parsed here. Run "
            "`./scripts/bootstrap.sh` (contributors: `just setup-cli`) and "
            "`source ./activate.sh`.")
    endif()

    set(_args "")
    foreach(_s IN LISTS subdirs)
        list(APPEND _args --subdir "${_s}")
    endforeach()

    execute_process(
        COMMAND "${_NANO_ROS_CODEGEN_TOOL}" ws order
                --workspace "${ws_root}" ${_args}
        OUTPUT_VARIABLE _ordered
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    # A cycle, or a subdir naming no package, is FATAL rather than a fallback
    # to the authored order: silently building in the order someone happened to
    # type is how the constraint stops being checked at all.
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano_ros_workspace(ORDER_FROM_DEPENDS): `nros ws order` failed "
            "(${_rc}).\n${_err}")
    endif()

    string(REPLACE "\n" ";" _lines "${_ordered}")
    set(_out "")
    foreach(_l IN LISTS _lines)
        if(NOT _l STREQUAL "")
            list(APPEND _out "${_l}")
        endif()
    endforeach()

    list(LENGTH subdirs _want)
    list(LENGTH _out _got)
    if(NOT _want EQUAL _got)
        message(FATAL_ERROR
            "nano_ros_workspace(ORDER_FROM_DEPENDS): asked to order ${_want} "
            "subdir(s), got ${_got} back. Refusing to build a set that is not "
            "the one requested.")
    endif()

    set(${out_var} "${_out}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# Public — `nano_ros_workspace(SYSTEM … BACKEND … PLATFORM … SUBDIRS …
#                              [ORDER_FROM_DEPENDS])`
# ---------------------------------------------------------------------------
function(nano_ros_workspace)
    cmake_parse_arguments(_NRW
        "ORDER_FROM_DEPENDS"
        "SYSTEM;BACKEND;PLATFORM;EDITION;NANO_ROS_ROOT;WORKSPACE_ROOT"
        "SUBDIRS"
        ${ARGN})

    # Where the WORKSPACE is, as opposed to where this root happens to sit.
    #
    # phase-383 W10.a. Every hand-written root lives AT the workspace root, so
    # `CMAKE_SOURCE_DIR` was both, and the code below used it for both. A
    # GENERATED root does not: RFC-0065 D3/D8 put it under `build/<coord>/`,
    # which made the bringup lookup search the build directory —
    #
    #   nano_ros_workspace: could not resolve the capability axes of SYSTEM
    #   'demo_bringup': no bringup pkg named 'demo_bringup' in
    #   .../examples/workspaces/c/build/posix-zenoh
    #
    # — the first configure of the first cmake workspace `nros build` tried.
    #
    # Defaulting to `CMAKE_SOURCE_DIR` keeps every hand-written root building
    # exactly as before, so this is additive during the migration D13 sequences.
    # SUBDIRS stay relative to THIS file (the generator writes them that way),
    # so they deliberately keep using `CMAKE_SOURCE_DIR`.
    if(NOT _NRW_WORKSPACE_ROOT)
        set(_NRW_WORKSPACE_ROOT "${CMAKE_SOURCE_DIR}")
    endif()
    # The generator writes it RELATIVE, so the generated file stays
    # byte-identical across machines (W3.c). Resolve it here, against the
    # calling list file, because everything downstream hands it to a tool that
    # would otherwise resolve it against its OWN working directory: `nros config
    # show --workspace ../..` reported `no bringup pkg named 'demo_bringup' in
    # ../..`, the same failure one layer along. Absolute input is a no-op, so
    # the hand-written default is unaffected.
    get_filename_component(_NRW_WORKSPACE_ROOT "${_NRW_WORKSPACE_ROOT}"
        ABSOLUTE BASE_DIR "${CMAKE_CURRENT_SOURCE_DIR}")

    # issue 0949 — PUBLISH it. `nros_resolve_board_facts` looks for the
    # workspace in this order:
    #
    #     NROS_WORKSPACE_DIR -> APPLICATION_SOURCE_DIR -> CMAKE_SOURCE_DIR
    #
    # and NOTHING SET THE FIRST ONE. So a GENERATED root fell through to
    # `CMAKE_SOURCE_DIR`, which for RFC-0065 D8 is `build/<coord>/` — a
    # directory with no `system.toml`. Board facts were therefore never
    # delivered for a migrated workspace: no NROS_BOARD, no NROS_BOARD_TOML, no
    # NROS_NETSTACK, for embedded images too. The resolution above already knew
    # the right answer; it just kept it to itself.
    #
    # CACHE INTERNAL because board-facts resolves in a different scope, and a
    # plain `set()` here dies with this function's frame — the `_NROS_ENTRY_DIR`
    # pitfall.
    #
    # NB the near-homonym above: `_NROS_WORKSPACE_DIR` (underscore) is the cmake
    # MODULE directory, unrelated to this. Pre-existing, and worth reading twice.
    set(NROS_WORKSPACE_DIR "${_NRW_WORKSPACE_ROOT}"
        CACHE INTERNAL "issue 0949: the workspace root board-facts resolves from")

    # Defaults: backend = zenoh, platform = posix, ROS edition = humble.
    if(NOT _NRW_BACKEND)
        set(_NRW_BACKEND zenoh)
    endif()
    # phase-368 — a `-DNROS_RMW=<x>` on the configure line used to be
    # SILENTLY overridden by the BACKEND argument: this function stamps the
    # workspace-wide RMW from BACKEND, and the cache variable simply lost.
    # Measured cost: a configure that said cyclonedds linked zenoh with no
    # hint. The BACKEND argument stays authoritative (the workspace root is
    # the one place that declares the system), but losing has to be LOUD.
    # Roots that WANT the flag respected forward it explicitly:
    #     if(NOT DEFINED NROS_RMW)
    #         set(NROS_RMW cyclonedds)
    #     endif()
    #     nano_ros_workspace(BACKEND ${NROS_RMW} …)
    # (the shape the scaffolded template ships).
    if(DEFINED CACHE{NROS_RMW} AND NOT "$CACHE{NROS_RMW}" STREQUAL "${_NRW_BACKEND}")
        message(WARNING
            "nano_ros_workspace: -DNROS_RMW=$CACHE{NROS_RMW} is OVERRIDDEN by this "
            "workspace's `BACKEND ${_NRW_BACKEND}` — the build links ${_NRW_BACKEND}. "
            "To switch the RMW, edit the BACKEND argument in the root CMakeLists.txt "
            "(or make the root forward the flag: `nano_ros_workspace(BACKEND "
            "\${NROS_RMW} …)` guarded by `if(NOT DEFINED NROS_RMW)`).")
    endif()
    if(NOT _NRW_PLATFORM)
        set(_NRW_PLATFORM posix)
    endif()
    # phase-304 W2b (RFC-0056) — the ROS edition axis. Drives BOTH the codegen
    # type-hash AND the runtime `ros-<edition>` keyexpr feature from one value,
    # so they can never disagree. Absent → humble (byte-identical to pre-W2b).
    _nros_resolve_ros_edition("${_NRW_EDITION}" _NRW_EDITION)

    # Resolve the nano-ros root (priority chain in _nros_resolve_root).
    _nros_resolve_root("${_NRW_NANO_ROS_ROOT}"
                       "${CMAKE_CURRENT_SOURCE_DIR}"
                       _nros_root)

    # Stamp the resolution so subdirs + the per-pkg guard reuse it
    # without re-walking. PARENT_SCOPE here = the workspace-root scope.
    set(NANO_ROS_ROOT        "${_nros_root}"      PARENT_SCOPE)
    set(NANO_ROS_PLATFORM    "${_NRW_PLATFORM}"   PARENT_SCOPE)
    set(NANO_ROS_RMW         "${_NRW_BACKEND}"    PARENT_SCOPE)
    set(NROS_RMW             "${_NRW_BACKEND}"    PARENT_SCOPE)
    set(NANO_ROS_ROS_EDITION "${_NRW_EDITION}"    PARENT_SCOPE)

    # Also set them in the local fn scope so _nros_import_once's
    # add_subdirectory body sees them directly (PARENT_SCOPE writes
    # don't reach the current fn frame).
    set(NANO_ROS_ROOT        "${_nros_root}")
    set(NANO_ROS_PLATFORM    "${_NRW_PLATFORM}")
    set(NANO_ROS_RMW         "${_NRW_BACKEND}")
    set(NROS_RMW             "${_NRW_BACKEND}")
    set(NANO_ROS_ROS_EDITION "${_NRW_EDITION}")

    # phase-323 W1 — the capability axes, BEFORE the import.
    #
    # `SYSTEM` already names the bringup; it was simply read too late. The
    # metadata call below runs AFTER `_nros_import_once`, but `nros-c` /
    # `nros-cpp` read `NANO_ROS_FEATURES` during the `add_subdirectory` body
    # (`set(_caps ${NANO_ROS_FEATURES})`), so a value set afterwards is a value
    # nobody sees. The workspace cache read `NANO_ROS_FEATURES:STRING=` and NO
    # declared capability reached the C/C++ build — issue 0353.
    #
    # This is the same treatment `BACKEND` already gets a few lines up, and for
    # the same reason: the axis has to be resolved before the import that
    # consumes it.
    #
    # The list comes from the CLI rather than from parsing `system.toml` here,
    # because `SystemToml::capability_enabled` is the SSoT accessor — it honours
    # the generic `[system].features` list AND the deprecated typed blocks, and
    # a cmake-side regex would be exactly the second source phase-314 spent its
    # length removing.
    if(_NRW_SYSTEM)
        if(NOT NROS_BIN)
            include("${_nros_root}/cmake/NanoRosCodegenCore.cmake")
            nros_resolve_cli(NROS_BIN CONTEXT "nano_ros_workspace")
        endif()
        set(_caps_cmake "${CMAKE_BINARY_DIR}/nros_capabilities.cmake")
        execute_process(
            COMMAND "${NROS_BIN}" config show
                    --workspace "${_NRW_WORKSPACE_ROOT}"
                    --system "${_NRW_SYSTEM}"
                    --format cmake
            OUTPUT_FILE "${_caps_cmake}"
            RESULT_VARIABLE _caps_rc
            ERROR_VARIABLE  _caps_err)
        if(NOT _caps_rc EQUAL 0)
            # Fail loudly. A capability that silently fails to resolve is the
            # defect this wave closes, and a warning here would recreate it.
            message(FATAL_ERROR
                "nano_ros_workspace: could not resolve the capability axes of "
                "SYSTEM '${_NRW_SYSTEM}':\n${_caps_err}")
        endif()
        include("${_caps_cmake}")
        # Issue 0745 — LOWER the axes too: `nros_lower_system_features` was
        # defined (phase-261 W5) but called from no path, so the C/C++
        # lowering (e.g. NROS_SYSTEM_PARAM_SERVICES for ComponentNode's
        # launch-seed adoption) never reached component TUs.
        include("${_nros_root}/cmake/NanoRosCapabilities.cmake")
        nros_lower_system_features("${NANO_ROS_FEATURES}")
        # Re-configure when the declaration changes.
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
            "${_NRW_WORKSPACE_ROOT}/src/${_NRW_SYSTEM}/system.toml")
        set(NANO_ROS_FEATURES "${NANO_ROS_FEATURES}" PARENT_SCOPE)
    endif()

    _nros_import_once("${_nros_root}")

    # Optional: workspace metadata for `nros plan` consumption. SYSTEM
    # arg threads through; if absent we skip — workspaces without a
    # Bringup pkg are valid (single-Entry self-bringup mode).
    if(_NRW_SYSTEM)
        include("${_nros_root}/cmake/nano_ros_workspace_metadata.cmake")
        nano_ros_workspace_metadata(SYSTEM "${_NRW_SYSTEM}"
                                    WORKSPACE_ROOT "${_NRW_WORKSPACE_ROOT}")
    endif()

    # phase-348 W4 — ORDER_FROM_DEPENDS derives the add_subdirectory() order
    # from each package's `<depend>` tags instead of trusting the order the
    # SUBDIRS list happens to be written in.
    #
    # The SET stays authored and the ORDER becomes derived, deliberately. A
    # workspace's SUBDIRS list is filtered by PLATFORM (`if(NANO_ROS_BOARD
    # STREQUAL …) list(APPEND …)`), and which board is active is a SELECTION,
    # not a dependency — no `<depend>` can express it. So discovery replaces
    # the half it can prove and leaves the half it cannot.
    #
    # What this removes is the standing comment in every workspace CMakeLists,
    # "Node pkgs BEFORE entries so the entry codegen sees their
    # nano_ros_node_register metadata" — a real constraint that the entry
    # packages ALREADY state as `<exec_depend>talker_pkg</exec_depend>`, and
    # that a hand-maintained list can silently get wrong.
    if(_NRW_ORDER_FROM_DEPENDS AND _NRW_SUBDIRS)
        _nano_ros_order_subdirs("${_NRW_WORKSPACE_ROOT}" "${_NRW_SUBDIRS}" _NRW_SUBDIRS)
    endif()

    # A source dir OUTSIDE this build tree needs an explicit BINARY dir.
    #
    # phase-383 W10.a — the generated root sits in `build/<coord>/` and its
    # SUBDIRS point back OUT (`../../src/talker_pkg`), which `add_subdirectory`
    # rejects without a second argument. A hand-written root's subdirs are all
    # below it, so this never came up. Derive the binary dir from the package
    # NAME rather than the path, so `../../src/talker_pkg` does not become a
    # binary tree with `..` components in it.
    foreach(_sub IN LISTS _NRW_SUBDIRS)
        if(IS_ABSOLUTE "${_sub}")
            set(_nrw_src "${_sub}")
        else()
            set(_nrw_src "${_NRW_WORKSPACE_ROOT}/${_sub}")
        endif()
        get_filename_component(_nrw_src "${_nrw_src}" ABSOLUTE)
        string(FIND "${_nrw_src}" "${CMAKE_SOURCE_DIR}/" _nrw_below)
        if(_nrw_below EQUAL 0)
            add_subdirectory("${_nrw_src}")
        else()
            get_filename_component(_nrw_name "${_nrw_src}" NAME)
            add_subdirectory("${_nrw_src}" "${CMAKE_BINARY_DIR}/pkg/${_nrw_name}")
        endif()
    endforeach()

    # Phase 241 W11 (Option D) — if this configure contains a Rust Node pkg, synthesise the
    # per-configure runtime umbrella (nros-cpp + all workspace Rust nodes, one staticlib)
    # and re-point NanoRos::NanoRosCpp at it. No-op for pure-C / pure-C++ workspaces. Runs
    # AFTER the SUBDIRS loop so nros-metadata.json lists every registered node; the umbrella
    # archive swap is an INTERFACE property edit, evaluated at generate time.
    include("${_nros_root}/cmake/NanoRosRuntimeCrate.cmake")
    nros_synth_runtime_umbrella(BACKEND "${_NRW_BACKEND}" PLATFORM "${_NRW_PLATFORM}" EDITION "${_NRW_EDITION}")
endfunction()

# ---------------------------------------------------------------------------
# Public — `nano_ros_workspace_pkg_guard([NANO_ROS_ROOT <path>])`
#
# Top-of-CMakeLists call in every Node + Entry pkg subdir. Inside a
# workspace it is a no-op; standalone it bootstraps the same way the
# workspace root would.
# ---------------------------------------------------------------------------
function(nano_ros_workspace_pkg_guard)
    if(TARGET NanoRos::NanoRosCpp OR TARGET NanoRos::NanoRos)
        return()
    endif()

    cmake_parse_arguments(_NRG
        ""
        "NANO_ROS_ROOT;BACKEND;PLATFORM"
        ""
        ${ARGN})

    if(NOT _NRG_BACKEND)
        if(NROS_RMW)
            set(_NRG_BACKEND "${NROS_RMW}")
        else()
            set(_NRG_BACKEND zenoh)
        endif()
    endif()
    if(NOT _NRG_PLATFORM)
        if(NANO_ROS_PLATFORM)
            set(_NRG_PLATFORM "${NANO_ROS_PLATFORM}")
        else()
            set(_NRG_PLATFORM posix)
        endif()
    endif()

    _nros_resolve_root("${_NRG_NANO_ROS_ROOT}"
                       "${CMAKE_CURRENT_SOURCE_DIR}"
                       _nros_root)

    # Direct-scope sets so the cmake-fn helpers + add_subdirectory body
    # see them. PARENT_SCOPE keeps them visible to the rest of the
    # pkg's CMakeLists too.
    set(NANO_ROS_ROOT     "${_nros_root}"     PARENT_SCOPE)
    set(NANO_ROS_PLATFORM "${_NRG_PLATFORM}"  PARENT_SCOPE)
    set(NANO_ROS_RMW      "${_NRG_BACKEND}"   PARENT_SCOPE)
    set(NROS_RMW          "${_NRG_BACKEND}"   PARENT_SCOPE)
    set(NANO_ROS_ROOT     "${_nros_root}")
    set(NANO_ROS_PLATFORM "${_NRG_PLATFORM}")
    set(NANO_ROS_RMW      "${_NRG_BACKEND}")
    set(NROS_RMW          "${_NRG_BACKEND}")

    _nros_import_once("${_nros_root}")
endfunction()
