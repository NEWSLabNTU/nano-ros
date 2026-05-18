# Phase 139.2 — ESP-IDF integration shell.
#
# Registers nano-ros as an ESP-IDF component and dispatches the build
# into the Phase 137 root CMake. The component itself ships no .c
# sources — it's a thin re-export of NanoRos::NanoRos so user
# components can list "nano-ros" in their REQUIRES.
#
# ≤50 lines (excl. comments) per Phase 139's per-shell budget.

# Resolve symlinks before walking up. The Arduino library builder
# (Phase 23.2) stages this directory under a renamed symlink so the
# IDF component name resolves to `nano-ros`; resolving the real
# path of CMAKE_CURRENT_LIST_DIR first ensures the `../..` walk
# lands at the repo root rather than inside the staging dir.
file(REAL_PATH "${CMAKE_CURRENT_LIST_DIR}" _nros_int_real)
file(REAL_PATH "${_nros_int_real}/../.." _nros_root)

# Map Kconfig.projbuild knobs to NANO_ROS_* cache vars. Defaults
# mirror the Phase 137 root defaults; user `sdkconfig` overrides win.
if(NOT DEFINED CONFIG_NROS_RMW OR CONFIG_NROS_RMW STREQUAL "")
    set(_nros_rmw "zenoh")
else()
    set(_nros_rmw "${CONFIG_NROS_RMW}")
endif()
if(NOT DEFINED CONFIG_NROS_ROS_EDITION OR CONFIG_NROS_ROS_EDITION STREQUAL "")
    set(_nros_ros_edition "humble")
else()
    set(_nros_ros_edition "${CONFIG_NROS_ROS_EDITION}")
endif()

# Phase 21.6 / 21.9 — switch from `baremetal` (which the nros-c
# subproject rejects with `FATAL_ERROR`) to the dedicated `esp-idf`
# platform module that ships with this checkout. The `esp-idf`
# module pulls in `packages/core/nros-platform-esp-idf/` as a child
# IDF component and exposes `NanoRos::Platform`.
set(NANO_ROS_PLATFORM    "esp-idf"             CACHE STRING "" FORCE)
set(NANO_ROS_RMW         "${_nros_rmw}"        CACHE STRING "" FORCE)
set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

# Register first so $COMPONENT_LIB exists for the link below. The
# REQUIRES list pulls every IDF component the nros-platform-esp-idf
# child component (and therefore nros-c) needs through the IDF
# dependency walker — without this, the child component's
# `idf_component_register(REQUIRES freertos esp_timer …)` would
# fail because those components don't transitively reach this top-
# level shell otherwise.
idf_component_register(
    SRCS ""
    INCLUDE_DIRS "${_nros_root}/packages/core/nros-c/include"
    REQUIRES log freertos esp_timer esp_hw_support esp_system lwip)

# Phase 21.10.A — gather every IDF component's
# INTERFACE_INCLUDE_DIRECTORIES that nano-ros's cross-compiled Rust
# code needs to see (FreeRTOS, lwIP, esp_*) and inject them into
# `ENV{CFLAGS_<rust-target>}` BEFORE the root nano-ros CMake adds
# the Corrosion-imported staticlibs. Without this, `zpico-sys/
# build.rs`'s cc::Build invocations can't find `FreeRTOS.h` /
# `lwip/sockets.h`. The env captured here flows through Corrosion's
# `cmake -E env` wrapper to the downstream cargo invocations.
function(_nros_esp_idf_publish_cflags)
    if(NOT DEFINED Rust_CARGO_TARGET)
        return()
    endif()
    set(_dirs "")
    foreach(_comp freertos lwip esp_hw_support esp_system esp_common
                  hal soc xtensa riscv esp_rom esp_timer log esp_pm
                  newlib esp_event esp_netif esp_phy esp_app_format
                  esp_partition esp_security spi_flash heap)
        if(TARGET __idf_${_comp})
            get_property(_d TARGET __idf_${_comp}
                PROPERTY INTERFACE_INCLUDE_DIRECTORIES)
            foreach(_p IN LISTS _d)
                if(NOT "${_p}" MATCHES "\\$<" AND IS_DIRECTORY "${_p}")
                    list(APPEND _dirs "${_p}")
                endif()
            endforeach()
        endif()
    endforeach()
    if(_dirs)
        list(REMOVE_DUPLICATES _dirs)
    endif()
    # Fallback: zenoh-pico uses unqualified `#include "FreeRTOS.h"`
    # but ESP-IDF's freertos component only exports the parent dir
    # (`.../FreeRTOS-Kernel/include` — relies on consumers writing
    # `#include "freertos/FreeRTOS.h"`). Add the inner `freertos/`
    # dirs so bare includes resolve.
    foreach(_p IN LISTS _dirs)
        foreach(_inner freertos)
            if(IS_DIRECTORY "${_p}/${_inner}")
                list(APPEND _dirs "${_p}/${_inner}")
            endif()
        endforeach()
    endforeach()
    list(REMOVE_DUPLICATES _dirs)
    set(_flags "")
    foreach(_p IN LISTS _dirs)
        string(APPEND _flags " -I${_p}")
    endforeach()
    # Write the resolved CFLAGS to a sentinel file in the build
    # dir so the two-pass driver
    # (`scripts/arduino/build-libnanoros.sh`) can read it after the
    # first `idf.py reconfigure` and re-export it as
    # `CFLAGS_<rust-target>` before the second `idf.py build`.
    # Corrosion 0.5's `corrosion_set_env_vars` reads its property
    # off a bare-name target Corrosion never creates, so per-crate
    # cmake plumbing does not work — the build script is the only
    # reliable injection point.
    string(REPLACE "-" "_" _tenv "${Rust_CARGO_TARGET}")
    string(TOUPPER "${_tenv}" _tenv)
    # Quote each value so the build driver can `source` this file
    # under `set -a` without bash splitting `-I foo -I bar` into
    # separate commands.
    file(WRITE "${CMAKE_BINARY_DIR}/nros_esp_idf_rust_cflags.env"
        "CFLAGS_${_tenv}=\"${_flags}\"\nTARGET_CFLAGS=\"${_flags}\"\n")
    message(STATUS
        "nano-ros esp-idf: wrote Rust CFLAGS to "
        "${CMAKE_BINARY_DIR}/nros_esp_idf_rust_cflags.env "
        "(${_dirs})")
endfunction()
_nros_esp_idf_publish_cflags()

# Pull in the unified root CMake. ESP-IDF runs each component
# CMakeLists in an isolated CMake project, so add_subdirectory works
# the same as in a regular host build.
add_subdirectory("${_nros_root}" nano_ros_root)

# Re-export the umbrella interface target. INTERFACE linkage so user
# components depending on `nano-ros` pick up the transitive deps.
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
