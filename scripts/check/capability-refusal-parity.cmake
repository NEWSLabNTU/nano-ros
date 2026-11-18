# The cmake half of the capability-refusal parity test.
#
# Two capability refusals are spelled twice, once in Rust
# (`backend_link::undeclared_capability_refusal`, `unservable_services_refusal`)
# and once in cmake (`nros_capability_features` in cmake/NanoRosFeatureSet.cmake).
# The parity test in nros-cli-core (`capability_refusal_parity`) runs ONE table
# through both. This script is how it asks cmake: it hands
# `nros_capability_features` one row, with the backend facts the CLI exports
# (`nros dev rmw dispatch` rows, passed in, never re-read from a descriptor),
# and prints ACCEPTED, or stops with the function's own FATAL_ERROR.
#
# Usage:
#   cmake -DRMW=<name> -DCAPS=<a;b> -DRMW_CAPABILITIES=<a;b> -DRMW_FACTS=<k=ON;...> \
#         -P scripts/check/capability-refusal-parity.cmake

cmake_minimum_required(VERSION 3.22)
get_filename_component(_root "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
include("${_root}/cmake/NanoRosFeatureSet.cmake")

foreach(_v RMW CAPS RMW_CAPABILITIES RMW_FACTS)
    if(NOT DEFINED ${_v})
        message(FATAL_ERROR "capability-refusal-parity: -D${_v}=... is required")
    endif()
endforeach()

# What `nros_rmw_dispatch()` sets from the CLI's rows.
set(NROS_RMW_CAPABILITIES "${RMW_CAPABILITIES}")
set(NROS_RMW_FACTS "${RMW_FACTS}")

nros_capability_features(_feats RMW "${RMW}" CAPABILITIES ${CAPS})
message(STATUS "ACCEPTED ${_feats}")
