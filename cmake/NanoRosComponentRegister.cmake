# cmake/NanoRosComponentRegister.cmake — Phase 212.L.9
#
# C++ cmake fn surface for the three Phase 212.L pkg shapes:
#
#   * `nano_ros_component_register(NAME <name> CLASS <UserClass>
#       SOURCES <files...> DEPLOY <target1> [<target2> ...])`
#       — declares a Component pkg entity. Compiles SOURCES into a
#         STATIC `<pkg>_<name>_component` lib linked to
#         `NanoRos::NanoRosCpp`. Enforces L.4: CLASS must start with
#         `${PROJECT_NAME}::`.
#
#   * `nano_ros_application(NAME <name> SOURCES <files...>
#       DEPLOY <target1> [<target2> ...])`
#       — declares an Application pkg entity. Calls `add_executable`
#         + links `NanoRos::NanoRosCpp`. L.2 rule: only "native"
#         allowed in DEPLOY.
#
#   * `nano_ros_deploy(TARGET <name> RMW <rmw> DOMAIN_ID <n>
#       [LOCATOR <uri>])`
#       — records per-target rmw / domain_id / locator config.
#
# Side effect: every fn appends to GLOBAL props and rewrites
# `${CMAKE_BINARY_DIR}/nros-metadata.json` so `nros codegen-system`
# can consume it at configure time. No sidecar TOML for C++ pkgs.
#
# Hard cap: ≤200 LoC (tokei gate per Phase 212.L.9 acceptance).

if(DEFINED _NROS_COMPONENT_REGISTER_INCLUDED)
    return()
endif()
set(_NROS_COMPONENT_REGISTER_INCLUDED TRUE)

define_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON
    BRIEF_DOCS "Accumulated component JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_component_register().")
define_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON
    BRIEF_DOCS "Accumulated application JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_application().")
define_property(GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON
    BRIEF_DOCS "Accumulated deploy_targets JSON fragments"
    FULL_DOCS  "Phase 212.L.9 — appended by nano_ros_deploy().")
set_property(GLOBAL PROPERTY NROS_COMPONENTS_JSON "")
set_property(GLOBAL PROPERTY NROS_APPLICATIONS_JSON "")
set_property(GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON "")

# Emit the JSON file. Idempotent — called after every fn so the file
# is always current. Keep small: writes the whole doc each time.
function(_nros_metadata_emit)
    get_property(_comps   GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    get_property(_apps    GLOBAL PROPERTY NROS_APPLICATIONS_JSON)
    get_property(_targets GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON)
    set(_doc "{\n")
    string(APPEND _doc "  \"components\": [${_comps}\n  ],\n")
    string(APPEND _doc "  \"applications\": [${_apps}\n  ],\n")
    string(APPEND _doc "  \"deploy_targets\": {${_targets}\n  }\n")
    string(APPEND _doc "}\n")
    file(WRITE "${CMAKE_BINARY_DIR}/nros-metadata.json" "${_doc}")
endfunction()

# Helper: render a string list as a JSON array body.
function(_nros_json_strlist out_var)
    set(_acc "")
    set(_first TRUE)
    foreach(_v IN LISTS ARGN)
        if(_first)
            set(_acc "\"${_v}\"")
            set(_first FALSE)
        else()
            set(_acc "${_acc}, \"${_v}\"")
        endif()
    endforeach()
    set(${out_var} "${_acc}" PARENT_SCOPE)
endfunction()

function(nano_ros_component_register)
    cmake_parse_arguments(_NRC "" "NAME;CLASS" "SOURCES;DEPLOY" ${ARGN})
    foreach(_req NAME CLASS SOURCES DEPLOY)
        if(NOT _NRC_${_req})
            message(FATAL_ERROR
                "nano_ros_component_register: ${_req} required")
        endif()
    endforeach()
    # L.4 enforcement: CLASS must start with `${PROJECT_NAME}::`.
    string(FIND "${_NRC_CLASS}" "${PROJECT_NAME}::" _idx)
    if(NOT _idx EQUAL 0)
        message(FATAL_ERROR
            "nano_ros_component_register: CLASS '${_NRC_CLASS}' must "
            "start with '${PROJECT_NAME}::' (Phase 212.L.4 rule — the "
            "pkg directory name IS the pkg name).")
    endif()

    set(_lib "${PROJECT_NAME}_${_NRC_NAME}_component")
    if(NOT TARGET ${_lib})
        add_library(${_lib} STATIC ${_NRC_SOURCES})
        if(TARGET NanoRos::NanoRosCpp)
            target_link_libraries(${_lib} PUBLIC NanoRos::NanoRosCpp)
        endif()
        target_include_directories(${_lib} PUBLIC
            "${CMAKE_CURRENT_SOURCE_DIR}/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/src")
    endif()

    _nros_json_strlist(_sources_json ${_NRC_SOURCES})
    _nros_json_strlist(_deploy_json  ${_NRC_DEPLOY})
    get_property(_acc GLOBAL PROPERTY NROS_COMPONENTS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    {\"name\": \"${_NRC_NAME}\", \"class\": \"${_NRC_CLASS}\", \
\"sources\": [${_sources_json}], \"deploy\": [${_deploy_json}], \
\"pkg_dir\": \"${CMAKE_CURRENT_SOURCE_DIR}\", \"lang\": \"cpp\"}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_COMPONENTS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()

function(nano_ros_application)
    cmake_parse_arguments(_NRA "" "NAME" "SOURCES;DEPLOY" ${ARGN})
    foreach(_req NAME SOURCES DEPLOY)
        if(NOT _NRA_${_req})
            message(FATAL_ERROR
                "nano_ros_application: ${_req} required")
        endif()
    endforeach()
    # L.2: Application pkgs are NATIVE-ONLY.
    foreach(_t IN LISTS _NRA_DEPLOY)
        if(NOT _t STREQUAL "native")
            message(FATAL_ERROR
                "nano_ros_application: DEPLOY target '${_t}' rejected — "
                "Application pkgs are native-only (Phase 212.L.2).")
        endif()
    endforeach()

    if(NOT TARGET ${_NRA_NAME})
        add_executable(${_NRA_NAME} ${_NRA_SOURCES})
        if(TARGET NanoRos::NanoRosCpp)
            target_link_libraries(${_NRA_NAME} PRIVATE NanoRos::NanoRosCpp)
        endif()
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

function(nano_ros_deploy)
    cmake_parse_arguments(_NRD "" "TARGET;RMW;DOMAIN_ID;LOCATOR" "" ${ARGN})
    foreach(_req TARGET RMW DOMAIN_ID)
        if(NOT DEFINED _NRD_${_req})
            message(FATAL_ERROR
                "nano_ros_deploy: ${_req} required")
        endif()
    endforeach()
    if(DEFINED _NRD_LOCATOR AND NOT _NRD_LOCATOR STREQUAL "")
        set(_loc_json "\"${_NRD_LOCATOR}\"")
    else()
        set(_loc_json "null")
    endif()

    get_property(_acc GLOBAL PROPERTY NROS_DEPLOY_TARGETS_JSON)
    if(_acc)
        set(_sep ",")
    else()
        set(_sep "")
    endif()
    set(_entry
"${_sep}\n    \"${_NRD_TARGET}\": {\"rmw\": \"${_NRD_RMW}\", \
\"domain_id\": ${_NRD_DOMAIN_ID}, \"locator\": ${_loc_json}}")
    set_property(GLOBAL APPEND_STRING PROPERTY NROS_DEPLOY_TARGETS_JSON "${_entry}")
    _nros_metadata_emit()
endfunction()
