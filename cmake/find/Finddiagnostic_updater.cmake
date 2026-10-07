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

if(TARGET diagnostic_updater::diagnostic_updater)
    set(diagnostic_updater_FOUND TRUE)
else()
    set(diagnostic_updater_FOUND FALSE)
endif()
