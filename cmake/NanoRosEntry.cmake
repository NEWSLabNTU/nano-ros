# cmake/NanoRosEntry.cmake — Phase 212.N.6
#
# Defines the C++ cmake fn `nano_ros_entry(NAME <name> SOURCES <files...>
#   [BOARD <board>] DEPLOY <target1> [<target2> ...])`.
#
# This is the rename of the pre-N.6 `nano_ros_application` per Phase
# 212.L.9 + 212.N — see `cmake/NanoRosComponentRegister.cmake` for the
# legacy alias (DEPRECATION shim that forwards here). Both names
# resolve so the in-tree caller migration (212.N.7 wave) can land
# incrementally without breaking the configure step.
#
# Semantics (= pre-N.6 `nano_ros_application` body):
#   * `NAME` (required) — exe target + Entry pkg entity name.
#   * `SOURCES` (required, multi-value) — sources passed to
#     `add_executable`.
#   * `DEPLOY` (required, multi-value) — L.2 rule: only `native`
#     allowed; embedded targets reject configure with FATAL_ERROR.
#   * `BOARD` (optional, single-value) — Phase 212.N.6 addition: name
#     of the `Board` impl (see §212.N.1) the Entry pkg targets.
#     Stored as the `NANO_ROS_BOARD` target property so later N.4 /
#     N.5 work can read it at configure time + drive the codegen
#     planner. Absent BOARD is currently valid (host-native pkgs +
#     pre-Board callers); a future phase may make it required for
#     embedded DEPLOY targets once the Board family lands.
#
# Side effect: appends an entry to the GLOBAL `NROS_APPLICATIONS_JSON`
# property and rewrites `${CMAKE_BINARY_DIR}/nros-metadata.json` via
# `_nros_metadata_emit()` (defined in NanoRosComponentRegister.cmake;
# we depend on it being included alongside this module).

if(DEFINED _NROS_ENTRY_INCLUDED)
    return()
endif()
set(_NROS_ENTRY_INCLUDED TRUE)

# Pull in the shared metadata-emit helper + GLOBAL property
# definitions. `NanoRosComponentRegister.cmake` is the SSoT for those
# (it predates this module). The include is guarded inside that file,
# so re-including is a no-op when callers already loaded it.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosComponentRegister.cmake")

function(nano_ros_entry)
    cmake_parse_arguments(_NRA "" "NAME;BOARD" "SOURCES;DEPLOY" ${ARGN})
    foreach(_req NAME SOURCES DEPLOY)
        if(NOT _NRA_${_req})
            message(FATAL_ERROR
                "nano_ros_entry: ${_req} required")
        endif()
    endforeach()
    # L.2: Entry pkgs are NATIVE-ONLY at the cmake surface.
    foreach(_t IN LISTS _NRA_DEPLOY)
        if(NOT _t STREQUAL "native")
            message(FATAL_ERROR
                "nano_ros_entry: DEPLOY target '${_t}' rejected — "
                "Entry pkgs are native-only (Phase 212.L.2).")
        endif()
    endforeach()

    if(NOT TARGET ${_NRA_NAME})
        add_executable(${_NRA_NAME} ${_NRA_SOURCES})
        if(TARGET NanoRos::NanoRosCpp)
            target_link_libraries(${_NRA_NAME} PRIVATE NanoRos::NanoRosCpp)
        endif()
    endif()

    # Phase 212.N.6 — stash the BOARD selection on the target so the
    # later N.4 / N.5 codegen planner can read it. Empty when caller
    # didn't pass BOARD.
    if(DEFINED _NRA_BOARD)
        set_target_properties(${_NRA_NAME} PROPERTIES
            NANO_ROS_BOARD "${_NRA_BOARD}")
    endif()

    _nros_json_strlist(_sources_json ${_NRA_SOURCES})
    _nros_json_strlist(_deploy_json  ${_NRA_DEPLOY})
    get_property(_acc GLOBAL PROPERTY NROS_APPLICATIONS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    {\"name\": \"${_NRA_NAME}\", \"sources\": [${_sources_json}], \
\"deploy\": [${_deploy_json}], \"pkg_dir\": \"${CMAKE_CURRENT_SOURCE_DIR}\", \
\"lang\": \"cpp\"}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_APPLICATIONS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()
