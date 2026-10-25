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
                    "it will fail to compile (phase-488).")
                continue()
            endif()
            find_package(${_dep} MODULE QUIET)
            if(${_dep}_FOUND OR TARGET ${_dep}::${_dep})
                list(APPEND _found "${_dep}")
            endif()
        endforeach()
    endif()
    set(${PROJECT_NAME}_FOUND_BUILD_DEPENDS "${_found}" PARENT_SCOPE)
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
endfunction()

function(ament_auto_add_executable target)
    add_executable(${target} ${ARGN})
    target_link_libraries(${target} PRIVATE NanoRos::NanoRosCpp)
    _nros_ament_apply_target_settings(${target})
    _nros_ament_auto_target_deps(${target})
    if(COMMAND nros_platform_link_app)
        nros_platform_link_app(${target})
    endif()
endfunction()

# --- ament_target_dependencies / ament_export_* shims -------------------------

# `ament_target_dependencies` lives in its own file because the Zephyr arm of
# `nano_rosConfig.cmake` includes it without the rest of this surface.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosAmentTargetDeps.cmake")

function(ament_export_dependencies)
    # No-op — nano-ros has no ament install layout; the embedded build does not
    # need exported package deps.
endfunction()

function(ament_export_include_directories)
    # No-op — INSTALL_INTERFACE include dirs are an ament-install concept.
endfunction()

function(ament_export_libraries)
endfunction()

function(ament_export_targets)
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
    if(COMMAND nros_platform_link_app)
        nros_platform_link_app(${_RCRN_EXECUTABLE})
    endif()
endfunction()

# --- ament_auto_package + ament_package --------------------------------------

function(ament_auto_package)
    # Stock form: `ament_auto_package(INSTALL_TO_SHARE config launch)`. The
    # nano-ros embedded target has no `share/<pkg>` install layout; the args
    # are silently ignored. Launch/yaml is Phase 209.F.
endfunction()

function(ament_package)
endfunction()
