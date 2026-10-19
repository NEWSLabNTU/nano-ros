# cmake/NanoRosRootProfiles.cmake — issue 1953
#
# A cargo root nano-ros GENERATES at configure time (the RFC-0071 D9 image
# crate, the workspace runtime umbrella, the per-package C++ FFI glue) is its
# own `[workspace]`, so cargo reads its profiles from THAT manifest and from
# nothing else. Before D9 the API crate was built as a member of the
# repository's root workspace, which defines every profile it uses; the image
# crate moved it into a generated root and silently dropped all of them.
#
# The `nros-*` presets survived, because `nros_cargo_profile_env()` injects
# them as `CARGO_PROFILE_*` variables. `release` and `dev` did not: nano-ros
# deliberately never injects those (nros-cargo-profile's ownership rule — a
# `CARGO_PROFILE_*` variable overrides a user's own manifest). So a generated
# root built with `--profile release` got cargo's DEFAULT release profile:
# `panic = "unwind"`, thin-local LTO, 16 codegen units. On a host-triple
# `no_std` image (threadx-linux C/C++) the unwind is fatal — `nros-c` builds
# `staticlib`/`cdylib` artifacts and rustc refuses "unwinding panics are not
# supported without std". On a cross target it compiled and quietly lost the
# fat LTO the root asks for.
#
# A generated root is not a user's manifest — nano-ros writes it — so the
# ownership rule does not apply to it. It carries the root workspace's
# profiles, COPIED from the root `Cargo.toml` at configure time rather than
# restated, so there is no second table to drift (the hand copy in
# `cpp_ffi_Cargo.toml.in` had already drifted: `lto = true`, no
# `codegen-units`).
#
#   nros_root_cargo_profiles(<out-var> [REPO <checkout>])
#
# Sets <out-var> to the root's `[profile.*]` tables as TOML (comments and blank
# lines dropped), ready to append to a generated manifest. REPO defaults to the
# checkout this module sits in. Works in script mode (`cmake -P`), which the
# NuttX apps road uses to generate the image crate.
#
# Gate: `just check generated-root-profiles` — every cmake emitter of a
# `[workspace]` table carries the profiles, and this helper's output holds
# `panic = "abort"` for both built-in profiles.

include_guard(GLOBAL)

set(_NROS_ROOT_PROFILES_REPO "${CMAKE_CURRENT_LIST_DIR}/.."
    CACHE INTERNAL "nano-ros checkout, for NanoRosRootProfiles.cmake")

function(nros_root_cargo_profiles out)
    cmake_parse_arguments(_RP "" "REPO" "" ${ARGN})
    if(NOT _RP_REPO)
        set(_RP_REPO "${_NROS_ROOT_PROFILES_REPO}")
    endif()
    get_filename_component(_root "${_RP_REPO}/Cargo.toml" ABSOLUTE)
    if(NOT EXISTS "${_root}")
        message(FATAL_ERROR
            "nros_root_cargo_profiles: no root workspace manifest at ${_root}. "
            "A generated cargo root copies its profiles from it (issue 1953); "
            "without them a `release` build gets cargo's default "
            "`panic = \"unwind\"`.")
    endif()
    # Walk the lines by hand. `file(STRINGS)` returns a CMake LIST, and the
    # manifest's comments hold `;` and unbalanced `[` — a list splits on the
    # first and stops splitting inside the second, so comment fragments came
    # out as TOML keys (`key with no value, expected '='`).
    file(READ "${_root}" _rest)
    set(_in FALSE)
    set(_toml "")
    while(NOT _rest STREQUAL "")
        string(FIND "${_rest}" "\n" _nl)
        if(_nl EQUAL -1)
            set(_line "${_rest}")
            set(_rest "")
        else()
            string(SUBSTRING "${_rest}" 0 ${_nl} _line)
            math(EXPR _next "${_nl} + 1")
            string(SUBSTRING "${_rest}" ${_next} -1 _rest)
        endif()
        if(_line MATCHES "^[ \t]*\\[")
            if(_line MATCHES "^\\[profile\\.")
                set(_in TRUE)
                string(APPEND _toml "\n${_line}\n")
            else()
                set(_in FALSE)
            endif()
        elseif(_in)
            # Drop comment-only lines and trailing comments. Profile values
            # are numbers, booleans and bare words, so a `#` is never data.
            string(REGEX REPLACE "[ \t]*#.*$" "" _kv "${_line}")
            if(NOT _kv STREQUAL "")
                string(APPEND _toml "${_kv}\n")
            endif()
        endif()
    endwhile()
    if(NOT _toml MATCHES "\\[profile\\.release\\]")
        message(FATAL_ERROR
            "nros_root_cargo_profiles: ${_root} defines no [profile.release]; "
            "a generated root built at the `release` profile would fall back "
            "to cargo's unwinding default (issue 1953).")
    endif()
    set(${out}
"
# Copied from the root workspace manifest at configure time
# (cmake/NanoRosRootProfiles.cmake, issue 1953): a generated root is its own
# workspace, so it gets no profile it does not state.${_toml}" PARENT_SCOPE)
endfunction()

if(CMAKE_SCRIPT_MODE_FILE AND
   CMAKE_SCRIPT_MODE_FILE STREQUAL CMAKE_CURRENT_LIST_FILE)
    # `cmake [-DNROS_ROOT_PROFILES_REPO=<dir>] -P cmake/NanoRosRootProfiles.cmake`
    # prints the TOML — the gate's probe of the real root.
    set(_repo_arg "")
    if(DEFINED NROS_ROOT_PROFILES_REPO)
        set(_repo_arg REPO "${NROS_ROOT_PROFILES_REPO}")
    endif()
    nros_root_cargo_profiles(_toml ${_repo_arg})
    execute_process(COMMAND "${CMAKE_COMMAND}" -E echo "${_toml}")
endif()
