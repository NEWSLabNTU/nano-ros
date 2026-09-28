# cmake/NanoRosSdkPin.cmake
#
# nros_sdk_pin(<tool> <out_version> <out_upstream>)
#
# The version `nros-sdk-index.toml` PINS for `[tool.<tool>]` — the one cmake
# reader of it, issue 1546.
#
# A provisioned tool lives at `<store>/<tool>/<version>` because `nros setup`
# read `<version>` from the index. A consumer CONSTRUCTS that path from the same
# two inputs and never lists the store to pick one: the store is SHARED between
# checkouts and ACCUMULATES (issue 0500) while the pin is per-checkout, so "the
# newest version present" is as often a sibling checkout's answer as ours
# (phase-365; gated by `check-sdk-store-not-enumerated`).
#
# Both outputs are empty when the index is absent (a module used outside a
# nano-ros checkout) or has no such section; a caller then has no store rung
# and must say so rather than guess.
#
# Twins, because the build systems cannot call each other:
# `nros_sdk_pinned_version` in scripts/lib/sdk-pin.sh and
# `nros_build_paths::sdk_pinned_version()`. Where the `nros` CLI is reachable
# and the caller's store root is the CLI's, `nros sdk-path <tool>` answers the
# whole path.
include_guard(GLOBAL)

# The `[tool.<name>]` pin from nros-sdk-index.toml — `version` (the repackaged
# id) and `upstream` (the vendor release the pin repackages).
#
# Read from the index rather than restated here on purpose: an AUTHORED copy of
# a pinned version is a map that drifts from the territory, which is the failure
# `check-rmw-api-parity` records at length. There is nothing to keep in sync
# because there is no second copy.
function(nros_sdk_pin tool out_version out_upstream)
    set(${out_version} "" PARENT_SCOPE)
    set(${out_upstream} "" PARENT_SCOPE)
    set(_index "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../nros-sdk-index.toml")
    if(NOT EXISTS "${_index}")
        return()
    endif()
    file(READ "${_index}" _txt)
    # The section runs from its header to the next line that STARTS a table.
    # `[^\[]*` would stop at the `smoke = [` array inside the section.
    string(REPLACE "." "\\." _tool_re "${tool}")
    string(REGEX MATCH "\n\\[tool\\.${_tool_re}\\]\n(([^\n\\[][^\n]*)?\n)*" _sec "\n${_txt}")
    if(NOT _sec)
        return()
    endif()
    if(_sec MATCHES "\nversion[ \t]*=[ \t]*\"([^\"]*)\"")
        set(${out_version} "${CMAKE_MATCH_1}" PARENT_SCOPE)
    endif()
    if(_sec MATCHES "\nupstream[ \t]*=[ \t]*\"([^\"]*)\"")
        set(${out_upstream} "${CMAKE_MATCH_1}" PARENT_SCOPE)
    endif()
endfunction()
