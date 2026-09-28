# Issue 1550 -- the ROS domain on the ENTRY road, and where it came from.
#
# WHY THIS EXISTS
#
# A Zephyr image's session takes `CONFIG_NROS_DOMAIN_ID` (Kconfig, RFC-0049's
# single writer). Two other places state a domain for the same image and
# nothing compared them on the road most images take:
#
#   * the entry package's `system.toml` (`[image] domain_id` > `[system]
#     domain_id`), which `nano_ros_read_leaf_system()` reads and PRINTS
#     (`deployment from .../system.toml ... domain_id=10`) and then compares
#     with nothing;
#   * the transport snippet (`-S <snippet>`), which is where an image that
#     varies its link also tends to state its domain -- and where a snippet
#     that forgets to is invisible: the Autoware Safety Island's
#     `island-ethernet` set none, so its Ethernet board image would have
#     joined domain 0 while every document said 10.
#
# `nros_system_check_domain_agreement` (phase-460 W4, issue 1423) already
# refuses a disagreement, but only inside `nros_system_generate`, and the
# island's entries use `nano_ros_add_executable`. This is the same refusal on
# that road, with the snippet as a third witness, and it changes no
# precedence: Kconfig stays what the image bakes, and `system.toml` gains no
# authority over it.
#
# WHAT IT READS
#
# `merge_config_files` -- the list Zephyr's `cmake/modules/kconfig.cmake`
# merged into `.config`, in merge order, as absolute paths. It is set at the
# application's top level by `find_package(Zephyr)`, so it is in scope for the
# entry verbs and for the module alike. A fragment is classified by where it
# lives:
#
#   command-line  Zephyr's `extra_kconfig_options.conf`, i.e. a
#                 `-DCONFIG_NROS_DOMAIN_ID=<n>` on the cmake / west line
#   snippet       a fragment beside a `snippet.yml`
#   kconfig       any other fragment (a board `.conf`, `prj.conf`, an extra)
#
# and the LAST fragment that states the symbol is the one Kconfig kept.
# None stating it is `default`: the Kconfig default, 0.
include_guard(GLOBAL)

# _nros_domain_fragment_kind(<path> <out-kind> <out-label>)
function(_nros_domain_fragment_kind path out_kind out_label)
    get_filename_component(_dir "${path}" DIRECTORY)
    if((DEFINED EXTRA_KCONFIG_OPTIONS_FILE AND path STREQUAL EXTRA_KCONFIG_OPTIONS_FILE)
       OR path MATCHES "/misc/generated/extra_kconfig_options\\.conf$")
        set(${out_kind} command-line PARENT_SCOPE)
        set(${out_label} "-DCONFIG_NROS_DOMAIN_ID on the cmake/west command line"
            PARENT_SCOPE)
        return()
    endif()
    if(EXISTS "${_dir}/snippet.yml")
        # The snippet's NAME is what `-S` takes; read it rather than assume the
        # directory is called the same.
        get_filename_component(_name "${_dir}" NAME)
        file(STRINGS "${_dir}/snippet.yml" _nl REGEX "^name:[ \t]*")
        if(_nl)
            list(GET _nl 0 _nl)
            string(REGEX REPLACE "^name:[ \t]*([^ \t#]+).*$" "\\1" _name "${_nl}")
        endif()
        set(${out_kind} snippet PARENT_SCOPE)
        set(${out_label} "snippet ${_name} (${path})" PARENT_SCOPE)
        return()
    endif()
    set(${out_kind} kconfig PARENT_SCOPE)
    set(${out_label} "${path}" PARENT_SCOPE)
endfunction()

# nros_domain_statements(<out-list>)
#
# Every merged fragment that states CONFIG_NROS_DOMAIN_ID, in merge order, as
# `<kind>|<value>|<label>` rows. Empty when nothing states it, and empty (not
# an error) when `merge_config_files` is not in scope -- a caller outside a
# Zephyr configure has no fragments to report.
function(nros_domain_statements out)
    set(_rows "")
    foreach(_f IN LISTS merge_config_files)
        if(NOT EXISTS "${_f}" OR IS_DIRECTORY "${_f}")
            continue()
        endif()
        file(STRINGS "${_f}" _lines REGEX "^[ \t]*CONFIG_NROS_DOMAIN_ID[ \t]*=")
        foreach(_l IN LISTS _lines)
            string(REGEX REPLACE
                "^[ \t]*CONFIG_NROS_DOMAIN_ID[ \t]*=[ \t]*\"?([0-9]*)\"?.*$" "\\1"
                _v "${_l}")
            _nros_domain_fragment_kind("${_f}" _kind _label)
            list(APPEND _rows "${_kind}|${_v}|${_label}")
        endforeach()
    endforeach()
    set(${out} "${_rows}" PARENT_SCOPE)
endfunction()

# nros_domain_provenance(<out-source> <out-label>)
#
# Which rung decided CONFIG_NROS_DOMAIN_ID: `default`, `kconfig`, `snippet` or
# `command-line` -- the spellings `NROS_KNOB_SOURCE_NROS_DOMAIN_ID` carries to
# `nros-node/build.rs` for the boot record -- and a label naming the fragment.
function(nros_domain_provenance out_source out_label)
    nros_domain_statements(_rows)
    if(NOT _rows)
        set(${out_source} default PARENT_SCOPE)
        set(${out_label} "the Kconfig default -- no merged fragment states it"
            PARENT_SCOPE)
        return()
    endif()
    list(GET _rows -1 _last)
    string(REPLACE "|" ";" _last "${_last}")
    list(GET _last 0 _kind)
    list(GET _last 2 _label)
    set(${out_source} "${_kind}" PARENT_SCOPE)
    set(${out_label} "${_label}" PARENT_SCOPE)
endfunction()

# nros_check_domain_agreement([SYSTEM_DOMAIN <n>] [SYSTEM_FILE <path>]
#                             [CONTEXT <text>])
#
# REFUSES when the image's domain, the `system.toml` domain (when one is
# given) and any merged fragment that states the symbol -- the snippet above
# all -- do not all agree, naming the three values. A fragment whose statement
# a later one overrode is a disagreement too: it is the image saying two
# things, and the one it did not mean is the one a reader of that file sees.
function(nros_check_domain_agreement)
    cmake_parse_arguments(_D "" "SYSTEM_DOMAIN;SYSTEM_FILE;CONTEXT" "" ${ARGN})
    if(NOT _D_CONTEXT)
        set(_D_CONTEXT "nano-ros")
    endif()
    if(NOT DEFINED CONFIG_NROS_DOMAIN_ID)
        message(WARNING
            "${_D_CONTEXT}: CONFIG_NROS_DOMAIN_ID is not in scope, so the "
            "image's domain was compared against nothing. Call this after "
            "find_package(Zephyr) (issue 1550).")
        return()
    endif()
    set(_k "${CONFIG_NROS_DOMAIN_ID}")
    nros_domain_statements(_rows)
    nros_domain_provenance(_src _src_label)

    set(_agree TRUE)
    if(NOT "${_D_SYSTEM_DOMAIN}" STREQUAL "" AND NOT _D_SYSTEM_DOMAIN EQUAL _k)
        set(_agree FALSE)
    endif()

    set(_snippet_lines "")
    set(_other_lines "")
    foreach(_r IN LISTS _rows)
        string(REPLACE "|" ";" _r "${_r}")
        list(GET _r 0 _kind)
        list(GET _r 1 _v)
        list(GET _r 2 _label)
        if(NOT _v EQUAL _k)
            set(_agree FALSE)
        endif()
        if(_kind STREQUAL "snippet")
            string(APPEND _snippet_lines
                "\n  snippet               = ${_v}  (${_label})")
        elseif(NOT _v EQUAL _k)
            string(APPEND _other_lines
                "\n  also stated           = ${_v}  (${_label}, overridden)")
        endif()
    endforeach()
    if(_snippet_lines STREQUAL "")
        set(_active "${SNIPPET}")
        string(REPLACE ";" ", " _active "${_active}")
        if(_active STREQUAL "")
            set(_active "none")
        endif()
        set(_snippet_lines
            "\n  snippet               = (not stated; active snippets: ${_active})")
    endif()
    if("${_D_SYSTEM_DOMAIN}" STREQUAL "")
        set(_sys_line "\n  system.toml domain_id = (not stated)")
    else()
        set(_sys_line
            "\n  system.toml domain_id = ${_D_SYSTEM_DOMAIN}  (${_D_SYSTEM_FILE})")
    endif()

    if(NOT _agree)
        # ONE string with no blank lines: cmake indents every line of a
        # FATAL_ERROR, so an empty one renders as stray whitespace.
        message(FATAL_ERROR
            "${_D_CONTEXT}: this image's ROS domain is stated more than once and "
            "the statements disagree:"
            "\n  CONFIG_NROS_DOMAIN_ID = ${_k}  (what the image bakes; from ${_src_label})"
            "${_sys_line}${_snippet_lines}${_other_lines}"
            "\nThe image runs on ${_k}; a peer on any other domain never sees it, "
            "and nothing at run time says why. Kconfig is what the image bakes "
            "(RFC-0049): state `CONFIG_NROS_DOMAIN_ID=<n>` once, in the fragment "
            "every transport of this image merges (the board `.conf`, or each "
            "transport snippet), and make system.toml's `domain_id` say the same "
            "(issue 1550).")
    endif()
    if("${_D_SYSTEM_DOMAIN}" STREQUAL "")
        set(_sys_word "no system.toml domain_id")
    else()
        set(_sys_word "system.toml ${_D_SYSTEM_DOMAIN}")
    endif()
    message(STATUS
        "${_D_CONTEXT}: domain ${_k} agrees -- CONFIG_NROS_DOMAIN_ID from "
        "${_src_label}; ${_sys_word}")
endfunction()
