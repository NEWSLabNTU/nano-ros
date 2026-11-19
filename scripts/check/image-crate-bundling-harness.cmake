# The cmake half of `check-image-crate-bundling-agreement` (issue 2726).
#
# Runs `nros_image_backend()` from cmake/NanoRosImageCrate.cmake for each RMW
# in RMWS, with the backend's facts from the real dispatch query
# (`nros_rmw_dispatch`, through the CLI the caller names), and prints one
# `<rmw><TAB>ON|OFF` line per RMW: whether the image crate bundles it.
#
#   cmake -D_NANO_ROS_CODEGEN_TOOL=<nros> "-DRMWS=a;b" \
#         [-DMUTATE_RMW=<rmw> -DMUTATE=drop-register|flip-strategy|force-bundle] \
#         -P scripts/check/image-crate-bundling-harness.cmake
#
# MUTATE is the gate's negative control: it edits one RMW's dispatch rows
# after the query, before the decision, so the gate can show it goes red.
cmake_minimum_required(VERSION 3.22)

get_filename_component(_harness_repo "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
include("${_harness_repo}/cmake/NanoRosImageCrate.cmake")

if(DEFINED MUTATE_RMW)
    # A redefinition keeps the original reachable as `_nros_rmw_dispatch`.
    function(nros_rmw_dispatch rmw)
        _nros_rmw_dispatch("${rmw}")
        if(rmw STREQUAL MUTATE_RMW)
            if(MUTATE STREQUAL "drop-register")
                set(NROS_RMW_REGISTER_CRATE "")
            elseif(MUTATE STREQUAL "flip-strategy")
                if(NROS_RMW_LINK_STRATEGY STREQUAL "umbrella")
                    set(NROS_RMW_LINK_STRATEGY "cmake")
                else()
                    set(NROS_RMW_LINK_STRATEGY "umbrella")
                endif()
            elseif(MUTATE STREQUAL "force-bundle")
                set(NROS_RMW_LINK_STRATEGY "umbrella")
                if(NOT NROS_RMW_REGISTER_CRATE)
                    set(NROS_RMW_REGISTER_CRATE "harness-register")
                    set(NROS_RMW_REGISTER_FN "register")
                endif()
            else()
                message(FATAL_ERROR "image-crate-bundling-harness: unknown MUTATE '${MUTATE}'")
            endif()
        endif()
        foreach(_v IN LISTS _NROS_RMW_DISPATCH_VARS)
            set(${_v} "${${_v}}" PARENT_SCOPE)
        endforeach()
    endfunction()
endif()

foreach(_rmw IN LISTS RMWS)
    nros_image_backend(_b RMW "${_rmw}" API_FEATURES rmw-cffi)
    if(_b_BUNDLED)
        set(_on ON)
    else()
        set(_on OFF)
    endif()
    execute_process(COMMAND "${CMAKE_COMMAND}" -E echo "${_rmw}\t${_on}")
endforeach()
