# integrations/s32ds/cmake/S32dsProject.cmake
#
# RFC-0062 — probe an NXP S32 Design Studio (Eclipse CDT) project and recover
# the facts nano-ros needs to build objects that will link into it.
#
# WHY PROBE INSTEAD OF ASKING THE USER
#
# nano-ros compiles its own translation units (the `nros-platform-freertos`
# shim, lwIP, the generated node code) OUTSIDE S32DS, and those objects are
# then linked into the S32DS image. Every ABI-affecting flag must match the
# project EXACTLY or the link fails — or worse, silently produces a broken
# image. For MR-CANHUBK3 that set is:
#
#     -mcpu=cortex-m7 -mthumb -mlittle-endian -mfloat-abi=hard -mfpu=fpv5-sp-d16
#
# Note `fpv5-sp-d16` (single precision), not the `fpv5-d16` one would guess.
# Asking the user to retype these is a defect generator; CDT already wrote them
# down, once per translation unit, in `<config>/**/<tu>.args` response files.
# Every one of the 10 in the reference project carries an identical `-m*` set.
#
# HOST PORTABILITY
#
# A CDT project generated on Windows carries absolute `C:/...` include paths and
# a `--sysroot=C:/...`. These fall into two groups, and they must be treated
# DIFFERENTLY — an earlier version of this file dropped both and the build then
# failed on `FreeRTOSConfig.h: No such file or directory`, because that header
# lives in `generate/include`, which the project references ONLY by its Windows
# absolute path:
#
#   inside the project   C:/Users/.../<project>/generate/include
#                        C:/Users/.../<project>/board
#                        → RE-ROOTED onto the local project directory.
#   outside the project  C:/NXP/S32DS.3.4/.../PlatformSDK_S32K3_.../include
#                        → dropped with a warning; genuinely needs the S32DS
#                          install. Supply via NROS_S32DS_EXTRA_INCLUDES.
#
# Only `RTD/include` happens to appear in both relative and absolute form; the
# rest do not, so "drop all Windows paths" loses real include directories.
# `--sysroot=` is always dropped — the local cross toolchain supplies its own.

if(DEFINED _NROS_S32DS_PROJECT_INCLUDED)
    return()
endif()
set(_NROS_S32DS_PROJECT_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# nros_s32ds_probe(PROJECT <dir> [CONFIG <name>])
#
# Sets in the CALLER's scope:
#   NROS_S32DS_ABI_FLAGS      list — the `-m*` flags every object must share
#   NROS_S32DS_SPECS          e.g. `-specs=rdimon.specs` ("" if none)
#   NROS_S32DS_INCLUDE_DIRS   host-portable include dirs, absolute
#   NROS_S32DS_FREERTOS_DIR   <project>/FreeRTOS/Source, if present
#   NROS_S32DS_FREERTOS_PORT  e.g. GCC/ARM_CM7/r0p1
#   NROS_S32DS_LINKER_SCRIPT  the flash linker script, if exactly one matches
#   NROS_S32DS_CONFIG         the build config actually probed
# ---------------------------------------------------------------------------
function(nros_s32ds_probe)
    cmake_parse_arguments(_P "" "PROJECT;CONFIG" "" ${ARGN})

    if(NOT _P_PROJECT)
        message(FATAL_ERROR
            "nros_s32ds_probe: PROJECT <dir> is required — the S32DS project "
            "root (the directory holding .cproject and Debug_FLASH/).")
    endif()
    get_filename_component(_proj "${_P_PROJECT}" ABSOLUTE)
    # Used to recognise Windows absolute paths that point back INTO this
    # project, so they can be re-rooted rather than lost.
    get_filename_component(_proj_name "${_proj}" NAME)
    if(NOT EXISTS "${_proj}/.cproject")
        message(FATAL_ERROR
            "nros_s32ds_probe: '${_proj}' has no .cproject — not an S32DS "
            "project root.")
    endif()

    # --- pick a build configuration -----------------------------------------
    # Prefer the caller's, else the first directory containing `.args` files.
    set(_cfg "${_P_CONFIG}")
    if(NOT _cfg)
        file(GLOB _cfg_dirs RELATIVE "${_proj}" "${_proj}/*")
        foreach(_d IN LISTS _cfg_dirs)
            if(IS_DIRECTORY "${_proj}/${_d}")
                file(GLOB_RECURSE _probe "${_proj}/${_d}/*.args")
                if(_probe)
                    set(_cfg "${_d}")
                    break()
                endif()
            endif()
        endforeach()
    endif()
    if(NOT _cfg OR NOT IS_DIRECTORY "${_proj}/${_cfg}")
        message(FATAL_ERROR
            "nros_s32ds_probe: no build configuration with CDT `.args` files "
            "under '${_proj}'. Build the project once in S32DS (or run "
            "`make -C <config>`) so CDT emits them, then re-run.")
    endif()

    # --- choose a COMPILE args file, never the link one ----------------------
    # The link response file sits at `<config>/<project>.args`; per-TU compile
    # files live in subdirectories. Only compile files carry the include set.
    file(GLOB_RECURSE _args_files "${_proj}/${_cfg}/*.args")
    set(_compile_args "")
    foreach(_f IN LISTS _args_files)
        get_filename_component(_dir "${_f}" DIRECTORY)
        if(NOT "${_dir}" STREQUAL "${_proj}/${_cfg}")
            set(_compile_args "${_f}")
            break()
        endif()
    endforeach()
    if(NOT _compile_args)
        message(FATAL_ERROR
            "nros_s32ds_probe: found no per-source `.args` under "
            "'${_proj}/${_cfg}' (only a link response file, if any).")
    endif()

    # --- parse ---------------------------------------------------------------
    file(STRINGS "${_compile_args}" _lines)
    set(_abi "")
    set(_specs "")
    set(_includes "")
    set(_port "")
    set(_dropped "")

    foreach(_line IN LISTS _lines)
        string(STRIP "${_line}" _line)
        if(_line STREQUAL "")
            continue()
        endif()

        # `--sysroot=C:/...` — Windows-only, and the cross toolchain on this
        # host supplies its own. Always dropped.
        if(_line MATCHES "^--sysroot")
            continue()
        endif()

        if(_line MATCHES "^-m")
            list(APPEND _abi "${_line}")
        elseif(_line MATCHES "^-specs=")
            set(_specs "${_line}")
        elseif(_line MATCHES "^-I")
            # Strip `-I` and any surrounding quotes.
            string(REGEX REPLACE "^-I" "" _inc "${_line}")
            string(REGEX REPLACE "^\"(.*)\"$" "\\1" _inc "${_inc}")
            # Windows absolute path: re-root it onto the local project if it
            # points INSIDE the project, else drop it. See the header note —
            # `generate/include` (which holds FreeRTOSConfig.h) exists ONLY in
            # this form, so blanket-dropping breaks the build.
            if(_inc MATCHES "^[A-Za-z]:[/\\\\]")
                string(REPLACE "\\" "/" _inc "${_inc}")
                if(_inc MATCHES "/${_proj_name}/(.+)$")
                    set(_inc "${_proj}/${CMAKE_MATCH_1}")
                else()
                    list(APPEND _dropped "${_inc}")
                    continue()
                endif()
            endif()
            # CDT emits relative paths against the CONFIG dir, not the project.
            if(NOT IS_ABSOLUTE "${_inc}")
                get_filename_component(_inc "${_proj}/${_cfg}/${_inc}" ABSOLUTE)
            endif()
            if(IS_DIRECTORY "${_inc}")
                list(APPEND _includes "${_inc}")
            endif()
            # Recover the FreeRTOS port from the portable-layer include.
            if(_inc MATCHES "/portable/(GCC/[^\"]+)$")
                set(_port "${CMAKE_MATCH_1}")
            endif()
        endif()
    endforeach()

    if(NOT _abi)
        message(FATAL_ERROR
            "nros_s32ds_probe: no `-m*` ABI flags in '${_compile_args}'. "
            "nano-ros objects cannot be guaranteed link-compatible; refusing "
            "to guess.")
    endif()
    list(REMOVE_DUPLICATES _includes)

    # Paths outside the project — the S32DS install's PlatformSDK RTD headers.
    # Not silently swallowed: if a compile later fails on a missing vendor
    # header, this is the list to satisfy via NROS_S32DS_EXTRA_INCLUDES.
    if(_dropped)
        list(REMOVE_DUPLICATES _dropped)
        list(LENGTH _dropped _dropped_count)
        message(STATUS
            "nros-s32ds: dropped ${_dropped_count} include path(s) outside the "
            "project (S32DS install). If a vendor header goes missing, pass "
            "them via -DNROS_S32DS_EXTRA_INCLUDES:")
        foreach(_d IN LISTS _dropped)
            message(STATUS "nros-s32ds:   dropped ${_d}")
        endforeach()
    endif()

    # --- FreeRTOS + linker script -------------------------------------------
    set(_frt "")
    if(IS_DIRECTORY "${_proj}/FreeRTOS/Source/include")
        set(_frt "${_proj}/FreeRTOS/Source")
    endif()

    set(_ld "")
    file(GLOB _lds "${_proj}/Project_Settings/Linker_Files/*flash*.ld")
    list(LENGTH _lds _ld_count)
    if(_ld_count EQUAL 1)
        list(GET _lds 0 _ld)
    endif()

    # --- publish -------------------------------------------------------------
    set(NROS_S32DS_CONFIG          "${_cfg}"      PARENT_SCOPE)
    set(NROS_S32DS_ABI_FLAGS       "${_abi}"      PARENT_SCOPE)
    set(NROS_S32DS_SPECS           "${_specs}"    PARENT_SCOPE)
    set(NROS_S32DS_INCLUDE_DIRS    "${_includes}" PARENT_SCOPE)
    set(NROS_S32DS_FREERTOS_DIR    "${_frt}"      PARENT_SCOPE)
    set(NROS_S32DS_FREERTOS_PORT   "${_port}"     PARENT_SCOPE)
    set(NROS_S32DS_LINKER_SCRIPT   "${_ld}"       PARENT_SCOPE)

    message(STATUS "nros-s32ds: project   ${_proj}")
    message(STATUS "nros-s32ds: config    ${_cfg} (probed ${_compile_args})")
    message(STATUS "nros-s32ds: ABI       ${_abi}")
    message(STATUS "nros-s32ds: FreeRTOS  ${_frt} port=${_port}")
endfunction()
