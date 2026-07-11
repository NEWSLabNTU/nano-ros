# cmake/NanoRosVerbs.cmake — RFC-0048 (phase-287 W3): the two role verbs.
#
# `find_package(nano_ros)` pulls this in. It defines the two verbs a nano-ros
# package uses in place of a bare `add_executable`, matching the two roles a
# ROS 2 developer already distinguishes:
#
#   * nano_ros_add_executable(<name> <sources…>) — a STANDALONE ENTRY (own
#     `main` / self-bringup). Emits `add_executable` on native/FreeRTOS/NuttX/
#     ThreadX and `add_library`-into-Zephyr's-`app` on Zephyr; the platform
#     choice is hidden inside `nano_ros_entry`, so the call is identical
#     everywhere.
#   * nano_ros_add_node(<name> <sources…> CLASS <ns::Class>) — a WORKSPACE
#     COMPONENT (no own `main`; registered into a carrier ELF). Always a
#     component library.
#
# Interface codegen is authoritative through `nros_find_interfaces`, which reads
# the package's `package.xml` `<depend>` closure and shells `nros codegen
# resolve-deps` (the proven path every example uses; it resolves well-known ROS
# packages the CLI knows, so no in-tree bundle or sourced ROS install is needed).
# A leaf's `find_package(<msg_pkg> REQUIRED)` line satisfies the ament shape (via
# the compat find-stubs) and validates the dependency; the generation itself is
# driven here from `package.xml`, so C and C++ leaves stay byte-identical.

include_guard(GLOBAL)

# `nano_ros_bootstrap` / `nano_ros_link` (root resolve + auto-link of the
# generated interface libs + platform). Imported already by the config, but keep
# the include so the verbs are usable if a caller pulls this module directly.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBootstrap.cmake")

# ---------------------------------------------------------------------------
# _nros_infer_lang(<out_var> <sources…>)
#   CPP if any source has a C++ extension, else C. Mirrors the inference
#   `nano_ros_entry` does so the verb and the entry agree on LANG.
# ---------------------------------------------------------------------------
function(_nros_infer_lang out_var)
    set(_lang c)
    foreach(_src ${ARGN})
        if(_src MATCHES "\\.(cpp|cxx|cc|C)$")
            set(_lang cpp)
        endif()
    endforeach()
    set(${out_var} "${_lang}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_add_executable(<name> <sources…> [DEPLOY <target>…] [BOARD <board>])
#
# Standalone entry. DEPLOY/BOARD default to the package.xml `<export>` tuple in
# W4; until then DEPLOY defaults to `native` and an embedded board is passed
# explicitly (or comes from a prior `nano_ros_use_board`).
# ---------------------------------------------------------------------------
function(nano_ros_add_executable name)
    cmake_parse_arguments(_NRE "" "BOARD" "DEPLOY" ${ARGN})
    set(_srcs ${_NRE_UNPARSED_ARGUMENTS})
    if(NOT _srcs)
        message(FATAL_ERROR
            "nano_ros_add_executable(${name}): no sources given.")
    endif()
    _nros_infer_lang(_lang ${_srcs})

    if(NOT _NRE_DEPLOY)
        set(_NRE_DEPLOY native)
    endif()

    # Generate the package's declared interface closure in the leaf's language.
    nros_find_interfaces(LANGUAGE ${_lang} SKIP_INSTALL)

    set(_board_arg "")
    if(_NRE_BOARD)
        set(_board_arg BOARD ${_NRE_BOARD})
    endif()
    nano_ros_entry(
        NAME ${name}
        SOURCES ${_srcs}
        DEPLOY ${_NRE_DEPLOY}
        LANG ${_lang}
        ${_board_arg})

    nano_ros_link(${name})
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_add_node(<name> <sources…> CLASS <ns::Class> [DEPLOY <target>…])
#
# Workspace component. Registers a component library via `nano_ros_node_register`;
# the carrier entry ELF is assembled by the workspace root / `nros plan`.
# ---------------------------------------------------------------------------
function(nano_ros_add_node name)
    cmake_parse_arguments(_NRN "" "CLASS" "SOURCES;DEPLOY" ${ARGN})
    set(_srcs ${_NRN_SOURCES} ${_NRN_UNPARSED_ARGUMENTS})
    if(NOT _srcs)
        message(FATAL_ERROR "nano_ros_add_node(${name}): no sources given.")
    endif()
    if(NOT _NRN_CLASS)
        message(FATAL_ERROR
            "nano_ros_add_node(${name}): CLASS <ns::Class> required "
            "(a workspace component registers a class; use "
            "nano_ros_add_executable for a standalone entry with its own main).")
    endif()
    if(NOT _NRN_DEPLOY)
        set(_NRN_DEPLOY native)
    endif()
    _nros_infer_lang(_lang ${_srcs})

    nros_find_interfaces(LANGUAGE ${_lang} SKIP_INSTALL)

    nano_ros_node_register(
        NAME ${name}
        CLASS ${_NRN_CLASS}
        SOURCES ${_srcs}
        DEPLOY ${_NRN_DEPLOY})
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_generate_interfaces(<name> <files…> [DEPENDENCIES <pkgs…>])
#
# For a package that DEFINES its own .msg/.srv/.action — the `rosidl_generate_
# interfaces` analogue (RFC-0048 §5). Thin alias over the low-level generator so
# the ament shape reads uniformly; defaults to C++ bindings like rosidl does.
# ---------------------------------------------------------------------------
function(nano_ros_generate_interfaces name)
    cmake_parse_arguments(_NRG "" "LANGUAGE" "DEPENDENCIES" ${ARGN})
    if(NOT _NRG_LANGUAGE)
        set(_NRG_LANGUAGE CPP)
    endif()
    nros_generate_interfaces(${name}
        ${_NRG_UNPARSED_ARGUMENTS}
        DEPENDENCIES ${_NRG_DEPENDENCIES}
        LANGUAGE ${_NRG_LANGUAGE}
        SKIP_INSTALL)
endfunction()
