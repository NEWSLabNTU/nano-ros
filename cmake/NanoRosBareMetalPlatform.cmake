# cmake/NanoRosBareMetalPlatform.cmake — issue 1512.
#
# `NANO_ROS_BOARD` -> the `nros-c` / `nros-cpp` cargo feature that selects this
# board's platform implementation.
#
# ## Why this file exists at all
#
# Every other platform answers "which `nros-platform/platform-X`?" with one
# name, so `nros_feature_set()`'s PLATFORM ladder can spell it inline. Bare
# metal cannot: a no-RTOS port IS the board (its clock is that board's timer,
# its net is that board's MAC, and there is no kernel in between to abstract
# either), so `nros-platform` carries three board-specific features where the
# RTOSes carry one apiece. See the long block in `packages/api/nros-c/
# Cargo.toml` for why the C road has to spell the board at all, when the Rust
# road does not.
#
# ## Which vocabulary the key is in
#
# The CMAKE BOARD token — the `<board>` in `cmake/board/nano-ros-board-
# <board>.cmake`, which is what `cmake/platform/nano-ros-baremetal.cmake`
# dispatches on and what its FATAL_ERROR tells a caller to set. That is NOT the
# board-descriptor `names` vocabulary: `packages/boards/nros-board-mps2-an385/
# nros-board.toml` lists `baremetal`, `bare-metal`, `qemu-mps2-an385` and
# `rtic-mps2-an385` and does NOT list `mps2-an385-baremetal`, so resolving this
# through the descriptors would find nothing. One namespace per file; this one
# is CMake's.
#
# ## Why an authored map, and what holds it honest
#
# The value is derivable in principle — the board CRATE selects it
# unconditionally (`nros-board-mps2-an385`'s `nros-platform = { …, features =
# ["platform-mps2-an385", …] }`), which is the very fact the Rust road relies
# on — but deriving it here means regex-parsing a Cargo.toml dependency line
# from CMake, and a reformat there would break a build over here. So the map is
# authored and `check-baremetal-platform-arms` checks it in BOTH directions:
# every `cmake/board/nano-ros-board-*-baremetal.cmake` has a row, every row's
# feature exists in `nros-c` AND `nros-cpp`, and `nros-platform`'s bare-metal
# feature roster is exactly the set of values used. A missing row is a configure
# FATAL_ERROR rather than a fall-through, which is the failure mode issue 1512
# found: the old ladder reached `elseif(_cross)` and asked cargo for
# `platform-baremetal`, a feature no crate has.
include_guard(GLOBAL)

# The rows. `<cmake board token>` `<nros-c/nros-cpp feature>`, in pairs.
set(_NROS_BAREMETAL_BOARD_FEATURES
    mps2-an385-baremetal platform-mps2-an385
    esp32-c3-baremetal   platform-esp32-qemu
    CACHE INTERNAL "issue 1512 — bare-metal CMAKE board token -> nros-c platform feature")

# nros_baremetal_platform_feature(<out_var> <board> <context>)
#
# Sets <out_var> in the caller's scope to the `nros-c` / `nros-cpp` platform
# feature for bare-metal board <board>. FATAL_ERROR when <board> is empty or
# unknown; <context> names the caller in the diagnostic.
function(nros_baremetal_platform_feature out_var board context)
    set(_known "")
    set(_hit "")
    set(_key "")
    foreach(_tok IN LISTS _NROS_BAREMETAL_BOARD_FEATURES)
        if(_key STREQUAL "")
            set(_key "${_tok}")
            list(APPEND _known "${_tok}")
        else()
            if(_key STREQUAL "${board}")
                set(_hit "${_tok}")
            endif()
            set(_key "")
        endif()
    endforeach()
    string(REPLACE ";" ", " _known_flat "${_known}")
    if(board STREQUAL "")
        message(FATAL_ERROR
            "${context}: a bare-metal build must set NANO_ROS_BOARD before the "
            "runtime is imported. Bare metal is the one platform with no single "
            "`nros-platform/platform-*` feature — the board IS the port — so "
            "the C/C++ staticlib cannot be configured without knowing which "
            "board it is for. Known: ${_known_flat}.")
    endif()
    if(_hit STREQUAL "")
        message(FATAL_ERROR
            "${context}: no bare-metal platform feature for NANO_ROS_BOARD "
            "'${board}' (known: ${_known_flat}). Add the row to "
            "cmake/NanoRosBareMetalPlatform.cmake together with the matching "
            "`platform-*` arm on nros-c and nros-cpp; "
            "`just check baremetal-platform-arms` checks the three agree.")
    endif()
    set(${out_var} "${_hit}" PARENT_SCOPE)
endfunction()
