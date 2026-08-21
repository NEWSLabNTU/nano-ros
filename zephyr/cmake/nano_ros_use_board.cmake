# Phase 215.B — nano_ros_use_board(<name>)
#
# Layer a board crate's `board.cmake` sidecar manifest into the
# downstream Zephyr app build. Replaces hand-curated EXTRA_CONF_FILE /
# DTC_OVERLAY_FILE / BOARD wiring on the consumer side. See
# `docs/roadmap/phase-215-board-crate-as-importable-unit.md` §215.B.
#
# Usage (consumer app, BEFORE find_package(Zephyr)):
#   include(${ZEPHYR_NROS_MODULE_DIR}/cmake/nano_ros_use_board.cmake)
#   nano_ros_use_board(fvp-aemv8r-smp)
#   find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
#
# When nano-ros is on ZEPHYR_EXTRA_MODULES, zephyr/CMakeLists.txt
# include()s this file so downstream apps get the fn for free.

# Repo root = parent of zephyr/ = grandparent of zephyr/cmake/.
# Resolve from THIS file's location — never hardcode an absolute path.
if(NOT DEFINED NROS_REPO_DIR)
    get_filename_component(NROS_REPO_DIR
        "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
    set(NROS_REPO_DIR "${NROS_REPO_DIR}" CACHE PATH
        "nano-ros repo root (parent of zephyr/ module dir)")
endif()

function(nano_ros_use_board NAME)
    # 215.B.3 — call-order guard. EXTRA_CONF_FILE / BOARD / DTC_OVERLAY_FILE
    # must propagate BEFORE Zephyr's board-resolution phase. If
    # find_package(Zephyr) already ran (ZEPHYR_VERSION is set as a
    # side-effect), the variables we set here are ignored and the build
    # silently uses whatever the consumer hardcoded. Fail loudly instead.
    if(DEFINED ZEPHYR_VERSION)
        message(FATAL_ERROR
            "nano_ros_use_board(${NAME}) called AFTER find_package(Zephyr). "
            "Move the call ABOVE find_package(Zephyr) so BOARD / "
            "EXTRA_CONF_FILE / DTC_OVERLAY_FILE land before Zephyr's "
            "board-resolution phase.")
    endif()

    # Board keys resolve in TWO shapes, tried in this order. phase-337 W9.a
    # added the second, and RFC-0064 expects it to become the common one.
    #
    #   1. a board CRATE          packages/boards/nros-board-<NAME>/board.cmake
    #   2. a CONF BUNDLE under a  packages/boards/nros-board-<family>/
    #      family crate             boards/<NAME>/board.cmake
    #
    # A per-board crate for a Zephyr board is the duplication this phase exists
    # to remove: Zephyr already owns boot, MMU, net stack and drivers, so what
    # a "board crate" actually held was a prj.conf, a DTS overlay and this
    # manifest — a config bundle wearing a Cargo.toml. `fvp-aemv8r-smp` was the
    # last one and its Rust half (`Config` + a `run` loop) had ZERO consumers.
    #
    # Shape 1 stays first and stays supported: out-of-tree boards that DO carry
    # real Rust (a `BoardEntry`, a driver) are still crates, and RFC-0064 says
    # so. The key is what the consumer names either way — folding a board must
    # not change `nano_ros_use_board(<name>)` at any call site, which is the
    # whole reason the lookup widened instead of the callers moving.
    #
    # The CLI's `locate_board_crate` (nros-cli-core/src/cmd/board.rs) implements
    # the SAME lookup — in-tree roots plus `NROS_EXTRA_BOARD_PATH` — for
    # `nros board info` / `nros setup board`. Two languages, so no shared code
    # is possible — but they must not diverge, and `board_import_fvp`
    # (tests/fvp_smoke.rs) exercises this path.
    #
    # Search roots: the in-tree boards dir, then every entry of
    # NROS_EXTRA_BOARD_PATH (cache var wins; else the environment —
    # PATH-style ':' separated). This is the out-of-tree escape hatch: a
    # consumer board no longer has to be copied into the checkout. Board keys
    # stay GLOBAL — a name found under more than one root is a fatal
    # ambiguity, never shadowed by search order.
    set(_board_roots "${NROS_REPO_DIR}/packages/boards")
    set(_extra_raw "${NROS_EXTRA_BOARD_PATH}")
    if(_extra_raw STREQUAL "" AND DEFINED ENV{NROS_EXTRA_BOARD_PATH})
        set(_extra_raw "$ENV{NROS_EXTRA_BOARD_PATH}")
    endif()
    if(NOT _extra_raw STREQUAL "")
        string(REPLACE ":" ";" _extra_list "${_extra_raw}")
        foreach(_extra IN LISTS _extra_list)
            if(NOT _extra STREQUAL "" AND IS_DIRECTORY "${_extra}")
                list(APPEND _board_roots "${_extra}")
            endif()
        endforeach()
    endif()

    set(_board_candidates "")
    foreach(_root IN LISTS _board_roots)
        if(EXISTS "${_root}/nros-board-${NAME}/board.cmake")
            list(APPEND _board_candidates "${_root}/nros-board-${NAME}/board.cmake")
        endif()
        file(GLOB _bundle_candidates "${_root}/*/boards/${NAME}/board.cmake")
        list(APPEND _board_candidates ${_bundle_candidates})
    endforeach()
    list(LENGTH _board_candidates _board_count)
    if(_board_count GREATER 1)
        message(FATAL_ERROR
            "nano_ros_use_board(${NAME}): ambiguous — ${NAME} resolves under "
            "more than one board search root (packages/boards + "
            "NROS_EXTRA_BOARD_PATH):\n"
            "  ${_board_candidates}\n"
            "Board keys are global; rename one.")
    elseif(_board_count EQUAL 1)
        list(GET _board_candidates 0 _board_cmake)
        get_filename_component(_board_dir "${_board_cmake}" DIRECTORY)
    else()
        message(FATAL_ERROR
            "nano_ros_use_board(${NAME}): no board.cmake for board ${NAME}.\n"
            "Looked in these roots for a board crate "
            "`nros-board-${NAME}/board.cmake` and a conf bundle "
            "`*/boards/${NAME}/board.cmake`:\n"
            "  ${_board_roots}\n"
            "Check the board name (or set NROS_EXTRA_BOARD_PATH for an "
            "out-of-tree board), or run `nros board info ${NAME}` "
            "to validate the manifest.")
    endif()
    include("${_board_cmake}")

    # 4. BOARD — set if empty, warn on mismatch. CACHE FORCE so it
    # propagates to find_package(Zephyr)'s board-resolution scope.
    if(NOT BOARD)
        set(BOARD "${NROS_BOARD_ZEPHYR_ID}" CACHE STRING
            "Zephyr BOARD (set by nano_ros_use_board(${NAME}))" FORCE)
    elseif(NOT "${BOARD}" STREQUAL "${NROS_BOARD_ZEPHYR_ID}")
        message(WARNING
            "nano_ros_use_board(${NAME}): BOARD=${BOARD} overrides the "
            "board crate's ZEPHYR_ID=${NROS_BOARD_ZEPHYR_ID}. Proceeding "
            "with the user value; per-board overlays may not apply.")
    endif()

    # 5. EXTRA_CONF_FILE — append base prj.conf + per-board hwv2 fragment.
    list(APPEND EXTRA_CONF_FILE
        "${NROS_BOARD_PRJ_CONF}"
        "${NROS_BOARD_BOARD_CONF}")
    set(EXTRA_CONF_FILE "${EXTRA_CONF_FILE}" PARENT_SCOPE)

    # 6. DTC_OVERLAY_FILE — append per-board DTS overlay.
    #
    # phase-292 W2 (ASI wall #2) — setting DTC_OVERLAY_FILE DISABLES Zephyr's
    # automatic discovery of the APP's own overlays (`<app>/boards/<board>.
    # overlay`, `<app>/app.overlay`) — Zephyr only auto-discovers when the
    # variable is unset. The FVP board crate deliberately leaves ethernet to
    # the app overlay ("users override at the example-app level"), so
    # swallowing the app overlay silently killed the consumer's NIC
    # (`net_if: There is no network interface`, ASI phase-3 W3 first boot).
    # Replicate Zephyr's app-overlay convention here BEFORE appending ours,
    # only when the consumer has not already curated the list.
    if(NOT DTC_OVERLAY_FILE)
        string(REGEX REPLACE "[/@]" "_" _nros_board_norm "${NROS_BOARD_ZEPHYR_ID}")
        foreach(_app_ovl
                "${CMAKE_CURRENT_SOURCE_DIR}/boards/${_nros_board_norm}.overlay"
                "${CMAKE_CURRENT_SOURCE_DIR}/app.overlay")
            if(EXISTS "${_app_ovl}")
                list(APPEND DTC_OVERLAY_FILE "${_app_ovl}")
            endif()
        endforeach()
    endif()
    list(APPEND DTC_OVERLAY_FILE "${NROS_BOARD_BOARD_OVERLAY}")
    set(DTC_OVERLAY_FILE "${DTC_OVERLAY_FILE}" PARENT_SCOPE)

    # 7. NANO_ROS_RMW — board's default if the consumer didn't pin one.
    if(NOT DEFINED NANO_ROS_RMW)
        set(NANO_ROS_RMW "${NROS_BOARD_DEFAULT_RMW}" CACHE STRING
            "nano-ros RMW backend (default from board ${NAME})")
    endif()

    # 7b. RMW-common Kconfig — phase-292 W2 (ASI wall #4). On Zephyr 4.x the
    # `-S nros-<rmw>` snippet carries the RMW-common conf (worker stacks,
    # net buffers, TLS...); 3.7 has no snippet support and board-crate
    # consumers got NONE of it — Cyclone's pthread workers then run on the
    # 2 KiB CONFIG_DYNAMIC_THREAD_STACK_SIZE default and overflow into a
    # wild jump past z_mapped_end during dds_create_participant. The
    # snippet .conf files are plain Kconfig fragments, so merge them here
    # for every Zephyr version; a consumer merging the snippet too is
    # harmless (identical values).
    file(GLOB _nros_rmw_common_conf
        "${NROS_REPO_DIR}/zephyr/snippets/nros-${NANO_ROS_RMW}/*.conf")
    if(_nros_rmw_common_conf)
        list(APPEND EXTRA_CONF_FILE ${_nros_rmw_common_conf})
        set(EXTRA_CONF_FILE "${EXTRA_CONF_FILE}" PARENT_SCOPE)
    endif()

    # 8. Cache the runner so `west fvp run` reads it from CMakeCache.txt
    # (Phase 215.D).
    set(NROS_BOARD_RUNNER "${NROS_BOARD_RUNNER}" CACHE STRING
        "nano-ros board runner (armfvp / qemu / native / …)" FORCE)

    # 9. Phase 215.J.4 — if the board ships a Rust-support Kconfig overlay
    # module (enabling RUST_SUPPORTED for its arch without mutating the
    # consumer's zephyr-lang-rust tree), put it on ZEPHYR_EXTRA_MODULES so the
    # downstream build gets it for free. Must land BEFORE find_package(Zephyr)
    # (the call-order guard above enforces that).
    if(DEFINED NROS_BOARD_RUST_SUPPORT_MODULE
            AND EXISTS "${NROS_BOARD_RUST_SUPPORT_MODULE}/zephyr/module.yml")
        list(APPEND ZEPHYR_EXTRA_MODULES "${NROS_BOARD_RUST_SUPPORT_MODULE}")
        set(ZEPHYR_EXTRA_MODULES "${ZEPHYR_EXTRA_MODULES}" PARENT_SCOPE)
    endif()

    message(STATUS
        "nano_ros_use_board(${NAME}): zephyr_board=${NROS_BOARD_ZEPHYR_ID}, "
        "rmw=${NANO_ROS_RMW}, runner=${NROS_BOARD_RUNNER}")
endfunction()
