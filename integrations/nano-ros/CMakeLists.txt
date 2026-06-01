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
if(NOT DEFINED CONFIG_NROS_ROS_EDITION OR CONFIG_NROS_ROS_EDITION STREQUAL "")
    set(_nros_ros_edition "humble")
else()
    set(_nros_ros_edition "${CONFIG_NROS_ROS_EDITION}")
endif()
if(NOT DEFINED CONFIG_NROS_BRINGUP_NAME OR CONFIG_NROS_BRINGUP_NAME STREQUAL "")
    set(_nros_bringup_name "demo_bringup")
else()
    set(_nros_bringup_name "${CONFIG_NROS_BRINGUP_NAME}")
endif()
if(NOT DEFINED CONFIG_NROS_DOMAIN_ID OR CONFIG_NROS_DOMAIN_ID STREQUAL "")
    set(_nros_domain_id "0")
else()
    set(_nros_domain_id "${CONFIG_NROS_DOMAIN_ID}")
endif()

set(NANO_ROS_PLATFORM    "baremetal"            CACHE STRING "" FORCE)
set(NANO_ROS_RMW         "${_nros_rmw}"         CACHE STRING "" FORCE)
set(NANO_ROS_ROS_EDITION "${_nros_ros_edition}" CACHE STRING "" FORCE)

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

find_program(NROS_EXECUTABLE nros HINTS "$ENV{HOME}/.nros/bin")

if(NROS_EXECUTABLE AND EXISTS "${_nros_bringup_dir}/system.toml")
    file(MAKE_DIRECTORY "${_nros_system_out}")
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
        "Install via scripts/install-nros.sh.")
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
    INCLUDE_DIRS "${_nros_root}/packages/core/nros-c/include"
                 "${_nros_system_out}"
    REQUIRES     log freertos)

# Pull in the unified root CMake. ESP-IDF runs each component
# CMakeLists in an isolated CMake project, so add_subdirectory works
# the same as in a regular host build.
add_subdirectory("${_nros_root}" nano_ros_root)

# Re-export the umbrella interface target. INTERFACE linkage so user
# components depending on `nano-ros` pick up the transitive deps.
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
