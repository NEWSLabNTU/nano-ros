# Phase 139.2 + Phase 212.H.5 — ESP-IDF integration shell.
#
# Registers nano-ros as an ESP-IDF component and dispatches the build
# into the Phase 137 root CMake. The component ships no .c sources of
# its own — it's a thin re-export of NanoRos::NanoRos so user
# components can list "nano-ros" in their REQUIRES.
#
# Phase 212.H.5 adds a configure-time `nros codegen-system` step:
# when the host CLI is on PATH and a bringup pkg is identified via
# Kconfig (`CONFIG_NROS_BRINGUP_NAME`) or cache (`NROS_BRINGUP_DIR`),
# we emit the baked tree to ${CMAKE_BINARY_DIR}/nros-system/ and feed
# its include path back into the consuming user component.
#
# ≤200 LoC HARD cap per Phase 139's per-shell budget + Phase 212.H.8.

set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../..")

# --- Kconfig.projbuild → CMake cache mapping ----------------------------
# Defaults mirror the Phase 137 root defaults; user `sdkconfig` wins.
if(NOT DEFINED CONFIG_NROS_RMW OR CONFIG_NROS_RMW STREQUAL "")
    set(_nros_rmw "xrce")
else()
    set(_nros_rmw "${CONFIG_NROS_RMW}")
endif()
# issue 0947 — one resolver, and it VALIDATES. This used to default to the
# literal `humble`, so a typo in `sdkconfig` passed straight through as a
# cargo feature that does not exist, rather than failing here with the
# value that caused it.
include("${_nros_root}/cmake/NanoRosRosEdition.cmake")
_nros_resolve_ros_edition("${CONFIG_NROS_ROS_EDITION}" _nros_ros_edition)
if(NOT DEFINED CONFIG_NROS_BRINGUP_NAME OR CONFIG_NROS_BRINGUP_NAME STREQUAL "")
    set(_nros_bringup_name "demo_bringup")
else()
    set(_nros_bringup_name "${CONFIG_NROS_BRINGUP_NAME}")
endif()
# phase-405 W3 — no `_nros_domain_id` local here. One used to be computed and
# then read by nothing (the NANO_ROS_* block below forwards rmw and edition
# only). It was never needed: ESP-IDF puts CONFIG_NROS_DOMAIN_ID in its own
# generated sdkconfig.h, which is where nros/main.hpp reads it from.

set(NANO_ROS_PLATFORM    "esp_idf"              CACHE STRING "" FORCE)
set(NANO_ROS_RMW         "${_nros_rmw}"         CACHE STRING "" FORCE)
set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

# Rust cargo target derived from IDF_TARGET. Corrosion infers x86_64 when
# the toolchain is set but the target-triple isn't — bake it explicitly.
# Xtensa SoCs (esp32, esp32s2, esp32s3) need an espup-built rust; RISC-V
# SoCs (esp32c3 / c6 / h2 / p4) use the stock riscv32imc/imac target.
if(NOT DEFINED Rust_CARGO_TARGET OR Rust_CARGO_TARGET STREQUAL "")
    if(IDF_TARGET STREQUAL "esp32c3" OR IDF_TARGET STREQUAL "esp32c2")
        set(Rust_CARGO_TARGET "riscv32imc-unknown-none-elf" CACHE STRING "")
    elseif(IDF_TARGET STREQUAL "esp32c6" OR IDF_TARGET STREQUAL "esp32h2"
           OR IDF_TARGET STREQUAL "esp32p4")
        set(Rust_CARGO_TARGET "riscv32imac-unknown-none-elf" CACHE STRING "")
    elseif(IDF_TARGET MATCHES "^esp32(|s2|s3)$")
        set(Rust_CARGO_TARGET "xtensa-${IDF_TARGET}-none-elf" CACHE STRING "")
    endif()
endif()

# Phase 212.M.7 — the `--cfg=portable_atomic_unsafe_assume_single_core`
# flag required by riscv32imc (esp32c3 / esp32c2) lives in workspace
# `.cargo/config.toml` under `[target.riscv32imc-unknown-none-elf]` so it
# reaches every transitive crate (`portable-atomic` itself + every
# consumer). See that file for the rationale + safety constraint.

# --- Phase 212.H.5 codegen-system bake ----------------------------------
# Where the user workspace + bringup pkg live. `NROS_WORKSPACE_DIR` is
# typically the parent of the IDF project (`${CMAKE_SOURCE_DIR}/..`);
# users may pin it via -D on `idf.py build` or in the IDF project's
# top-level CMakeLists. The fallback walks up two from the IDF project
# (esp_idf_app/ → workspace root).
if(NOT DEFINED NROS_WORKSPACE_DIR OR NROS_WORKSPACE_DIR STREQUAL "")
    get_filename_component(_nros_ws "${CMAKE_SOURCE_DIR}/.." ABSOLUTE)
    set(NROS_WORKSPACE_DIR "${_nros_ws}" CACHE PATH "nano-ros workspace root")
endif()
set(_nros_bringup_dir "${NROS_WORKSPACE_DIR}/${_nros_bringup_name}")
set(_nros_system_out  "${CMAKE_BINARY_DIR}/nros-system")

# Post-Phase-218 the `nros` CLI is built in-tree by `just setup-cli`;
# `source ./activate.sh` puts it on PATH.
#
# issue 0325 — this used `find_program(... HINTS "$ENV{HOME}/.nros/bin")`, and
# HINTS are searched BEFORE PATH, so a stale provisioned CLI shadowed the
# in-tree one that activate.sh exports (the precedence trap CLAUDE.md names
# verbatim: "use PATHS for fallbacks"). Route through the shared resolver from
# issue 0219 instead of keeping a fifth bespoke one: it puts the store in
# PATHS, honours $NROS_CLI, and drops a cached path that no longer exists.
#
# OPTIONAL because this shim must still configure without a CLI — the
# `if(NROS_EXECUTABLE ...)` guard below already fails soft.
include("${_nros_root}/cmake/NanoRosCodegenCore.cmake")
nros_resolve_cli(NROS_EXECUTABLE OPTIONAL CONTEXT "integrations/nano-ros")

if(NROS_EXECUTABLE AND EXISTS "${_nros_bringup_dir}/system.toml")
    file(MAKE_DIRECTORY "${_nros_system_out}")
    # issue 1018 — same rule as the Zephyr bake: the emitted `system_config.h`
    # is compiled into the image, so a rebuilt `nros` must re-run this
    # configure. Doubly so here, because the failure below is SOFT: without the
    # reconfigure a stale bake is not merely kept, it is kept silently.
    nros_codegen_tool_reconfigure("${NROS_EXECUTABLE}")
    # Configure-time bake. If the running CLI lacks `codegen-system`
    # (Phase 212.E not yet shipped in this nros-cli), this fails soft
    # so the shim still configures.
    execute_process(
        COMMAND "${NROS_EXECUTABLE}" codegen-system
                --workspace "${NROS_WORKSPACE_DIR}"
                --bringup   "${_nros_bringup_name}"
                --target    "esp-idf"
                --out       "${_nros_system_out}"
        RESULT_VARIABLE _nros_cg_rc
        OUTPUT_VARIABLE _nros_cg_out
        ERROR_VARIABLE  _nros_cg_err)
    if(NOT _nros_cg_rc EQUAL 0)
        message(STATUS
            "nano-ros: codegen-system skipped (rc=${_nros_cg_rc}); "
            "Phase 212.E not shipped in this nros-cli yet. "
            "Continuing with pre-baked headers if present.")
    else()
        message(STATUS "nano-ros: codegen-system baked into ${_nros_system_out}")
    endif()
elseif(NOT NROS_EXECUTABLE)
    message(STATUS
        "nano-ros: nros CLI not on PATH; skipping codegen-system. "
        "Install via `just setup-cli` + `source ./activate.sh` (Phase 218).")
endif()

# --- Component registration ---------------------------------------------
# If codegen-system emitted system_main.c into nros-system/, pull it in
# as a real component source; otherwise stay header-only (Phase 139.2).
set(_nros_sys_main "${_nros_system_out}/system_main.c")
if(EXISTS "${_nros_sys_main}")
    set(_nros_srcs "${_nros_sys_main}")
else()
    set(_nros_srcs "")
endif()

idf_component_register(
    SRCS         ${_nros_srcs}
    INCLUDE_DIRS "${_nros_root}/packages/api/nros-c/include"
                 "${_nros_system_out}"
    REQUIRES     log freertos)

# Pull in the unified root CMake. ESP-IDF runs each component
# CMakeLists in an isolated CMake project, so add_subdirectory works
# the same as in a regular host build.
add_subdirectory("${_nros_root}" nano_ros_root)

# issue 0719 — the shared applier, not a third hand-copy of it.
#
# This shim does not go through `nano_ros_entry()` and cannot: ESP-IDF owns the
# image, and the entry is entry-package shaped. What it shares with the entry is
# the cross-cutting half, which now lives in `nros_apply_panic_policy`. The copy
# that used to be here (added by #0700, after `nros-c` failed to compile with
# `#[panic_handler] required, but not found`) lacked the conflict detection and
# the externally-applied-lane check that the entry has always had.
nros_apply_panic_policy(platform "integrations/nano-ros (esp-idf component shim)")

# Re-export the umbrella interface target. INTERFACE linkage so user
# components depending on `nano-ros` pick up the transitive deps.
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
