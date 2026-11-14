# NanoRosBundledInterfaces.cmake — issue 2278.
#
# Loaded by NanoRosCodegenCore.cmake (so by both interface generators) and by
# the find modules' resolver (`find/_NrosFindRosMsgPackage.cmake`), which runs
# before either generator is loaded. Functions only.
include_guard(GLOBAL)

# --- the bundled interfaces are an opt-in (issue 2278) -----------------------
#
# nano-ros bundles a copy of the common message packages (`std_msgs`,
# `builtin_interfaces`, ...): at `share/nano-ros/interfaces/` in an install and
# at `packages/cli/interfaces/` in a checkout. `nros configure` refuses to
# generate from them in place of the host's ROS install, because a bundled copy
# silently picks a distro, and a ROS peer built from another one disagrees on
# message layout and type hash (issue 2275). The CMake road used to fall back to
# them on any host without ROS, so a west or `add_subdirectory` app (which never
# runs configure) built what configure refuses. Now it refuses too: a package
# that neither NROS_INTERFACE_SEARCH_PATH nor AMENT_PREFIX_PATH names is not
# generated from the bundled copy unless the search path names the bundled root.
# The bundled root keeps its old place after AMENT_PREFIX_PATH, so opting in
# changes nothing else.

# _nros_bundled_interface_roots(<prefix> <out_var>) — the two directories
# nano-ros ships its bundled interface packages in under <prefix>.
function(_nros_bundled_interface_roots prefix out_var)
    set(_roots "")
    if(NOT prefix STREQUAL "")
        foreach(_c "${prefix}/share/nano-ros/interfaces" "${prefix}/packages/cli/interfaces")
            if(IS_DIRECTORY "${_c}")
                file(REAL_PATH "${_c}" _r)
                list(APPEND _roots "${_r}")
            endif()
        endforeach()
    endif()
    set(${out_var} "${_roots}" PARENT_SCOPE)
endfunction()

# _nros_is_bundled_interface_root(<dir> <prefix> <out_var>) — TRUE when <dir>
# is one of <prefix>'s bundled roots.
function(_nros_is_bundled_interface_root dir prefix out_var)
    set(${out_var} FALSE PARENT_SCOPE)
    if(dir STREQUAL "" OR NOT IS_DIRECTORY "${dir}")
        return()
    endif()
    _nros_bundled_interface_roots("${prefix}" _roots)
    file(REAL_PATH "${dir}" _d)
    if(_d IN_LIST _roots)
        set(${out_var} TRUE PARENT_SCOPE)
    endif()
endfunction()

# _nros_bundled_interfaces_opted_in(<prefix> <out_var>) — TRUE when
# NROS_INTERFACE_SEARCH_PATH (the cmake variable or the environment) names one
# of <prefix>'s bundled roots.
function(_nros_bundled_interfaces_opted_in prefix out_var)
    set(_entries "")
    if(DEFINED NROS_INTERFACE_SEARCH_PATH AND NOT NROS_INTERFACE_SEARCH_PATH STREQUAL "")
        string(REPLACE ":" ";" _e "${NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _entries ${_e})
    endif()
    if(DEFINED ENV{NROS_INTERFACE_SEARCH_PATH} AND NOT "$ENV{NROS_INTERFACE_SEARCH_PATH}" STREQUAL "")
        string(REPLACE ":" ";" _e "$ENV{NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _entries ${_e})
    endif()
    foreach(_entry IN LISTS _entries)
        _nros_is_bundled_interface_root("${_entry}" "${prefix}" _is)
        if(_is)
            set(${out_var} TRUE PARENT_SCOPE)
            return()
        endif()
    endforeach()
    set(${out_var} FALSE PARENT_SCOPE)
endfunction()

# _nros_refuse_bundled_interfaces(<pkg> <prefix> <files>...) — FATAL_ERROR when
# any of <files> lies under one of <prefix>'s bundled roots and the build has
# not opted in. The remedy is configure's (`cmd/configure.rs`,
# `recorded_env_to_restore_with`), so both roads give one answer.
function(_nros_refuse_bundled_interfaces pkg prefix)
    _nros_bundled_interface_roots("${prefix}" _roots)
    if(NOT _roots)
        return()
    endif()
    set(_hit "")
    foreach(_f IN LISTS ARGN)
        if(NOT EXISTS "${_f}")
            continue()
        endif()
        file(REAL_PATH "${_f}" _rf)
        foreach(_root IN LISTS _roots)
            string(FIND "${_rf}" "${_root}/" _at)
            if(_at EQUAL 0)
                set(_hit "${_root}")
                break()
            endif()
        endforeach()
        if(_hit)
            break()
        endif()
    endforeach()
    if(NOT _hit)
        return()
    endif()
    _nros_bundled_interfaces_opted_in("${prefix}" _opted)
    if(_opted)
        return()
    endif()
    list(GET _roots 0 _shown)
    # issue 2278.
    message(FATAL_ERROR
        "nano-ros: the message package `${pkg}` is on neither NROS_INTERFACE_SEARCH_PATH "
        "nor AMENT_PREFIX_PATH, and nano-ros does not generate it from its bundled copy "
        "in place of your ROS install. Source ROS 2 in this shell "
        "(`source /opt/ros/<distro>/setup.bash`, or your overlay's `install/setup.bash`) "
        "and configure again. To build against the bundled copy instead, name it on the "
        "search path: NROS_INTERFACE_SEARCH_PATH=${_shown}")
endfunction()
