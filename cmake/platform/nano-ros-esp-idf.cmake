# cmake/platform/nano-ros-esp-idf.cmake
#
# Phase 21.6 — ESP-IDF (Espressif's FreeRTOS fork) platform module.
# Single source of truth for the Phase 137 `add_subdirectory(<nano-ros-root>)`
# consumption shape when the umbrella build runs inside an
# `idf_component_register()` block (Phase 139 ESP-IDF integration
# shell).
#
# Unlike `nano-ros-freertos.cmake`, ESP-IDF supplies the kernel,
# heap, lwIP, WiFi/Ethernet drivers, and startup itself — through
# the IDF component manager's `REQUIRES freertos esp_timer
# esp_hw_support esp_system lwip` declaration in
# `packages/core/nros-platform-esp-idf/CMakeLists.txt`. There is no
# board overlay axis here; the user's IDF `sdkconfig` (chip choice,
# WiFi credentials, partition table) replaces the per-board cmake
# overlay that bare-metal FreeRTOS needs.
#
# Contract (Phase 138 §A):
#   NanoRos::Platform                 — INTERFACE alias for the IDF component
#   nros_platform_esp_idf_iface       — concrete INTERFACE behind it
#   nros_platform_link_app(<target>)  — per-app fixup (delegates to IDF)
#   NROS_PLATFORM_LINK_FEATURES       — default link feature set

if(DEFINED _NROS_PLATFORM_ESP_IDF_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_ESP_IDF_INCLUDED TRUE)

# ESP-IDF lwIP ships TCP / UDP unicast / UDP multicast on every chip
# variant; same default set as the FreeRTOS platform module.
set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the ESP-IDF platform")

# `IDF_VERSION` is set only during the main IDF build pass — the
# component-discovery script-mode pass does NOT have it set, but
# the `idf_component_register` command IS injected at every IDF
# scope. Gate the standalone-fatal-error on the more reliable
# command check.
if(NOT COMMAND idf_component_register)
    message(FATAL_ERROR
        "nano-ros-esp-idf: NANO_ROS_PLATFORM=esp-idf must be used inside "
        "an ESP-IDF project. Register nano-ros via the Phase 139 shell "
        "at integrations/esp-idf/ (or symlink/copy it under your IDF "
        "project's components/ directory).")
endif()

# ---------------------------------------------------------------------------
# User-facing nano-ros helpers (config + link). Loaded for parity with
# the other platform modules; `nano_ros_read_config()` etc. work
# identically inside an IDF build.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/NanoRosReadConfig.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/NanoRosLink.cmake")

# ---------------------------------------------------------------------------
# Codegen — same pre-cache pattern as nano-ros-freertos.cmake. ESP-IDF
# cross builds cannot build the Rust codegen tool with the Xtensa /
# RISC-V toolchain, so consumers must point `_NANO_ROS_CODEGEN_TOOL`
# at a host-built `nros-codegen` binary (run a `posix` configure
# first; pass `-D_NANO_ROS_CODEGEN_TOOL=…` to the IDF build).
# ---------------------------------------------------------------------------
set(_nros_esp_idf_codegen_module
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_esp_idf_codegen_module}")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
    if(NOT DEFINED CACHE{_NANO_ROS_CODEGEN_TOOL})
        find_program(_NANO_ROS_CODEGEN_TOOL nros-codegen
            PATHS
                "${_NANO_ROS_PREFIX}/build/install/bin"
                "${_NANO_ROS_PREFIX}/bin")
        if(_NANO_ROS_CODEGEN_TOOL)
            set(_NANO_ROS_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}"
                CACHE INTERNAL "Path to nros C codegen tool")
        endif()
    endif()
    include("${_nros_esp_idf_codegen_module}")
endif()

# ---------------------------------------------------------------------------
# Native-C platform shim — `packages/core/nros-platform-esp-idf/`.
# The IDF project's component-discovery pass (driven by the caller's
# `EXTRA_COMPONENT_DIRS`) is what registers this directory as an IDF
# component, so we do NOT `add_subdirectory` it here — that would
# create a duplicate `__idf_nano-ros` target collision and force IDF
# to process the component in two different contexts. Callers
# (`scripts/arduino/idf-builder/CMakeLists.txt`, `tests/esp-idf-smoke/`,
# user projects) are responsible for adding
# `packages/core/nros-platform-esp-idf` to their EXTRA_COMPONENT_DIRS.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# NanoRos::Platform alias. The IDF component manager exposes
# `nros_platform_esp_idf` as a component library target; we wrap it
# in the INTERFACE umbrella so consumers link via
# `NanoRos::Platform` exactly like every other platform.
# ---------------------------------------------------------------------------
add_library(nros_platform_esp_idf_iface INTERFACE)
if(TARGET nros_platform_esp_idf)
    target_link_libraries(nros_platform_esp_idf_iface INTERFACE
        nros_platform_esp_idf)
endif()
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_esp_idf_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# ESP-IDF apps don't need a per-app startup file or linker script —
# the IDF build system supplies those. Just link the platform
# umbrella and let IDF handle the rest. The function exists for
# parity with the other platform modules so user code can call it
# unconditionally regardless of NANO_ROS_PLATFORM.
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()
    if(TARGET nros_platform_esp_idf)
        target_link_libraries(${target} PRIVATE nros_platform_esp_idf)
    endif()
endfunction()
