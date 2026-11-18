# The cmake half of the bundled runtime-interface check.
#
# A user who opts in to nano-ros's bundled interface packages (by naming the
# bundled root on NROS_INTERFACE_SEARCH_PATH, with no ROS 2 sourced) builds
# against those copies. The runtime makes the parameter services, the event
# publishers and `/rosout` from the packages listed in
# packages/core/nros-orchestration-ir/data/runtime-interface-types.json, and a
# backend that needs a generated descriptor for every type (Cyclone DDS) stops
# the configure when one of them cannot be found
# (cmake/NanoRosRuntimeInterfaceTypes.cmake). This script asks the resolver
# that configure uses, `_nros_find_msg_package_root`, for every package in that
# table and prints `<pkg>=<root>` for each, or stops naming the first it
# cannot resolve.
#
# The Rust test `runtime_interface_packages_resolve_from_the_opted_in_bundle`
# (nros-cli-core, orchestration::backend_link) runs it with no AMENT_PREFIX_PATH.
#
# Usage:
#   NROS_INTERFACE_SEARCH_PATH=<checkout>/packages/cli/interfaces \
#     cmake -P scripts/check/runtime-interface-bundle-resolution.cmake

cmake_minimum_required(VERSION 3.22)
get_filename_component(_root "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(_NANO_ROS_PREFIX "${_root}")
# The find stub pulls in the interface generator on load unless the command
# already exists, and the generator is not scriptable. This script only asks
# the resolver, so a stand-in keeps the generator out.
function(nros_generate_interfaces)
endfunction()
include("${_root}/cmake/find/_NrosFindRosMsgPackage.cmake")

file(READ "${_root}/packages/core/nros-orchestration-ir/data/runtime-interface-types.json" _json)
string(JSON _n LENGTH "${_json}" packages)
if(_n EQUAL 0)
    message(FATAL_ERROR "runtime-interface-bundle-resolution: the table names no package")
endif()
math(EXPR _last "${_n} - 1")
foreach(_i RANGE ${_last})
    string(JSON _pkg GET "${_json}" packages ${_i} name)
    _nros_find_msg_package_root(${_pkg} _pkg_root)
    if(_pkg_root STREQUAL "NOTFOUND")
        message(FATAL_ERROR "runtime-interface-bundle-resolution: `${_pkg}` resolves nowhere")
    endif()
    message(STATUS "${_pkg}=${_pkg_root}")
endforeach()
