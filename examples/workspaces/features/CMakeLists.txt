cmake_minimum_required(VERSION 3.22)

project(features_workspace LANGUAGES C CXX)

# 287-W6 / phase-331 W2 — the custom-message members resolve the
# workspace-local interface pkg via `find_package(custom_msgs REQUIRED)`; the
# compat layer auto-emits its Find-stub for packages under this search path
# (Phase 210.A.2). MUST precede `find_package(nano_ros)`. `src/custom_msgs` is
# deliberately NOT a `_ws_subdirs` entry — it declares the schema only; the
# components carry the type name as a string and hand-encode CDR (RFC-0043).
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")

find_package(nano_ros REQUIRED COMPONENTS workspace)

# NATIVE ONLY (RFC-0066 R2). `[param_services]` / `[lifecycle]` are alloc-gated
# and an embedded image must opt in explicitly, so this workspace declares no
# board variants at all — that is the whole reason the capability demos live
# here instead of in the language workspaces.
set(NANO_ROS_PLATFORM posix)

set(_ws_subdirs
    # --- C ---------------------------------------------------------------
    src/c_qos_talker_pkg
    src/c_qos_listener_pkg
    src/c_param_talker_pkg
    src/c_lifecycle_talker_pkg
    src/c_reading_talker_pkg
    src/c_reading_listener_pkg
    # --- C++ -------------------------------------------------------------
    src/cpp_qos_talker_pkg
    src/cpp_qos_listener_pkg
    src/cpp_param_talker_pkg
    src/cpp_lifecycle_talker_pkg
    src/cpp_reading_talker_pkg
    src/cpp_reading_listener_pkg
    # --- entries (node pkgs BEFORE entries: the entry codegen reads their
    #     `nano_ros_node_register` metadata) ---------------------------------
    src/native_c_qos_talker_entry
    src/native_c_qos_listener_entry
    src/native_c_params_entry
    src/native_c_lifecycle_entry
    src/native_c_custom_msg_talker_entry
    src/native_c_custom_msg_listener_entry
    src/native_cpp_qos_talker_entry
    src/native_cpp_qos_listener_entry
    src/native_cpp_params_entry
    src/native_cpp_lifecycle_entry
    src/native_cpp_custom_msg_talker_entry
    src/native_cpp_custom_msg_listener_entry
    # --- mixed (C node pkgs driven by C++ TYPED entry carriers) -----------
    src/mixed_qos_talker_pkg
    src/mixed_qos_listener_pkg
    src/mixed_reading_talker_pkg
    src/mixed_reading_listener_pkg
    src/native_mixed_qos_talker_entry
    src/native_mixed_qos_listener_entry
    src/native_mixed_custom_msg_talker_entry
    src/native_mixed_custom_msg_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
