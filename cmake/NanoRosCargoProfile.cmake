# NanoRosCargoProfile.cmake — phase-336
#
# Resolve ONE cargo profile for everything cmake builds through Corrosion or a
# custom command, and expose its derived forms. The table lives in the `nros`
# CLI (`nros profile`, backed by `packages/tooling/nros-cargo-profile`); this
# module is the bridge, so cmake never carries a second copy of the mapping.
#
# Two knobs, deliberately not unified: `CMAKE_BUILD_TYPE` keeps its normal
# meaning for C/C++, and `NROS_CARGO_PROFILE` selects the cargo profile. When
# the latter is unset it is DERIVED from the former (Debug→dev,
# RelWithDebInfo→nros-relwithdebinfo, MinSizeRel→nros-minsizerel,
# Release→release; unset→the development default; anything else is an error).
#
# Corrosion's own default is `Debug → dev`, everything else → `release`, which
# maps a `-O0`-intent CMake build onto a fat-LTO cargo build. Passing PROFILE
# explicitly at every import is what replaces that.
#
# After nros_resolve_cargo_profile():
#   NROS_CARGO_PROFILE       the profile name          (cache)
#   NROS_CARGO_PROFILE_DIR   its `target/` subdir      (cache)
#   NROS_CARGO_PROFILE_ENV   `K=V;K=V` definition, EMPTY unless nano-ros owns
#                            the profile — pass to corrosion_set_env_vars() so a
#                            workspace with no `[profile.*]` block still builds
#                            (cache)

include_guard(GLOBAL)

# At FILE scope, not inside the function below: an `include()` in a function
# body loses the included file's normal variables when the frame pops (the
# `_NROS_ENTRY_DIR` class in CLAUDE.md). Only the function definitions would
# survive, and relying on that is how that pitfall bites the next reader.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")

function(nros_resolve_cargo_profile)
    if(NROS_CARGO_PROFILE AND DEFINED NROS_CARGO_PROFILE_DIR)
        return()  # resolved earlier in this configure
    endif()

    _nros_resolve_codegen_tool(_NANO_ROS_CODEGEN_TOOL)
    set(_nros "${_NANO_ROS_CODEGEN_TOOL}")

    # An explicit -DNROS_CARGO_PROFILE=<name> wins; a user profile is passed
    # through verbatim and NOT defined by us (see the ownership rule in
    # nros-cargo-profile).
    set(_profile "${NROS_CARGO_PROFILE}")
    if(_profile STREQUAL "")
        set(_profile "$ENV{NROS_CARGO_PROFILE}")
    endif()
    if(_profile STREQUAL "")
        execute_process(
            COMMAND "${_nros}" profile resolve --build-type "${CMAKE_BUILD_TYPE}"
            OUTPUT_VARIABLE _profile
            ERROR_VARIABLE _err
            RESULT_VARIABLE _rc
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        if(NOT _rc EQUAL 0)
            # An unmapped CMAKE_BUILD_TYPE is fatal on purpose: guessing a
            # profile here would silently build at an optimization level the
            # user did not ask for, which is the behaviour this phase removes.
            message(FATAL_ERROR "nano-ros: ${_err}")
        endif()
    endif()

    execute_process(COMMAND "${_nros}" profile dir "${_profile}"
        OUTPUT_VARIABLE _dir RESULT_VARIABLE _rc OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: `nros profile dir ${_profile}` failed")
    endif()

    execute_process(COMMAND "${_nros}" profile env "${_profile}" --cmake
        OUTPUT_VARIABLE _env RESULT_VARIABLE _rc OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: `nros profile env ${_profile}` failed")
    endif()

    set(NROS_CARGO_PROFILE "${_profile}" CACHE STRING
        "Cargo profile for every crate nano-ros builds (empty = derive from CMAKE_BUILD_TYPE)" FORCE)
    set(NROS_CARGO_PROFILE_DIR "${_dir}" CACHE INTERNAL
        "target/ subdirectory for NROS_CARGO_PROFILE")
    set(NROS_CARGO_PROFILE_ENV "${_env}" CACHE INTERNAL
        "CARGO_PROFILE_* definition of NROS_CARGO_PROFILE, empty for user-owned profiles")

    message(STATUS
        "nano-ros: cargo profile `${_profile}` (CMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}) "
        "→ target/${_dir}")
endfunction()

# nros_cargo_profile_env(<target>)
#
# Give a Corrosion target the profile's definition, so the crate resolves
# `--profile <name>` even when its own manifest declares no such profile. A
# no-op for profiles nano-ros does not own — injecting there would OVERRIDE the
# user's own `[profile.<name>]`, because a CARGO_PROFILE_* variable beats the
# manifest.
function(nros_cargo_profile_env _target)
    nros_resolve_cargo_profile()
    if(NROS_CARGO_PROFILE_ENV STREQUAL "")
        return()
    endif()
    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_cargo_profile_env(${_target}): Corrosion not loaded")
    endif()
    corrosion_set_env_vars(${_target} ${NROS_CARGO_PROFILE_ENV})
endfunction()
