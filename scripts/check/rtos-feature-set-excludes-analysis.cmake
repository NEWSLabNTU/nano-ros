# phase-463 W5 I3(a) (issue 1419) -- no RTOS umbrella resolves an analysis mode.
#
# The census binary IS the native boot binary (phase-463 W2), so `metadata-mode`
# is in the NATIVE C and C++ umbrellas' feature sets and must be in no other
# (issue 1556 items 2-3 gave `nros-c` the feature and the C census switch). An RTOS
# image that carried it would link the recorder, the recording backend and the
# census FFI into firmware -- bytes on a board for a mode that cannot run there
# (the switch is read through `env`, which no RTOS board has). Issue 0304 is the
# precedent for checking a feature set rather than assuming it: a
# `metadata-mode` once reached `nros-c` before that crate had the feature.
#
# So this ASKS the one function that assembles every umbrella's features,
# `nros_feature_set` in cmake/NanoRosFeatureSet.cmake, for every crate x
# platform x cross combination it accepts, and holds the answer to one rule:
#
#   `metadata-mode` iff CRATE (c OR cpp) AND PLATFORM posix AND NOT cross;
#   `profile-mode`  never (phase-463 W7 has not landed; when it does, the same
#                   rule applies and this line moves with it).
#
# Reading the function's OUTPUT, not its source: a grep for the `if()` guarding
# the append would pass a rewrite that reaches the list another way.
#
# The negative control runs on every invocation (phase-395): the rule itself is
# handed a planted RTOS feature list carrying `metadata-mode` and must refuse it.
#
# Usage: cmake -P scripts/check/rtos-feature-set-excludes-analysis.cmake

cmake_minimum_required(VERSION 3.22)
get_filename_component(_root "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
include("${_root}/cmake/NanoRosFeatureSet.cmake")

set(_platforms posix freertos freertos_armcm3 nuttx nuttx_armv7a threadx threadx_linux
    threadx_riscv64)

# Returns, in `out`, an error line or "" for one evaluated feature list.
function(_analysis_violation out crate platform cross feats)
    set(_want_metadata FALSE)
    if((crate STREQUAL "cpp" OR crate STREQUAL "c") AND platform STREQUAL "posix"
       AND NOT cross)
        set(_want_metadata TRUE)
    endif()
    set(_has_metadata FALSE)
    if("metadata-mode" IN_LIST feats)
        set(_has_metadata TRUE)
    endif()
    set(_msg "")
    if(_has_metadata AND NOT _want_metadata)
        set(_msg "CRATE ${crate} PLATFORM ${platform} cross=${cross}: resolves `metadata-mode` -- an RTOS umbrella carries the census recorder")
    elseif(_want_metadata AND NOT _has_metadata)
        set(_msg "CRATE ${crate} PLATFORM ${platform} cross=${cross}: a NATIVE C/C++ umbrella lost `metadata-mode` -- its entry can no longer be the census producer")
    elseif("profile-mode" IN_LIST feats)
        set(_msg "CRATE ${crate} PLATFORM ${platform} cross=${cross}: resolves `profile-mode`")
    endif()
    set(${out} "${_msg}" PARENT_SCOPE)
endfunction()

# ---- negative control -------------------------------------------------------
_analysis_violation(_planted cpp freertos TRUE "alloc;platform-freertos;metadata-mode")
if(_planted STREQUAL "")
    message(FATAL_ERROR "rtos-feature-set-excludes-analysis: self-test -- a planted "
        "`metadata-mode` in a FreeRTOS feature set was not refused, so this gate refuses nothing")
endif()
_analysis_violation(_planted cpp posix FALSE "std;platform-posix")
if(_planted STREQUAL "")
    message(FATAL_ERROR "rtos-feature-set-excludes-analysis: self-test -- a native C++ set "
        "WITHOUT `metadata-mode` was not refused")
endif()
# Issue 1556 -- and the native C set, which is a census producer now too.
_analysis_violation(_planted c posix FALSE "std;platform-posix")
if(_planted STREQUAL "")
    message(FATAL_ERROR "rtos-feature-set-excludes-analysis: self-test -- a native C set "
        "WITHOUT `metadata-mode` was not refused")
endif()

# ---- the real ladder --------------------------------------------------------
set(_failures "")
set(_n 0)
foreach(_crate c cpp)
    foreach(_platform IN LISTS _platforms)
        foreach(_cross FALSE TRUE)
            if(_cross)
                nros_feature_set(_feats CRATE ${_crate} RMW none PLATFORM ${_platform}
                    NO_STD_CROSS)
            else()
                nros_feature_set(_feats CRATE ${_crate} RMW none PLATFORM ${_platform})
            endif()
            _analysis_violation(_v ${_crate} ${_platform} ${_cross} "${_feats}")
            if(NOT _v STREQUAL "")
                list(APPEND _failures "${_v} (features: ${_feats})")
            endif()
            math(EXPR _n "${_n} + 1")
        endforeach()
    endforeach()
endforeach()

if(_failures)
    list(JOIN _failures "\n  " _lines)
    message(FATAL_ERROR "rtos-feature-set-excludes-analysis: FAIL (phase-463 W5 I3a)\n  ${_lines}\n"
        "Only the NATIVE C and C++ umbrellas may carry `metadata-mode`; see the analysis block in "
        "cmake/NanoRosFeatureSet.cmake.")
endif()
message(STATUS "rtos-feature-set-excludes-analysis: OK -- ${_n} umbrella feature sets; "
    "`metadata-mode` only in c|cpp/posix/native, `profile-mode` in none")
