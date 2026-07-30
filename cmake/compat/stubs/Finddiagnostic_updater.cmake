# Find-stub for diagnostic_updater — Phase 209.B + 209.D.
#
# Pulls in the nano-ros header-only `diagnostic_updater` shim at
# `cmake/compat/diagnostic-updater/` (moved there in phase-321 W2.c: it is a
# C++-only rclcpp compat shim with ZERO Rust, so it belongs with the other
# compat stubs, not among the Rust core crates). The cmake target alias
# `diagnostic_updater::diagnostic_updater` is created by that package's
# CMakeLists, matching the upstream `target_link_libraries(... diagnostic_updater::
# diagnostic_updater)` shape ported ROS 2 nodes use.

if(NOT TARGET diagnostic_updater::diagnostic_updater)
    # Repo-relative path: this stub lives at <repo>/cmake/compat/stubs/.
    get_filename_component(_nros_repo_root
        "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
    set(_du_dir "${_nros_repo_root}/cmake/compat/diagnostic-updater")
    if(EXISTS "${_du_dir}/CMakeLists.txt")
        add_subdirectory("${_du_dir}" nros-diagnostic-updater EXCLUDE_FROM_ALL)
    endif()
endif()

if(TARGET diagnostic_updater::diagnostic_updater)
    set(diagnostic_updater_FOUND TRUE)
else()
    set(diagnostic_updater_FOUND FALSE)
endif()
