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

# `_nros_bootstrap` / `_nros_link` (root resolve + auto-link of the generated
# interface libs + platform — config internals since W8 retired the public
# spelling). Imported already by the config, but keep the include so the verbs
# are usable if a caller pulls this module directly.
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
# _nros_generate_declared_interfaces(<lang>)
#   Run interface codegen for the invoking package's declared interface deps —
#   but only when its package.xml actually declares one. A no-dep leaf (a pure
#   `nros::init` demo, or an own-msg pkg that generates via
#   nano_ros_generate_interfaces) would otherwise trip a spurious
#   "no interface packages resolved" warning from nros_find_interfaces.
# ---------------------------------------------------------------------------
function(_nros_generate_declared_interfaces lang)
    set(_pkgxml "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
    if(NOT EXISTS "${_pkgxml}")
        return()
    endif()
    file(READ "${_pkgxml}" _body)
    if(_body MATCHES "<(depend|build_depend|exec_depend|run_depend|build_export_depend)>")
        nros_find_interfaces(LANGUAGE ${lang} SKIP_INSTALL)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_add_executable(<name> <sources…> [DEPLOY <target>…] [BOARD <board>]
#     [LAUNCH <pkg:launch.xml>] [TYPED] [HOST <h>] [LOCATOR <l>] [ARGS <a>…])
#
# Standalone entry. DEPLOY/BOARD default to the package.xml `<export>` tuple in
# W4; until then DEPLOY defaults to `native` and an embedded board is passed
# explicitly (or comes from a prior `nano_ros_use_board`).
#
# 287-W6 workspace slice 3 — LAUNCH/TYPED/HOST/LOCATOR/ARGS pass through to
# `nano_ros_entry` so a workspace Entry pkg (multi-node carrier generated from
# a bringup launch manifest) can use the ament verb instead of the raw
# `nano_ros_entry(...)` call.
# ---------------------------------------------------------------------------
function(nano_ros_add_executable name)
    cmake_parse_arguments(_NRE "TYPED" "BOARD;LAUNCH;MODEL;HOST;LOCATOR;LANG" "DEPLOY;SOURCES;ARGS" ${ARGN})
    set(_srcs ${_NRE_SOURCES} ${_NRE_UNPARSED_ARGUMENTS})
    if(NOT _srcs AND NOT _NRE_LAUNCH AND NOT _NRE_MODEL)
        message(FATAL_ERROR
            "nano_ros_add_executable(${name}): no sources given "
            "(a LAUNCH-generated entry may omit sources; anything else "
            "needs at least one).")
    endif()
    _nros_infer_lang(_lang ${_srcs})

    # DEPLOY/BOARD default to the package.xml <export><nano_ros> tuple that
    # find_package(nano_ros) parsed into NROS_DEPLOY / NROS_BOARD (RFC-0048 §4);
    # an explicit keyword still wins.
    if(NOT _NRE_DEPLOY)
        if(NROS_DEPLOY)
            set(_NRE_DEPLOY "${NROS_DEPLOY}")
        else()
            set(_NRE_DEPLOY native)
        endif()
    endif()
    if(NOT _NRE_BOARD AND NROS_BOARD)
        set(_NRE_BOARD "${NROS_BOARD}")
    endif()

    # Generate the package's declared interface closure in the leaf's language
    # (no-op when package.xml declares no interface deps).
    _nros_generate_declared_interfaces(${_lang})

    set(_board_arg "")
    if(_NRE_BOARD)
        set(_board_arg BOARD ${_NRE_BOARD})
    endif()
    # Entry-carrier knobs (LAUNCH-generated multi-node entries + typed
    # components + connection overrides) forward verbatim.
    set(_entry_extra "")
    if(_NRE_LAUNCH)
        list(APPEND _entry_extra LAUNCH ${_NRE_LAUNCH})
    endif()
    # R1 / W4.2 — the canonical resolved-model input (RFC-0052).
    if(_NRE_MODEL)
        list(APPEND _entry_extra MODEL ${_NRE_MODEL})
    endif()
    if(_NRE_TYPED)
        list(APPEND _entry_extra TYPED)
    endif()
    if(_NRE_HOST)
        list(APPEND _entry_extra HOST ${_NRE_HOST})
    endif()
    if(_NRE_LOCATOR)
        list(APPEND _entry_extra LOCATOR ${_NRE_LOCATOR})
    endif()
    if(_NRE_ARGS)
        list(APPEND _entry_extra ARGS ${_NRE_ARGS})
    endif()
    # Language: explicit LANG wins (the only way a LAUNCH-only entry — no
    # sources to infer from — can select C; nano_ros_entry's sourceless
    # default is cpp). With sources, infer; without either, let nano_ros_entry
    # default.
    set(_lang_arg "")
    if(_NRE_LANG)
        set(_lang_arg LANG ${_NRE_LANG})
    elseif(_srcs)
        set(_lang_arg LANG ${_lang})
    endif()
    nano_ros_entry(
        NAME ${name}
        SOURCES ${_srcs}
        DEPLOY ${_NRE_DEPLOY}
        ${_lang_arg}
        ${_board_arg}
        ${_entry_extra})

    _nros_link(${name})
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_add_node(<name> <sources…> CLASS <ns::Class> [DEPLOY <target>…])
#
# Workspace component. Registers a component library via `nano_ros_node_register`;
# the carrier entry ELF is assembled by the workspace root / `nros plan`.
# ---------------------------------------------------------------------------
function(nano_ros_add_node name)
    cmake_parse_arguments(_NRN "TYPED" "CLASS;HEADER;SHAPE" "SOURCES;DEPLOY;CALLBACK_GROUPS" ${ARGN})
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
    # DEPLOY defaults to the package.xml tuple; with neither, the component is
    # registered CARRIER-LESS (no DEPLOY forwarded) — a workspace member's
    # carrier is assembled by its Entry pkg / the workspace root, exactly like
    # the pre-verb `nano_ros_node_register` calls that omitted DEPLOY. (287-W6
    # slice 3: the earlier implicit `native` default forced every member onto
    # the per-node carrier path — fatal on FreeRTOS, whose carrier requires
    # TYPED, and a spurious extra exe on posix.)
    if(NOT _NRN_DEPLOY AND NROS_DEPLOY)
        set(_NRN_DEPLOY "${NROS_DEPLOY}")
    endif()
    _nros_infer_lang(_lang ${_srcs})

    # Generate the package's declared interface closure (no-op when package.xml
    # declares none — a TYPED component publishes raw topics with no bindings).
    _nros_generate_declared_interfaces(${_lang})

    # TYPED (RFC-0043): a typed component carries the type name as a string, no
    # generated bindings — forward the flag to the register.
    set(_typed_arg "")
    if(_NRN_TYPED)
        set(_typed_arg TYPED)
    endif()
    # 287-W6 workspace slice 2 — pass-through for the register's remaining
    # per-component knobs (rclcpp-shape components, custom class headers,
    # RFC-0047 callback-group declarations) so every node member can use the
    # ament verb, not just the plain TYPED ones.
    set(_extra_args "")
    if(_NRN_HEADER)
        list(APPEND _extra_args HEADER ${_NRN_HEADER})
    endif()
    if(_NRN_SHAPE)
        list(APPEND _extra_args SHAPE ${_NRN_SHAPE})
    endif()
    if(_NRN_CALLBACK_GROUPS)
        list(APPEND _extra_args CALLBACK_GROUPS ${_NRN_CALLBACK_GROUPS})
    endif()
    set(_deploy_arg "")
    if(_NRN_DEPLOY)
        set(_deploy_arg DEPLOY ${_NRN_DEPLOY})
    endif()
    nano_ros_node_register(
        NAME ${name}
        CLASS ${_NRN_CLASS}
        LANGUAGE ${_lang}
        SOURCES ${_srcs}
        ${_deploy_arg}
        ${_typed_arg}
        ${_extra_args})
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
