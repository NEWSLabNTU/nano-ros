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

# nros_ament_port_default_features(<out_var>)  — phase-488
#
#   <out_var> : set to the capability axes a configure that DECLARED none gets,
#               which is nothing except on the one road below.
#
# A stock ROS 2 C++ package built on a host — the ament road: its own
# `package.xml` (or, for an umbrella over a colcon-layout `src/`, one of
# `src/*/package.xml`) names `ament_cmake` as a buildtool, and it pulls nano-ros
# in with `add_subdirectory()` — gets `param_services`. Upstream rclcpp runs the six
# parameter services in every node, so a ported node is written against them
# (`ros2 param get` against it timed out until this, on the Autoware
# `autoware_external_cmd_selector` port), and the package has no `system.toml`
# to declare them in. A package that depends on `rclcpp_lifecycle` gets
# `lifecycle` as well, since upstream's managed node serves its lifecycle
# services the same way. The road is a property of the CONSUMER's source tree,
# asked here before the runtime is configured, never of the build tree.
# A declaration still wins: any `NROS_FEATURES` the consumer sets, `-D`s or
# caches, including the empty one, is taken as written.
function(nros_ament_port_default_features out_var)
    set(${out_var} "" PARENT_SCOPE)
    if(DEFINED NROS_FEATURES OR NOT NROS_PLATFORM STREQUAL "posix"
       OR CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
        return()
    endif()
    # The package itself, or a colcon-layout umbrella over `src/<pkg>/`.
    file(GLOB _pxs "${CMAKE_SOURCE_DIR}/package.xml" "${CMAKE_SOURCE_DIR}/src/*/package.xml")
    set(_px "")
    set(_lifecycle "")
    foreach(_candidate IN LISTS _pxs)
        file(READ "${_candidate}" _body)
        if(_body MATCHES "<buildtool_depend>[ \t\r\n]*ament_cmake[A-Za-z_]*[ \t\r\n]*</buildtool_depend>")
            if(_px STREQUAL "")
                set(_px "${_candidate}")
            endif()
            # A managed node: upstream's rclcpp_lifecycle serves the five
            # lifecycle services, so the runtime needs `lifecycle` too
            # (phase-488 node 2, Nav2 — without it the image did not LINK).
            if(_body MATCHES "<(build_|exec_)?depend>[ \t\r\n]*rclcpp_lifecycle[ \t\r\n]*</")
                set(_lifecycle "${_candidate}")
            endif()
        endif()
    endforeach()
    if(_px STREQUAL "")
        return()
    endif()
    set(_features "param_services")
    if(NOT _lifecycle STREQUAL "")
        list(APPEND _features "lifecycle")
    endif()
    message(STATUS
        "nano-ros: ${_px} is an ament package on a host, so its runtime carries "
        "`${_features}` as upstream rclcpp's does. Set NROS_FEATURES to declare "
        "the capabilities yourself (phase-488).")
    set(${out_var} "${_features}" PARENT_SCOPE)
endfunction()

# nros_lower_system_features(<features>)  — phase-261 W5
#
#   <features> : a CMake list of declared capability axes by name, e.g.
#                "safety" or "safety;param_services". The C/C++ projection of
#                `system.toml` `[system].features = [...]` (RFC-0004). Set it
#                via `set(NROS_FEATURES "safety")` BEFORE
#                `add_subdirectory(<nano-ros>)`, or let the bake emit it.
#
# Lowers each axis to its CMake build knob (`cmake_token`) — the analog of
# `NROS_RMW` — so a declared capability flips the option `nros-cpp` reads at
# `add_subdirectory` time, not just the informational `system_config.h` `#define`.
#
# This map MIRRORS the Rust `Capability` registry (cargo-nano-ros
# `capability_resolver`), the SSoT for `(declared, cmake_token)`. A Rust drift
# test (`cmake_capability_map_matches_registry`) asserts the two never skew, so
# adding a row there + an arm here is the only edit a future axis needs.
#
# Known axes mirror the registry: `safety` → `NROS_SAFETY_E2E`;
# `param_services` and `params` are known but carry NO `cmake_token`
# (informational `#define` only), so it lowers to no option. An unknown name is a hard error (typo guard),
# matching the Rust `validate_and_warn_capabilities`.
function(nros_lower_system_features features)
    foreach(_feat IN LISTS features)
        if(_feat STREQUAL "")
            # empty list element — skip.
        elseif(_feat STREQUAL "safety")
            set(NROS_SAFETY_E2E ON CACHE BOOL
                "nano-ros: E2E message-integrity (CRC) — from [system].features" FORCE)
        elseif(_feat STREQUAL "param_services")
            # Known axis, no CMake OPTION. Issue 0745 — the define is a
            # directory-wide compile definition (not just system_config.h),
            # because it has to reach COMPONENT TUs, which never include the
            # entry's system_config.h. Capabilities resolve BEFORE the runtime
            # import (issue 0353), so this reaches every subdirectory of the
            # workspace configure.
            #
            # phase-426 W4 — no nano-ros HEADER branches on it any more. It
            # gated the two launch-seed adoption helpers, which existed only
            # because the C++ node types owned their own parameter store; W4
            # deleted the stores and the helpers, and the node parameter
            # forwarders are now unconditional (they answer
            # `NROS_CPP_RET_UNSUPPORTED` when no store was compiled in, so a
            # ported node still COMPILES and LINKS on an image that declared no
            # `param_services`). What the define still says is which capability
            # the bringup asked for — read by the compile probe
            # `ros2_param_launch_seed.cpp` and available to user code — and it
            # is measured against C++ layouts by
            # `check-cpp-capability-layout`, whose CAPS list it joined in W4.
            add_compile_definitions(NROS_SYSTEM_PARAM_SERVICES)
        elseif(_feat STREQUAL "params")
            # phase-461 W6 — the parameter store without the six services.
            # Known axis, no CMake knob; the cargo feature (`param-store`) is
            # chosen per image by `nros_feature_set` and on the west road by
            # `zephyr/CMakeLists.txt`. Directory-wide for 0745's reason.
            add_compile_definitions(NROS_SYSTEM_PARAM_STORE)
        elseif(_feat STREQUAL "lifecycle")
            # Phase 269 W2 — Known axis, no CMake knob. The CARGO feature
            # (`lifecycle-services`) is chosen per image from the same list:
            # `nros_feature_set` on the cmake road, `_nros_cap_suffix` in
            # `zephyr/CMakeLists.txt` on the west road (issue 1681 — that road
            # mapped no lifecycle axis, and the entry's
            # `nros_cpp_lifecycle_autostart` did not link). The C/C++ lowering
            # here is only the `#define NROS_SYSTEM_LIFECYCLE` in system_config.h.
        elseif(_feat STREQUAL "rosout")
            # Issue 1589 — Known axis, no CMake knob. The CARGO feature
            # (`rosout`) is chosen per image by `nros_feature_set` on the cmake
            # road and `_nros_cap_suffix` on the west road; it only pulls the
            # bridge in (without it the C/C++ entry points link and answer
            # UNSUPPORTED). The define is directory-wide for the reason 0745
            # gives `param_services`: a component TU never includes the entry's
            # system_config.h.
            add_compile_definitions(NROS_SYSTEM_ROSOUT)
        else()
            message(FATAL_ERROR
                "nros_lower_system_features: unknown capability '${_feat}' in "
                "NROS_FEATURES (known axes: safety, param_services, params, lifecycle, rosout)")
        endif()
    endforeach()
endfunction()
