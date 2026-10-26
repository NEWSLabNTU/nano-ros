# Find module for diagnostic_updater — part of nano-ros's CMake package.
#
# Pulls in nano-ros's header-only `diagnostic_updater` at
# `packages/api/nros-diagnostic-updater/` (phase-482 W1 moved it out of the
# deleted `cmake/compat/`). That package's CMakeLists creates the
# `diagnostic_updater::diagnostic_updater` alias, matching the upstream
# `target_link_libraries(... diagnostic_updater::diagnostic_updater)` shape.

if(NOT TARGET diagnostic_updater::diagnostic_updater)
    # This module lives at <repo>/cmake/find/.
    get_filename_component(_nros_repo_root
        "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
    set(_du_dir "${_nros_repo_root}/packages/api/nros-diagnostic-updater")
    if(NOT EXISTS "${_du_dir}/CMakeLists.txt")
        message(FATAL_ERROR
            "Finddiagnostic_updater: ${_du_dir}/CMakeLists.txt is missing — "
            "this module and the package it loads moved together (phase-482 W1).")
    endif()
    add_subdirectory("${_du_dir}" nros-diagnostic-updater EXCLUDE_FROM_ALL)
endif()

# phase-488 — `<diagnostic_updater/diagnostic_updater.hpp>` includes
# `diagnostic_msgs`, so the target carries that dependency, as upstream's
# `diagnostic_updaterConfig.cmake` does. Without it every consumer had to name
# `diagnostic_msgs__nano_ros_cpp` itself, and a stock package that names only
# `diagnostic_updater` failed on the header. The message target is a normal
# (global) target, so linking it by name here reaches every consumer.
if(TARGET nros-diagnostic-updater)
    get_target_property(_du_msgs nros-diagnostic-updater NROS_LINKS_DIAGNOSTIC_MSGS)
    if(NOT _du_msgs)
        if(NOT TARGET diagnostic_msgs__nano_ros_cpp)
            find_package(diagnostic_msgs MODULE REQUIRED)
        endif()
        # phase-518 W4 — under validate-only (the RTOS roads) the line above
        # only validated, and the headers below include the messages, so they
        # are generated here: every consumer of this library is C++.
        if(NOT TARGET diagnostic_msgs__nano_ros_cpp AND COMMAND _nros_require_cpp_msg_package)
            _nros_require_cpp_msg_package(diagnostic_msgs)
        endif()
        target_link_libraries(nros-diagnostic-updater INTERFACE diagnostic_msgs__nano_ros_cpp)
        set_target_properties(nros-diagnostic-updater PROPERTIES NROS_LINKS_DIAGNOSTIC_MSGS TRUE)
    endif()
endif()

if(TARGET diagnostic_updater::diagnostic_updater)
    set(diagnostic_updater_FOUND TRUE)
else()
    set(diagnostic_updater_FOUND FALSE)
endif()
