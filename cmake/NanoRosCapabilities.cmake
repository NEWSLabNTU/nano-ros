# NanoRosCapabilities.cmake — RFC-0042 D2 / phase-241 wave C (C.2).
#
# Lower a board's declared `[board.capabilities]` (the single source of truth in
# its `nros-board.toml`) into the matching `NROS_PLATFORM_HAS_*` compile defines,
# so the cmake-driven in-tree fixture builds derive them from board.toml instead
# of hand-setting them per overlay (the issue-0038 footgun: a heap-capable
# bare-metal board had to remember `-DNROS_PLATFORM_HAS_MALLOC`).
#
# Scope: the `-D`-mechanism platforms (bare-metal / ThreadX). Zephyr's heap/mutex
# stay Kconfig-derived (`CONFIG_HEAP_MEM_POOL_SIZE` / `CONFIG_MULTITHREADING`) and
# FreeRTOS's malloc stays `configSUPPORT_DYNAMIC_ALLOCATION`-derived — forcing a
# `-D` there would decouple the C view from the actual RTOS config. `HAS_ATOMICS`
# is universal today (header-defined); `threads` maps to `NROS_FEATURE_THREADS`,
# which is opt-in and not auto-emitted here.
#
# Reads the SSoT directly with `file(STRINGS)` — no generator, no committed
# fragment. A board with no `[board.capabilities]` block yields no defines (the
# conservative bare-metal default: no heap).
include_guard(GLOBAL)

# nros_board_capability_defines(<board_dir> <out_var>)
#   <board_dir> : dir holding the board's `nros-board.toml`
#   <out_var>   : set in the caller's scope to the list of capability defines
function(nros_board_capability_defines board_dir out_var)
    set(_toml "${board_dir}/nros-board.toml")
    set(_defs "")
    if(EXISTS "${_toml}")
        # `[board.capabilities]` `heap = true` → the board has an allocator, so
        # expose the canonical malloc/free (baremetal.h shims them over the CFFI
        # alloc/dealloc when NROS_PLATFORM_HAS_MALLOC is defined).
        file(STRINGS "${_toml}" _heap_true REGEX "^[ \t]*heap[ \t]*=[ \t]*true[ \t]*$")
        if(_heap_true)
            list(APPEND _defs NROS_PLATFORM_HAS_MALLOC)
        endif()
    endif()
    set(${out_var} "${_defs}" PARENT_SCOPE)
endfunction()

# nros_lower_system_features(<features>)  — phase-261 W5
#
#   <features> : a CMake list of declared capability axes by name, e.g.
#                "safety" or "safety;param_services". The C/C++ projection of
#                `system.toml` `[system].features = [...]` (RFC-0004). Set it
#                via `set(NANO_ROS_FEATURES "safety")` BEFORE
#                `add_subdirectory(<nano-ros>)`, or let the bake emit it.
#
# Lowers each axis to its CMake build knob (`cmake_token`) — the analog of
# `NANO_ROS_RMW` — so a declared capability flips the option `nros-cpp` reads at
# `add_subdirectory` time, not just the informational `system_config.h` `#define`.
#
# This map MIRRORS the Rust `Capability` registry (cargo-nano-ros
# `capability_resolver`), the SSoT for `(declared, cmake_token)`. A Rust drift
# test (`cmake_capability_map_matches_registry`) asserts the two never skew, so
# adding a row there + an arm here is the only edit a future axis needs.
#
# Known axes mirror the registry: `safety` → `NANO_ROS_SAFETY_E2E`;
# `param_services` is known but carries NO `cmake_token` (informational `#define`
# only), so it lowers to no option. An unknown name is a hard error (typo guard),
# matching the Rust `validate_and_warn_capabilities`.
function(nros_lower_system_features features)
    foreach(_feat IN LISTS features)
        if(_feat STREQUAL "")
            # empty list element — skip.
        elseif(_feat STREQUAL "safety")
            set(NANO_ROS_SAFETY_E2E ON CACHE BOOL
                "nano-ros: E2E message-integrity (CRC) — from [system].features" FORCE)
        elseif(_feat STREQUAL "param_services")
            # Known axis, no CMake knob (entry-umbrella-only; the `#define`
            # NROS_SYSTEM_PARAM_SERVICES in system_config.h is its only C/C++ lowering).
        else()
            message(FATAL_ERROR
                "nros_lower_system_features: unknown capability '${_feat}' in "
                "NANO_ROS_FEATURES (known axes: safety, param_services)")
        endif()
    endforeach()
endfunction()
