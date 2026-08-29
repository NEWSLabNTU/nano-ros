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

# issue 0657 — `nros_corrosion_env_target`.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCorrosionEnv.cmake")

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

# nros_resolve_carve_out_profile(<carve-out-name> <out-prefix>)
#
# Some artifacts cannot use the ambient profile at all (a codegen miscompile, a
# QEMU timing floor, a link-level symbol clash). They name a CARVE-OUT, and the
# table answers with the profile that artifact must use. Sets
# <out-prefix>_PROFILE / _DIR / _ENV in the caller's scope.
function(nros_resolve_carve_out_profile _name _prefix)
    _nros_resolve_codegen_tool(_NANO_ROS_CODEGEN_TOOL)
    execute_process(COMMAND "${_NANO_ROS_CODEGEN_TOOL}" profile carve-out "${_name}"
        OUTPUT_VARIABLE _profile ERROR_VARIABLE _err RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_err}")
    endif()
    execute_process(COMMAND "${_NANO_ROS_CODEGEN_TOOL}" profile dir "${_profile}"
        OUTPUT_VARIABLE _dir RESULT_VARIABLE _rc OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: `nros profile dir ${_profile}` failed")
    endif()
    execute_process(COMMAND "${_NANO_ROS_CODEGEN_TOOL}" profile env "${_profile}" --cmake
        OUTPUT_VARIABLE _env RESULT_VARIABLE _rc OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: `nros profile env ${_profile}` failed")
    endif()
    set(${_prefix}_PROFILE "${_profile}" PARENT_SCOPE)
    set(${_prefix}_DIR "${_dir}" PARENT_SCOPE)
    set(${_prefix}_ENV "${_env}" PARENT_SCOPE)
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
    # issue 0657 — the env-carrying target, not the imported artifact.
    nros_corrosion_env_target("${_target}" _target)
    corrosion_set_env_vars(${_target} ${NROS_CARGO_PROFILE_ENV})
endfunction()

# nros_riscv64_rustflags_env(<target>) — issue 0657
#
# compiler_builtins compiles C fallbacks (`bswapsi2.c` and friends) whose float
# ABI follows RUSTFLAGS, not the `CFLAGS_<triple>` a cc-rs build honours. On the
# toolchain `nros setup` provisions (xPack `riscv-none-elf`, a MULTILIB build)
# the default is soft-float, so those objects came out soft-float while every
# cmake object was `-mabi=lp64d`, and lld refused: "cannot link object files
# with different floating-point ABI".
#
# `riscv64-threadx.cmake` already carried `set(ENV{RUSTFLAGS} "-Ctarget-feature=+d")`
# for exactly this — and issue 0460 is why it did nothing: `set(ENV{})` touches
# the CONFIGURE-time process, and corrosion's cargo runs at BUILD time.
#
# It must also not be exported globally from the lane: cargo's `RUSTFLAGS` env
# REPLACES a leaf's `[build] rustflags`, so a lane-wide export silently drops
# `-C link-arg=-Tlink.lds` and the Rust images fail on `_bss_start` — measured.
# Per-target, through corrosion's own env, is the spelling that reaches the
# right cargo and only that one.
function(nros_riscv64_rustflags_env _target)
    # Gate on the TARGET ARCHITECTURE, not on a toolchain-file variable: normal
    # variables set by a toolchain file are not reliably visible in every
    # subdirectory scope that imports a crate, and the first version of this
    # returned early everywhere it mattered while looking correct.
    if(NOT CMAKE_SYSTEM_PROCESSOR MATCHES "riscv64")
        return()
    endif()
    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_riscv64_rustflags_env(${_target}): Corrosion not loaded")
    endif()
    # issue 0657 — the env-carrying INTERFACE target. Guessing between the two
    # names is what made the first two attempts at this inert; the normaliser
    # states the rule once.
    nros_corrosion_env_target("${_target}" _env_target)
    foreach(_cand "${_env_target}")
        if(TARGET ${_cand})
            # BOTH, because two different compilers produce objects here and
            # only one of them reads RUSTFLAGS:
            #   * rustc compiles compiler_builtins' Rust half — `-Ctarget-feature=+d`;
            #   * cc-rs compiles its C fallbacks (`bswapsi2.c` …) from inside
            #     that crate's build script, and takes `-mabi` from CFLAGS.
            # The C half is the one that actually failed the link, and the
            # RUSTFLAGS line alone left it soft-float.
            corrosion_set_env_vars(${_cand}
                "RUSTFLAGS=-Ctarget-feature=+d"
                "CFLAGS_riscv64gc_unknown_none_elf=-march=rv64gc -mabi=lp64d -mcmodel=medany"
                "CXXFLAGS_riscv64gc_unknown_none_elf=-march=rv64gc -mabi=lp64d -mcmodel=medany")
            message(STATUS "nano-ros: riscv64 hard-float RUSTFLAGS -> ${_cand}")
            return()
        endif()
    endforeach()
    message(STATUS
        "nano-ros: riscv64 hard-float RUSTFLAGS NOT attached — no target named "
        "${_target} or ${_target}-static (issue 0657)")
endfunction()

# nros_armv8r_cflags_env(<target>) — phase-372 W1, issue 0657's class on a
# second architecture.
#
# cc-rs builds inside cargo crates (nros-c's shim TUs, compiler_builtins' C
# fallbacks) derive their flags from the Rust triple, and for
# `armv8r-none-eabihf` that yields `-mfloat-abi=hard` with NO `-mfpu` — gcc
# refuses: "selected architecture lacks an FPU". The cmake lane's own TUs get
# the FPU from the toolchain file; the cargo-side C needs it through
# `CFLAGS_<triple>`, per-target via corrosion's env (the 0657 lesson: a
# configure-time `set(ENV{})` never reaches build-time cargo, and a lane-wide
# RUSTFLAGS export clobbers leaf link args).
function(nros_armv8r_cflags_env _target)
    if(NOT CMAKE_SYSTEM_PROCESSOR MATCHES "cortex-r52")
        return()
    endif()
    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_armv8r_cflags_env(${_target}): Corrosion not loaded")
    endif()
    nros_corrosion_env_target("${_target}" _env_target)
    if(TARGET ${_env_target})
        corrosion_set_env_vars(${_env_target}
            "CFLAGS_armv8r_none_eabihf=-mcpu=cortex-r52 -mfpu=neon-fp-armv8 -mfloat-abi=hard"
            "CXXFLAGS_armv8r_none_eabihf=-mcpu=cortex-r52 -mfpu=neon-fp-armv8 -mfloat-abi=hard")
        message(STATUS "nano-ros: cortex-r52 FPU CFLAGS -> ${_env_target}")
        return()
    endif()
    message(STATUS
        "nano-ros: cortex-r52 FPU CFLAGS NOT attached — no target named "
        "${_env_target} (phase-372 W1)")
endfunction()

# nros_board_toolchain_env(<target>) — issue 0835
#
# The board's TOOLCHAIN paths, carried to cargo the same way the facts above
# are. Separate from `nros_board_facts_env` because these do not come from
# `nros ws board-facts`: a board module computes them at configure time, and a
# standalone example leaf has no workspace for the facts path to resolve at all.
#
# WHY this exists. `cmake/board/nano-ros-board-riscv64-qemu.cmake` publishes
# `THREADX_CONFIG_DIR` / `NETX_CONFIG_DIR` / `THREADX_PORT` /
# `THREADX_EXTRA_INCLUDES` with `set(ENV{...})`, which only touches the
# CONFIGURE-time process. Corrosion's cargo step is a build-time custom command
# and inherits none of it, so `zpico-sys`'s build script read no
# `NETX_CONFIG_DIR` (`nros-zpico-build/src/runner.rs`, which simply omits the
# include when the var is unset) and every riscv64 ThreadX C/C++ cell failed:
#
#   third-party/threadx/netxduo/common/inc/nx_api.h:155:10:
#       fatal error: nx_port.h: No such file or directory
#
# NetX Duo ships no riscv64 port; the header lives in the BOARD's config dir
# (`packages/boards/nros-board-threadx-qemu-riscv64/config/nx_port.h`), which is
# exactly what the unset variable was supposed to name. Measured on
# `examples/qemu-riscv64-threadx/c/talker/build-zenoh`: `cmake --build` returns
# 101 as-is and 0 with those two variables exported, same tree.
#
# This is issue 0460 one lane over — there a Kconfig knob reached the Zephyr C
# lane and not the Rust one, for the same reason and with the same `set(ENV{})`
# at the root of it.
function(nros_board_toolchain_env _target)
    # ENV is checked as well as the cache because the board module publishes
    # some of these ONLY as process env (`THREADX_PORT`,
    # `THREADX_EXTRA_INCLUDES`), which is the very bug this forwards past.
    set(_names THREADX_CONFIG_DIR NETX_CONFIG_DIR THREADX_PORT
               THREADX_EXTRA_INCLUDES THREADX_DIR NETX_DIR THREADX_BOARD_DIR)
    set(_env "")
    foreach(_name IN LISTS _names)
        if(DEFINED ${_name})
            list(APPEND _env "${_name}=${${_name}}")
        elseif(DEFINED ENV{${_name}})
            list(APPEND _env "${_name}=$ENV{${_name}}")
        endif()
    endforeach()
    if(_env STREQUAL "")
        return()
    endif()
    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_board_toolchain_env(${_target}): Corrosion not loaded")
    endif()
    # issue 0657 — the INTERFACE target is the one the cargo command reads.
    nros_corrosion_env_target("${_target}" _target)
    corrosion_set_env_vars(${_target} ${_env})
    list(LENGTH _env _n)
    message(STATUS "nano-ros: ${_n} board toolchain path(s) delivered to cargo")
endfunction()
