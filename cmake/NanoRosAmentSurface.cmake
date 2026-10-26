# NanoRosAmentSurface.cmake — the ament / rclcpp CMake surface of nano-ros.
#
# Part of nano-ros's own CMake package (RFC-0096 D4, phase-482 W1): included by
# `nano_rosConfig.cmake` and `nano_ros_workspace()`, or by a project that brings
# nano-ros in with `add_subdirectory(<nano-ros> nano_ros)`. It is what lets a
# stock ROS 2 / ament_cmake_auto `CMakeLists.txt` build against nano-ros:
#
#   add_subdirectory(<path-to-nano-ros> nano_ros)
#   include(<path-to-nano-ros>/cmake/NanoRosAmentSurface.cmake)
#   # everything below is unmodified ament_cmake_auto:
#   find_package(ament_cmake_auto REQUIRED)
#   ament_auto_find_build_dependencies()
#   ament_auto_add_library(my_node SHARED src/my_node.cpp)
#   rclcpp_components_register_node(my_node PLUGIN "my_ns::MyNode" EXECUTABLE my_node_exe)
#   ament_auto_package(INSTALL_TO_SHARE config launch)
#
# What it provides
# ----------------
# * `cmake/find/` on CMAKE_MODULE_PATH: `find_package(<pkg>)` for the common
#   ROS 2 packages resolves to nano-ros's own `Find<pkg>.cmake`. Message
#   packages route into nano-ros codegen; `rclcpp` / `rclcpp_components` define
#   their IMPORTED targets over `NanoRos::NanoRosCpp`; the ament build-tool
#   packages only report found.
#
#   These are FIND MODULES and not `<pkg>Config.cmake` files on purpose:
#   `find_package()` tries module mode before config mode, so they win over an
#   installed ROS 2 (`/opt/ros/<distro>` on CMAKE_PREFIX_PATH after sourcing
#   `setup.bash`) instead of losing to it.
# * The `ament_*` / `rclcpp_components_*` cmake functions, translated to
#   `add_library`/`add_executable` + the nano-ros runtime link.
# * `<rclcpp/rclcpp.hpp>` and `<rclcpp_components/register_node_macro.hpp>`
#   need nothing from here: they are nros-cpp headers, on the include path of
#   every target linking `NanoRos::NanoRosCpp`.
# * `rclcpp_components_register_node(... EXECUTABLE <bin>)` synthesises a thin
#   `int main()` that constructs the registered class and spins it. nano-ros is
#   single-binary (no runtime composition), so each registration becomes one
#   self-contained executable.
#
# Out of scope (the porting user's call):
# * Launch / parameter yaml.
# * `ament_target_dependencies` on project-specific helper packages (e.g.
#   `autoware_universe_utils`) — the user vendors or replaces those.

if(_NROS_AMENT_SURFACE_INCLUDED)
    return()
endif()
set(_NROS_AMENT_SURFACE_INCLUDED TRUE)

# --- Find-module directory ----------------------------------------------------
set(_nros_ament_find_dir "${CMAKE_CURRENT_LIST_DIR}/find")
if(NOT "${_nros_ament_find_dir}" IN_LIST CMAKE_MODULE_PATH)
    list(PREPEND CMAKE_MODULE_PATH "${_nros_ament_find_dir}")
endif()

# Pull the message-package resolver proactively so it emits the workspace
# Find<pkg>.cmake modules for every pkg under `NROS_INTERFACE_SEARCH_PATH` at
# include time (phase-210 A.2/A.4). Without this the emit only happens when a
# per-pkg delegator fires — which can be after a consumer's
# `find_package(<workspace_pkg>)` runs, defeating it.
include("${_nros_ament_find_dir}/_NrosFindRosMsgPackage.cmake")

# issue 1467 — `nros_link_runtime_umbrella()`: the ONE decision about which
# runtime umbrella a target links, and whether the consumer wants one.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRuntimeUmbrella.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosDiagnostics.cmake")

# --- Sanity: nros-cpp must be loaded -----------------------------------------
# Two consumption shapes for `NanoRos::NanoRosCpp`:
#  1. Native / `add_subdirectory(<nano-ros>)` — the root CMakeLists.txt
#     publishes the IMPORTED INTERFACE target directly.
#  2. Zephyr — `find_package(Zephyr)` auto-loads the nros zephyr module
#     (`zephyr/CMakeLists.txt`) which calls `zephyr_library_named(nros)`
#     under `CONFIG_NROS_CPP_API=y`. There's no `NanoRos::NanoRosCpp`
#     target on Zephyr — the `nros` zephyr_library is its equivalent.
#     Bridge via ALIAS so the rest of this module and the Find modules
#     see one canonical name (phase-210 E.3.c).
if(CONFIG_NROS_CPP_API AND NOT TARGET NanoRos::NanoRosCpp AND TARGET nros)
    add_library(NanoRos::NanoRosCpp ALIAS nros)
endif()

if(NOT TARGET NanoRos::NanoRosCpp)
    message(FATAL_ERROR
        "NanoRosAmentSurface: NanoRos::NanoRosCpp not found.\n"
        "Include this module AFTER bringing nano-ros in:\n"
        "  add_subdirectory(<path-to-nano-ros> nano_ros)\n"
        "or `find_package(nano_ros REQUIRED)`,\n"
        "or build inside a Zephyr application with CONFIG_NROS_CPP_API=y.")
endif()

# --- add_library: a SHARED or MODULE library builds STATIC -------------------
#
# phase-488 (ros2/demos `composition`). The stock component shape is a plain
# `add_library(<name> SHARED ...)` that links rclcpp, so that class_loader can
# `dlopen` it. nano-ros has no class_loader, and its runtime is a static
# archive. A SHARED library that links rclcpp takes its own copy of that
# archive. The image then fails to link (`undefined reference to
# nros_app_register_backends` from each `.so`), or links and runs two runtimes,
# which is how Autoware's component died at start-up. One image is one binary,
# so a SHARED or MODULE library builds STATIC, and the configure says so once
# per target. `ament_auto_add_library` already did this; this covers the plain
# `add_library` a stock CMakeLists writes beside it.
#
# This overrides the CMake builtin for every `add_library` processed after this
# file is included, which is the user's package: nano-ros declares its own
# targets before a project can include this file. Left alone:
#   * an IMPORTED library, because it names a file that already exists;
#   * an ALIAS, OBJECT or INTERFACE library, and one with no type (which
#     follows `BUILD_SHARED_LIBS`).
# CMake keeps the previous definition as `_add_library`, one level deep. A
# GLOBAL property makes the override happen once, so a second include of this
# file, from a scope where the include guard variable is not visible, does not
# wrap the wrapper. A project that overrides `add_library` itself after this
# include replaces this override, and its SHARED components then build SHARED
# again.
get_property(_nros_add_library_wrapped GLOBAL PROPERTY NROS_AMENT_ADD_LIBRARY_WRAPPED)
if(NOT _nros_add_library_wrapped)
    set_property(GLOBAL PROPERTY NROS_AMENT_ADD_LIBRARY_WRAPPED TRUE)
    function(add_library name)
        set(_nal_args ${ARGN})
        list(LENGTH _nal_args _nal_n)
        set(_nal_kind "")
        if(_nal_n GREATER 0)
            list(GET _nal_args 0 _nal_kind)
        endif()
        if((_nal_kind STREQUAL "SHARED" OR _nal_kind STREQUAL "MODULE")
           AND NOT "IMPORTED" IN_LIST _nal_args)
            list(REMOVE_AT _nal_args 0)
            message(STATUS "nano-ros: add_library(${name} ${_nal_kind}) builds STATIC: "
                           "the image is one binary with one runtime (phase-488)")
            _add_library(${name} STATIC ${_nal_args})
        else()
            _add_library(${name} ${ARGN})
        endif()
    endfunction()
endif()

# --- add_executable: on a board, an executable IS an image -------------------
#
# phase-518. A stock CMakeLists builds its program with a plain
# `add_executable` (ros2/demos `manual_composition`), and the ament verbs and
# `rclcpp_components_register_node(... EXECUTABLE ...)` create theirs the same
# way. On posix that is the whole story. On a board it is not: an image needs
# the board's entry, which defines `main` and calls the program's renamed
# `main`, plus the per-image app config, locator, domain and network seed that
# `nano_ros_entry` generates. Built bare, a FreeRTOS executable collided with
# the board's `main` and then failed to link `NROS_APP_CONFIG`. So on a board
# every executable created after this file is included is built by
# `nano_ros_add_executable(<name> <sources> ROS2_MAIN)`, the verb the port
# templates name by hand: one road to an image, whichever CMake verb asked.
#
# On Zephyr the verb compiles the sources into the kernel's `app` and leaves
# `<name>` a placeholder linked into it, so what the CMakeLists then links into
# `<name>` reaches the image too.
#
# Left alone: an IMPORTED or ALIAS executable, and every executable nano-ros
# creates itself. Those sites set `_NROS_IN_IMAGE_ADD_EXECUTABLE` first: the
# image `nano_ros_entry` builds, a generated carrier, a board's own image. An
# in-tree example that calls `nano_ros_add_executable` directly reaches the
# builtin that way, not a second image. posix is not wrapped: there the
# program keeps its own `main`.
get_property(_nros_add_executable_wrapped GLOBAL PROPERTY NROS_AMENT_ADD_EXECUTABLE_WRAPPED)
if(NOT _nros_add_executable_wrapped AND NROS_PLATFORM AND NOT NROS_PLATFORM STREQUAL "posix")
    set_property(GLOBAL PROPERTY NROS_AMENT_ADD_EXECUTABLE_WRAPPED TRUE)
    # The verb and `nano_ros_entry` under it, included here at FILE scope:
    # an `include()` inside the wrapper would run in a function frame and drop
    # the files' directory variables when it returns (CLAUDE.md, cmake pitfall).
    include("${CMAKE_CURRENT_LIST_DIR}/NanoRosNodeRegister.cmake")
    function(add_executable name)
        if(_NROS_IN_IMAGE_ADD_EXECUTABLE OR "IMPORTED" IN_LIST ARGN OR "ALIAS" IN_LIST ARGN)
            _add_executable(${name} ${ARGN})
            return()
        endif()
        set(_nae_srcs ${ARGN})
        list(REMOVE_ITEM _nae_srcs WIN32 MACOSX_BUNDLE EXCLUDE_FROM_ALL)
        message(STATUS "nano-ros: add_executable(${name}) is a ${NROS_PLATFORM} image: "
                       "built as nano_ros_add_executable(${name} ... ROS2_MAIN) (phase-518)")
        set(_NROS_IN_IMAGE_ADD_EXECUTABLE TRUE)
        nano_ros_add_executable(${name} ${_nae_srcs} ROS2_MAIN)
        # `ROS2_MAIN` made the C++ of the calling directory hosted; this
        # function is between that directory and the verb.
        set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}" PARENT_SCOPE)
    endfunction()
endif()

# --- An installed ROS 2 package nano-ros does not model ----------------------
#
# phase-488. `find_package(<pkg>)` for a package with no module here falls to
# CONFIG mode, and with a ROS install sourced it loads the installed
# `<pkg>Config.cmake`. That is upstream's CMake for upstream's C++ build of the
# package (Nav2's `nav2_util` finds `bondcpp` and `tf2_ros`; ros2/demos finds
# `rcutils`), and it calls ament internals nano-ros does not define. The
# configure then died on "Unknown CMake command normalize_path", which names
# neither the package nor what to do. Measured over the 330 `*Config.cmake`
# files of a ROS 2 Humble install: 214 call one of the six internals trapped
# below (`normalize_path` the first of them in 125, `list_append_unique` in
# 51). The other 116 call none of them: ament's own `ament_cmake_*` tool
# packages and plain-CMake vendor packages, which this trap does not reach. A
# message package is among the 214, and nano-ros routes it to its own codegen
# before CONFIG mode is tried. No supported build reaches the six, since they
# are undefined today, so
# a trap changes only what such a configure says: the package, the file and
# the choices. A real ament on the module path (a project that includes
# ament_cmake itself) keeps its own definitions.
function(_nros_ament_installed_package_refusal command)
    set(_file "${CMAKE_CURRENT_LIST_FILE}")
    set(_pkg "")
    if(_file MATCHES "/share/([^/]+)/cmake/")
        set(_pkg "${CMAKE_MATCH_1}")
    endif()
    message(FATAL_ERROR
        "nano-ros: find_package(${_pkg}) loaded the INSTALLED ROS 2 package's "
        "CMake (${_file}), which called the ament internal `${command}`.\n"
        "nano-ros provides no module for `${_pkg}`, so CMake fell back to the "
        "copy your sourced ROS install carries: upstream's C++ build of it, "
        "which cannot link into a nano-ros image.\n"
        "Either remove the find_package(${_pkg}) (and the code that uses it), "
        "or port `${_pkg}` into your workspace as a package of its own, built "
        "with the same ament verbs. If it is a message package, put it under "
        "NROS_INTERFACE_SEARCH_PATH or AMENT_PREFIX_PATH with its .msg files.")
endfunction()
foreach(_nros_trap normalize_path list_append_unique get_executable_path
        ament_register_extension ament_index_get_resources ament_index_register_resource)
    if(NOT COMMAND ${_nros_trap})
        cmake_language(EVAL CODE "
            function(${_nros_trap})
                _nros_ament_installed_package_refusal(${_nros_trap})
            endfunction()")
    endif()
endforeach()

# --- Per-target settings for an ament-built target ---------------------------
function(_nros_ament_apply_target_settings target)
    if(NOT TARGET ${target})
        return()
    endif()
    # phase-438 W2 — ASK for the std surface. A ported file subclasses
    # `rclcpp::Node` and spells its members in `std::shared_ptr` /
    # `std::string` / `std::vector`; that surface is reachable only through
    # `NROS_CPP_STD`, never discovered from the include path (issue 1187 —
    # `__has_include(<string>)` is TRUE on arm-none-eabi `-ffreestanding`,
    # where including it is a hard `#error`). Applied per target, and only to
    # targets created through the ament verbs, so it follows the ported code
    # instead of leaking into images that use `rclcpp::Node` directly.
    target_compile_definitions(${target} PRIVATE NROS_CPP_STD=1)
    # phase-488 — and upstream's parameter services, when the runtime has them
    # (an ament package on a host gets `param_services` by default —
    # `nros_ament_port_default_features`). The node constructor starts them.
    if("param_services" IN_LIST NROS_FEATURES)
        target_compile_definitions(${target} PRIVATE NROS_AMENT_START_PARAMETER_SERVICES=1)
    endif()
endfunction()

# --- ament_cmake_auto shims ---------------------------------------------------

# phase-488 — upstream's `ament_auto_find_build_dependencies()` reads the
# package's `package.xml`, `find_package()`s every build dependency, and the
# `ament_auto_add_*` verbs then give each target the package's `include/` and
# every dependency found. This used to be a no-op, which only worked for a
# package whose sources include nothing but `rclcpp` and messages linked by
# hand; a stock Autoware package (`autoware_package()` calls this) failed on its
# own header.
#
# A dependency is looked up only when nano-ros can answer for it: a module on
# `CMAKE_MODULE_PATH` (`cmake/find/`, or a message-package stub). Anything
# else would resolve in CONFIG mode against an installed ROS package — upstream
# rclcpp-linked C++ — so it is reported and skipped instead; if the sources
# include it, the compile names the missing header.
function(ament_auto_find_build_dependencies)
    set(_px "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
    set(_found "")
    if(EXISTS "${_px}")
        nros_read_package_xml_body("${_px}" _body)
        string(REGEX MATCHALL
            "<(depend|build_depend|build_export_depend)[^>]*>[ \t\r\n]*[A-Za-z0-9_]+[ \t\r\n]*</"
            _tags "${_body}")
        set(_deps "")
        foreach(_tag IN LISTS _tags)
            string(REGEX REPLACE ".*>[ \t\r\n]*([A-Za-z0-9_]+)[ \t\r\n]*</$" "\\1" _dep "${_tag}")
            list(APPEND _deps "${_dep}")
        endforeach()
        list(REMOVE_DUPLICATES _deps)
        foreach(_dep IN LISTS _deps)
            set(_module "")
            foreach(_dir IN LISTS CMAKE_MODULE_PATH)
                if(EXISTS "${_dir}/Find${_dep}.cmake")
                    set(_module "${_dir}/Find${_dep}.cmake")
                    break()
                endif()
            endforeach()
            if(NOT _module)
                message(STATUS
                    "nano-ros: ${PROJECT_NAME} depends on `${_dep}`, which nano-ros "
                    "does not provide; it is not looked up. A source that includes "
                    "it will fail to compile.")
                nros_message_detail(
                    "nano-ros:   `${_dep}` has no Find module on CMAKE_MODULE_PATH, "
                    "so the ament surface skips it rather than failing the configure "
                    "(phase-488).")
                continue()
            endif()
            find_package(${_dep} MODULE QUIET)
            if(${_dep}_FOUND OR TARGET ${_dep}::${_dep})
                list(APPEND _found "${_dep}")
            endif()
        endforeach()
    endif()
    set(${PROJECT_NAME}_FOUND_BUILD_DEPENDS "${_found}" PARENT_SCOPE)
    # phase-518 — `find_package(rclcpp)` above made THIS function's C++ hosted
    # (NanoRosPortedCxx.cmake); the package's directory is the caller.
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}" PARENT_SCOPE)
endfunction()

# What `ament_auto_add_*` gives a target beyond the runtime, as upstream does:
# the package's `include/` and every dependency the find step answered.
function(_nros_ament_auto_target_deps target)
    if(IS_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include")
        get_target_property(_type ${target} TYPE)
        if(_type STREQUAL "EXECUTABLE")
            target_include_directories(${target} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/include")
        else()
            target_include_directories(${target} PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/include")
        endif()
    endif()
    if(${PROJECT_NAME}_FOUND_BUILD_DEPENDS)
        ament_target_dependencies(${target} ${${PROJECT_NAME}_FOUND_BUILD_DEPENDS})
    endif()
endfunction()

function(ament_auto_add_library target kind)
    # kind ∈ SHARED | STATIC | MODULE. Every kind builds STATIC (phase-488).
    # Upstream builds a component SHARED so class_loader can dlopen it; nano-ros
    # has no class_loader, and its runtime is a static archive. A SHARED library
    # linked that archive into itself, the executable linked it again, and the
    # image ran two runtimes: Autoware's `autoware_external_cmd_selector`
    # (`ament_auto_add_library(... SHARED ...)`, the stock component shape)
    # linked and then died at start-up on `undefined symbol:
    # nros_config_variant_sz_...`. One image is one binary.
    set(_srcs ${ARGN})
    if("${kind}" STREQUAL "SHARED" OR "${kind}" STREQUAL "STATIC"
       OR "${kind}" STREQUAL "MODULE")
        add_library(${target} STATIC ${_srcs})
    else()
        # If no kind keyword was passed (ament_cmake_auto often omits it),
        # treat the first arg as a source path.
        add_library(${target} STATIC ${kind} ${_srcs})
    endif()
    # issue 1467 — PUBLIC, so the umbrella propagates to an unknown consumer and
    # goes through the guarded resolver. The sibling `ament_auto_add_executable`
    # below stays a literal PRIVATE link: there the consumer IS the target, and
    # the NuttX boards match that name as a string.
    nros_link_runtime_umbrella(${target} PUBLIC CANDIDATES NanoRos::NanoRosCpp)
    _nros_ament_apply_target_settings(${target})
    _nros_ament_auto_target_deps(${target})
    set_property(GLOBAL APPEND PROPERTY NROS_WS_AUTO_LIBS_${PROJECT_NAME} ${target})
endfunction()

function(ament_auto_add_executable target)
    add_executable(${target} ${ARGN})
    target_link_libraries(${target} PRIVATE NanoRos::NanoRosCpp)
    _nros_ament_apply_target_settings(${target})
    _nros_ament_auto_target_deps(${target})
    _nros_ament_link_app(${target})
endfunction()

# --- ament_target_dependencies / ament_export_* shims -------------------------

# `ament_target_dependencies` lives in its own file because the Zephyr arm of
# `nano_rosConfig.cmake` includes it without the rest of this surface.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosAmentTargetDeps.cmake")

# phase-488 (Nav2) — what a package exports, recorded for its siblings.
#
# Upstream installs a package and its siblings find the installed copy. In a
# nano-ros workspace every package is configured in one CMake run, so a
# sibling's `find_package(<pkg>)` must reach the targets `<pkg>` declared in
# that run (Nav2's `nav2_velocity_smoother` finds `nav2_util`; before this it
# found the installed upstream copy). These verbs record the exports per
# project, and `ament_package()` turns them into `<pkg>::<pkg>`. The
# workspace Find stub (`_nros_find_workspace_package`) resolves to it.
# Include directories are not taken from `ament_export_include_directories`,
# which names INSTALL paths (`include/${PROJECT_NAME}`, a directory the
# source tree does not have); `ament_package()` exports the package's own
# `include/` instead, which is where the installed headers come from.
function(ament_export_dependencies)
    set_property(GLOBAL APPEND PROPERTY NROS_WS_EXPORT_DEPS_${PROJECT_NAME} ${ARGN})
endfunction()

function(ament_export_include_directories)
endfunction()

function(ament_export_libraries)
    set_property(GLOBAL APPEND PROPERTY NROS_WS_EXPORT_LIBS_${PROJECT_NAME} ${ARGN})
endfunction()

function(ament_export_targets)
    cmake_parse_arguments(_AET "HAS_LIBRARY_TARGET" "" "" ${ARGN})
    set_property(GLOBAL APPEND PROPERTY NROS_WS_EXPORT_LIBS_${PROJECT_NAME} ${_AET_UNPARSED_ARGUMENTS})
endfunction()

# --- rclcpp_components_register_node ------------------------------------------

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosComponentMain.cmake")

function(rclcpp_components_register_node component_target)
    cmake_parse_arguments(_RCRN "" "PLUGIN;EXECUTABLE;RESOURCE_INDEX" "" ${ARGN})
    if(NOT _RCRN_EXECUTABLE OR NOT _RCRN_PLUGIN)
        # In the upstream macro, omitting EXECUTABLE installs a plugin index
        # consumed by the runtime ComponentManager. nano-ros has no dynamic
        # composer — without an EXECUTABLE name there's nothing to emit.
        return()
    endif()
    set(_gen_dir "${CMAKE_CURRENT_BINARY_DIR}/nros_ament_main")
    set(_gen_src "${_gen_dir}/${_RCRN_EXECUTABLE}_main.cpp")
    file(MAKE_DIRECTORY "${_gen_dir}")
    # Generated entry point: NanoRosComponentMain.cmake says what it holds and
    # why it declares the class rather than including its header.
    nros_component_main_source("${_RCRN_PLUGIN}" "${_RCRN_EXECUTABLE}" _gen_content)
    file(GENERATE OUTPUT "${_gen_src}" CONTENT "${_gen_content}")
    add_executable(${_RCRN_EXECUTABLE} "${_gen_src}")
    target_link_libraries(${_RCRN_EXECUTABLE} PRIVATE
        ${component_target} NanoRos::NanoRosCpp)
    _nros_ament_apply_target_settings(${_RCRN_EXECUTABLE})
    _nros_ament_link_app(${_RCRN_EXECUTABLE})
endfunction()

# phase-488 — upstream's plural form, `rclcpp_components_register_nodes(<target>
# [RESOURCE_INDEX <index>] <class>...)`, records each class in the ament
# resource index for the runtime component container to `dlopen`. That is all
# it does: it never emits an executable. nano-ros has no container and no
# resource index, and the factory `RCLCPP_COMPONENTS_REGISTER_NODE` defines in
# the source is what a hand-written main (ros2/demos `manual_composition`)
# uses, so there is nothing to record. A stock CMakeLists calls it once per
# component, and was refused as an unknown command.
function(rclcpp_components_register_nodes component_target)
endfunction()

# --- ament_python_install_package --------------------------------------------

# phase-488 — Nav2's `nav2_common` installs its launch helpers as a Python
# package. nano-ros has no install layout, so there is nothing to install.
function(ament_python_install_package)
endfunction()

# --- ament_environment_hooks -------------------------------------------------

# phase-488 — upstream installs shell hooks that `setup.bash` sources (ros2/demos
# `composition` adds one to put its component libraries on the library path).
# nano-ros has no install layout and no `setup.bash`, so there is no hook to
# install.
function(ament_environment_hooks)
endfunction()

# --- ament_auto_package + ament_package --------------------------------------

function(ament_auto_package)
    # Stock form: `ament_auto_package(INSTALL_TO_SHARE config launch)`. The
    # nano-ros embedded target has no `share/<pkg>` install layout, so
    # INSTALL_TO_SHARE is ignored (launch/yaml is Phase 209.F). What upstream's
    # exports for its siblings, every `ament_auto_add_library` target and the
    # package's build dependencies, is recorded as `ament_package()` records it.
    cmake_parse_arguments(_AAP "" "" "INSTALL_TO_SHARE;CONFIG_EXTRAS" ${ARGN})
    get_property(_libs GLOBAL PROPERTY NROS_WS_AUTO_LIBS_${PROJECT_NAME})
    if(_libs)
        ament_export_libraries(${_libs})
    endif()
    if(${PROJECT_NAME}_FOUND_BUILD_DEPENDS)
        ament_export_dependencies(${${PROJECT_NAME}_FOUND_BUILD_DEPENDS})
    endif()
    ament_package(CONFIG_EXTRAS ${_AAP_CONFIG_EXTRAS})
endfunction()

# phase-488 — `<pkg>::<pkg>`: an INTERFACE target over what the package
# exported (see `ament_export_libraries` above), with an `<pkg>::<target>`
# alias per exported library target, as upstream's `ament_export_targets`
# namespaces them. Its CONFIG_EXTRAS files (Nav2's `nav2_common` defines
# `nav2_package()` in one) are recorded and run by the sibling's
# `find_package(<pkg>)`, with `<pkg>_DIR` set to the directory upstream
# installs them into: the package's `cmake/` when it has one (which upstream
# installs beside the extras), its root otherwise.
function(ament_package)
    cmake_parse_arguments(_AP "" "" "CONFIG_EXTRAS;CONFIG_EXTRAS_POST" ${ARGN})
    set(_pkg "${PROJECT_NAME}")
    get_property(_done GLOBAL PROPERTY NROS_WS_PACKAGE_${_pkg} SET)
    if(_done)
        return()
    endif()
    set(_extras "")
    foreach(_e IN LISTS _AP_CONFIG_EXTRAS _AP_CONFIG_EXTRAS_POST)
        get_filename_component(_abs "${_e}" ABSOLUTE BASE_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
        list(APPEND _extras "${_abs}")
    endforeach()
    set(_dir "${CMAKE_CURRENT_SOURCE_DIR}")
    if(IS_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
        set(_dir "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
    endif()
    set_property(GLOBAL PROPERTY NROS_WS_PACKAGE_DIR_${_pkg} "${_dir}")
    set_property(GLOBAL PROPERTY NROS_WS_PACKAGE_EXTRAS_${_pkg} "${_extras}")
    set_property(GLOBAL PROPERTY NROS_WS_PACKAGE_${_pkg} "${CMAKE_CURRENT_SOURCE_DIR}")
    # A message package already has `<pkg>::<pkg>`, from codegen.
    if(TARGET ${_pkg}::${_pkg})
        return()
    endif()
    add_library(_nros_ws_${_pkg} INTERFACE)
    add_library(${_pkg}::${_pkg} ALIAS _nros_ws_${_pkg})
    get_property(_libs GLOBAL PROPERTY NROS_WS_EXPORT_LIBS_${_pkg})
    list(REMOVE_DUPLICATES _libs)
    foreach(_l IN LISTS _libs)
        if(TARGET ${_l})
            target_link_libraries(_nros_ws_${_pkg} INTERFACE ${_l})
            if(NOT TARGET ${_pkg}::${_l})
                add_library(${_pkg}::${_l} ALIAS ${_l})
            endif()
        endif()
    endforeach()
    get_property(_deps GLOBAL PROPERTY NROS_WS_EXPORT_DEPS_${_pkg})
    list(REMOVE_DUPLICATES _deps)
    foreach(_d IN LISTS _deps)
        if(TARGET ${_d}::${_d})
            target_link_libraries(_nros_ws_${_pkg} INTERFACE ${_d}::${_d})
        endif()
    endforeach()
    if(IS_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include")
        target_include_directories(_nros_ws_${_pkg} INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
    endif()
endfunction()
