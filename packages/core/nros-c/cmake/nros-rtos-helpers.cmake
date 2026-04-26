# nros-rtos-helpers.cmake
#
# Cross-RTOS cmake primitives. Pure mechanics: this module knows nothing
# about any specific RTOS / network stack / libc / linker. It exists to
# eliminate the boilerplate that every per-RTOS module
# (nros-threadx.cmake, nros-freertos.cmake, …) would otherwise have to
# repeat.
#
# Public functions:
#
#   nros_validate_vars(VAR1 VAR2 …)
#       For each argument, ensure the cmake variable is set or read it
#       from the environment with the same name. FATAL_ERROR if neither
#       is present.
#
#   nros_build_rtos_static_lib(<name>
#                              SOURCES <files…>
#                              [INCLUDES <dirs…>]
#                              [DEFINES <defs…>]
#                              [WARN_FLAGS <flags…>]
#                              [C_STANDARD <std>])
#       add_library(<name> STATIC) with the conventions used by every
#       RTOS support module: PRIVATE include dirs, PRIVATE defines,
#       PRIVATE compile options (default: -Wno-unused-parameter
#       -Wno-sign-compare), and C_STANDARD (default: 11).
#
#   nros_compose_platform_target(<name>
#                                COMPONENTS <static_libs…>
#                                [INCLUDES <dirs…>]
#                                [DEFINES <defs…>]
#                                [LINK_LIBS <libs…>])
#       add_library(<name> INTERFACE) linking the listed static
#       components plus optional system libraries. INTERFACE include
#       directories propagate to consumers (so examples don't need to
#       repeat the kernel include paths).

if(DEFINED _NROS_RTOS_HELPERS_INCLUDED)
    return()
endif()
set(_NROS_RTOS_HELPERS_INCLUDED TRUE)

# ----------------------------------------------------------------------
# nros_validate_vars
# ----------------------------------------------------------------------
function(nros_validate_vars)
    foreach(_var ${ARGN})
        if(NOT DEFINED ${_var})
            if(DEFINED ENV{${_var}})
                set(${_var} "$ENV{${_var}}" PARENT_SCOPE)
            else()
                message(FATAL_ERROR
                    "${_var} not set. Pass -D${_var}=<path> or export ${_var}.")
            endif()
        endif()
    endforeach()
endfunction()

# ----------------------------------------------------------------------
# nros_build_rtos_static_lib
# ----------------------------------------------------------------------
function(nros_build_rtos_static_lib _name)
    cmake_parse_arguments(_NRSL
        ""                                  # no flag options
        "C_STANDARD"                        # one-value
        "SOURCES;INCLUDES;DEFINES;WARN_FLAGS"  # multi-value
        ${ARGN})

    if(NOT _NRSL_SOURCES)
        message(FATAL_ERROR
            "nros_build_rtos_static_lib(${_name}): SOURCES is required.")
    endif()
    if(NOT _NRSL_C_STANDARD)
        set(_NRSL_C_STANDARD 11)
    endif()
    if(NOT _NRSL_WARN_FLAGS)
        set(_NRSL_WARN_FLAGS -Wno-unused-parameter -Wno-sign-compare)
    endif()

    add_library(${_name} STATIC ${_NRSL_SOURCES})
    if(_NRSL_INCLUDES)
        target_include_directories(${_name} PRIVATE ${_NRSL_INCLUDES})
    endif()
    if(_NRSL_DEFINES)
        target_compile_definitions(${_name} PRIVATE ${_NRSL_DEFINES})
    endif()
    target_compile_options(${_name} PRIVATE ${_NRSL_WARN_FLAGS})
    set_target_properties(${_name} PROPERTIES C_STANDARD ${_NRSL_C_STANDARD})
endfunction()

# ----------------------------------------------------------------------
# nros_compose_platform_target
# ----------------------------------------------------------------------
function(nros_compose_platform_target _name)
    cmake_parse_arguments(_NCPT
        ""
        ""
        "COMPONENTS;INCLUDES;DEFINES;LINK_LIBS"
        ${ARGN})

    add_library(${_name} INTERFACE)
    if(_NCPT_COMPONENTS OR _NCPT_LINK_LIBS)
        target_link_libraries(${_name} INTERFACE
            ${_NCPT_COMPONENTS} ${_NCPT_LINK_LIBS})
    endif()
    if(_NCPT_INCLUDES)
        target_include_directories(${_name} INTERFACE ${_NCPT_INCLUDES})
    endif()
    if(_NCPT_DEFINES)
        target_compile_definitions(${_name} INTERFACE ${_NCPT_DEFINES})
    endif()
endfunction()
