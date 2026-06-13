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

    set(_board_dir "${NROS_REPO_DIR}/packages/boards/nros-board-${NAME}")
    set(_board_cmake "${_board_dir}/board.cmake")
    if(NOT EXISTS "${_board_cmake}")
        message(FATAL_ERROR
            "nano_ros_use_board(${NAME}): no board.cmake at\n"
            "  ${_board_cmake}\n"
            "Check the board name, or run `nros board info ${NAME}` "
            "to validate the crate's manifest.")
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
    list(APPEND DTC_OVERLAY_FILE "${NROS_BOARD_BOARD_OVERLAY}")
    set(DTC_OVERLAY_FILE "${DTC_OVERLAY_FILE}" PARENT_SCOPE)

    # 7. NANO_ROS_RMW — board's default if the consumer didn't pin one.
    if(NOT DEFINED NANO_ROS_RMW)
        set(NANO_ROS_RMW "${NROS_BOARD_DEFAULT_RMW}" CACHE STRING
            "nano-ros RMW backend (default from nros-board-${NAME})")
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
