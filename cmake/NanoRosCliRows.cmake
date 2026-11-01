# NanoRosCliRows.cmake -- `nros_cli_rows`, the ONE way a configure reads the
# `nros` CLI's line-oriented stdout (issue 2200).
#
# THE DEFECT CLASS
#
# Every reader used to spell it
#
#     string(REPLACE "\n" ";" _lines "${_out}")
#     foreach(_l IN LISTS _lines)
#
# which treats the CLI's output as a cmake LIST. A list is not a sequence of
# lines, and the difference is silent:
#
#   * a `;` inside a line SPLITS it. `NROS_LEAF_CONF=<board>/prj.conf;<leaf>/
#     prj-serial.conf` became the row `NROS_LEAF_CONF=<board>/prj.conf` plus a
#     bare `<leaf>/prj-serial.conf` that matched no row and was dropped, so a
#     board package carrying its own `prj.conf` cost a leaf its `os_config`
#     fragment -- an image linked with no zenoh link at all, and no warning.
#   * an unbalanced `[` MERGES lines: list splitting honours square-bracket
#     nesting, so every `;` (i.e. every line break) up to the matching `]` is
#     not a separator, and the rows in between vanish into one element.
#
# Escaping `;` as `\;` (the spelling NanoRosRmwDispatch.cmake carried) fixes
# the first and not the second, and there is no escape for a bracket. So this
# helper never builds a list out of the text at all: it walks the newlines with
# `string(FIND)` and hands each line back in its own variable.
#
# nros_cli_rows(<prefix> <text> [REQUIRE <regex>] [CONTEXT <what>])
#
#   Sets, in the caller's scope:
#     <prefix>_INDICES   "0;1;...;n-1" over the NON-EMPTY lines (empty when
#                        there are none) -- integers, so safe as a list
#     <prefix>_<i>       line <i>, verbatim: `;`, `[`, `]` and `\` intact
#   Iterate:
#     nros_cli_rows(_row "${_out}")
#     foreach(_i IN LISTS _row_INDICES)
#         set(_l "${_row_${_i}}")
#         ...
#
#   REQUIRE: every non-empty line must match <regex>, or the configure stops
#   naming CONTEXT and the line. A reader whose producer has a grammar (the
#   `NROS_LEAF_<KEY>=<value>` rows of `nros ws leaf-system`) states it here, so
#   a line it cannot attribute is an error rather than something to skip -- a
#   skipped line is exactly how the defect above stayed invisible.

include_guard(GLOBAL)

function(nros_cli_rows _ncr_prefix _ncr_text)
    cmake_parse_arguments(PARSE_ARGV 2 _NCR "" "REQUIRE;CONTEXT" "")
    if(_NCR_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nros_cli_rows: unexpected argument(s): ${_NCR_UNPARSED_ARGUMENTS}")
    endif()
    if(NOT DEFINED _NCR_CONTEXT OR _NCR_CONTEXT STREQUAL "")
        set(_NCR_CONTEXT "nros CLI output")
    endif()
    set(_ncr_rest "${_ncr_text}")
    set(_ncr_n 0)
    set(_ncr_indices "")
    while(NOT _ncr_rest STREQUAL "")
        string(FIND "${_ncr_rest}" "\n" _ncr_nl)
        if(_ncr_nl EQUAL -1)
            set(_ncr_line "${_ncr_rest}")
            set(_ncr_rest "")
        else()
            string(SUBSTRING "${_ncr_rest}" 0 ${_ncr_nl} _ncr_line)
            math(EXPR _ncr_next "${_ncr_nl} + 1")
            string(SUBSTRING "${_ncr_rest}" ${_ncr_next} -1 _ncr_rest)
        endif()
        # A CRLF producer (a Windows-built CLI) must not leave `\r` on a value.
        string(REGEX REPLACE "\r$" "" _ncr_line "${_ncr_line}")
        if(_ncr_line STREQUAL "")
            continue()
        endif()
        if(DEFINED _NCR_REQUIRE AND NOT _ncr_line MATCHES "${_NCR_REQUIRE}")
            message(FATAL_ERROR
                "nano-ros: ${_NCR_CONTEXT}: a line that is not a `${_NCR_REQUIRE}` row:\n"
                "  ${_ncr_line}\n"
                "Refused rather than skipped: a skipped line is how a value split "
                "across lines lost a conf file without a word (issue 2200).")
        endif()
        set(${_ncr_prefix}_${_ncr_n} "${_ncr_line}" PARENT_SCOPE)
        list(APPEND _ncr_indices ${_ncr_n})
        math(EXPR _ncr_n "${_ncr_n} + 1")
    endwhile()
    set(${_ncr_prefix}_INDICES "${_ncr_indices}" PARENT_SCOPE)
endfunction()
