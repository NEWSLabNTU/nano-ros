# cmake/board/nano-ros-board-esp32-c3.cmake
#
# Phase 138.3 — board overlay for ESP32-C3 (real silicon) + ESP32-C3
# QEMU. Used under NANO_ROS_PLATFORM=baremetal. ESP32-C3 normally
# consumes nano-ros as an ESP-IDF component (Phase 139); this overlay
# is the in-tree shim for non-IDF parents (rare — kept for symmetry
# with the other board overlays).

if(DEFINED _NROS_BOARD_ESP32_C3_INCLUDED)
    return()
endif()
set(_NROS_BOARD_ESP32_C3_INCLUDED TRUE)

function(nros_board_link_app target)
    # ESP32-C3 linking goes through the IDF / ESP-Rust toolchain; both
    # supply their own linker script + startup. The in-tree overlay
    # has nothing extra to wire — kept as a no-op so the per-platform
    # contract (every NANO_ROS_BOARD value resolves to a real module)
    # holds. Phase 139's idf-component shell drives the real link.
    set(_unused "${target}")
endfunction()
