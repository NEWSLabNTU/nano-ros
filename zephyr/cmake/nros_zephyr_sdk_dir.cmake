# nano-ros -- the caller's `ZEPHYR_SDK_INSTALL_DIR`, checked before Zephyr
# reads it (issue 3004).
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Zephyr's `FindZephyr-sdk.cmake` hands this variable to `find_package` as a
# HINT. A value that names no SDK root -- most often the store's
# `<version>/` directory, one level above `zephyr-sdk-<version>/` -- is not an
# error there: the search goes on to the CMake package registry and takes
# whichever SDK it holds, silently. That is how a build asked for SDK 0.16.9
# configured with 0.16.8.
#
# `zephyr/modules/modules.cmake` calls this after module discovery and before
# `dts.cmake` looks the SDK up, so it reaches a plain `west build` as well as
# `nros build`. The rule is the one the CLI (`builder::zephyr::callers_sdk`)
# and `scripts/lib/zephyr-sdk.sh` (`nros_zephyr_sdk_callers`) apply:
#   1. the value is an SDK root (holds `sdk_version`, or is named
#      `zephyr-sdk-*`): left alone;
#   2. `<value>/zephyr-sdk-<pinned>` exists: that;
#   3. exactly one `zephyr-sdk-*` child holds `sdk_version`: that;
#   otherwise the configure stops, naming the directory to set.
# A resolved value replaces the one Zephyr reads, with a status line.

function(nros_zephyr_sdk_dir_check)
    set(_from "")
    if(DEFINED CACHE{ZEPHYR_SDK_INSTALL_DIR})
        set(_dir "$CACHE{ZEPHYR_SDK_INSTALL_DIR}")
        set(_from cache)
    elseif(DEFINED ENV{ZEPHYR_SDK_INSTALL_DIR})
        set(_dir "$ENV{ZEPHYR_SDK_INSTALL_DIR}")
        set(_from env)
    endif()
    if(_from STREQUAL "" OR "${_dir}" STREQUAL "")
        return()
    endif()
    string(REGEX REPLACE "/+$" "" _dir "${_dir}")
    get_filename_component(_name "${_dir}" NAME)
    if(EXISTS "${_dir}/sdk_version" OR (IS_DIRECTORY "${_dir}" AND _name MATCHES "^zephyr-sdk-"))
        return()
    endif()

    set(_want "")
    if(DEFINED ZEPHYR_BASE AND EXISTS "${ZEPHYR_BASE}/SDK_VERSION")
        file(STRINGS "${ZEPHYR_BASE}/SDK_VERSION" _want LIMIT_COUNT 1)
        string(STRIP "${_want}" _want)
    endif()

    set(_root "")
    if(NOT _want STREQUAL "" AND IS_DIRECTORY "${_dir}/zephyr-sdk-${_want}")
        set(_root "${_dir}/zephyr-sdk-${_want}")
    else()
        file(GLOB _cands LIST_DIRECTORIES true "${_dir}/zephyr-sdk-*")
        set(_roots "")
        foreach(_c IN LISTS _cands)
            if(EXISTS "${_c}/sdk_version")
                list(APPEND _roots "${_c}")
            endif()
        endforeach()
        list(LENGTH _roots _n)
        if(_n EQUAL 1)
            set(_root "${_roots}")
        endif()
    endif()

    if(_root STREQUAL "")
        if(NOT IS_DIRECTORY "${_dir}")
            set(_why "it does not exist")
        elseif(_n EQUAL 0)
            set(_why "it holds no `sdk_version`, and no `zephyr-sdk-*` directory below it does")
        else()
            string(REPLACE ";" ", " _list "${_roots}")
            set(_why "it holds no `sdk_version`, and ${_n} `zephyr-sdk-*` directories below it do (${_list})")
        endif()
        message(FATAL_ERROR
            "nros: ZEPHYR_SDK_INSTALL_DIR=${_dir} names no Zephyr SDK: ${_why}. "
            "Refusing rather than letting the build take whichever SDK the CMake "
            "registry holds.\n"
            "  Set ZEPHYR_SDK_INSTALL_DIR to the SDK root itself, the directory holding "
            "`sdk_version` (`nros locate tool zephyr-sdk` prints the pinned one), or "
            "unset it.")
    endif()

    message(STATUS
        "nros: ZEPHYR_SDK_INSTALL_DIR=${_dir} is the directory above the SDK root; "
        "using ${_root}")
    if(_from STREQUAL "cache")
        set(ZEPHYR_SDK_INSTALL_DIR "${_root}" CACHE PATH "Zephyr SDK install directory" FORCE)
    else()
        set(ENV{ZEPHYR_SDK_INSTALL_DIR} "${_root}")
    endif()
endfunction()
