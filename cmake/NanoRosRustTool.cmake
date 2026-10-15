# NanoRosRustTool.cmake — the ONE spelling of "which cargo / rustc does a
# command this build emits run" (issue 1304).
#
# Every cargo invocation Corrosion makes goes through the executables its
# FindRust RESOLVED — rustup in `$HOME/.cargo/bin` needs nothing on PATH. The
# custom commands nano-ros emits itself (the message FFI glue, the NuttX
# cross-link, the Zephyr workspace-root probe, the host-triple / sysroot
# queries) ran `cargo` / `rustc` by NAME, so they needed PATH to agree with
# FindRust. It always did on a contributor host, where `activate.sh` has run.
# On an installed toolchain, where `nros setup` just ran a `--no-modify-path`
# rustup-init, configure found Rust and the build died at 92 % with a bare
# "No such file or directory" from the first message package's FFI glue.
#
# BY NAME WHEN THE NAME RESOLVES, so the answer is unchanged for every host
# where the name already worked, and differs only where it failed. (A
# `--locked` PATH shim once made the name load-bearing too, issues 0359/0378;
# it is retired, phase-485 W4.)
#
# Otherwise, in the order FindRust itself looks: the rustup PROXY beside the
# `rustup` it found (a proxy, not the toolchain binary, because the NuttX lane
# passes `+<toolchain>` and only a proxy understands it), then the proxies in
# `$CARGO_HOME/bin` and `$HOME/.cargo/bin` — where `rustup-init` installs, and
# what `nros setup`'s rust step (orchestration/rust_toolchain.rs) probes — and
# last Corrosion's resolved absolute path. With none of those, the bare name,
# which fails as it always did.
#
# Standalone and include-guarded so the Zephyr module can include it directly,
# without pulling in NanoRosCodegenCore.cmake.

include_guard(GLOBAL)

# CACHE INTERNAL: this file is include()d from inside functions, and a plain
# file-scope `set` is dropped when that frame pops (the `_NROS_ENTRY_DIR`
# pattern).
set(_NROS_RUST_TOOL_DIR "${CMAKE_CURRENT_LIST_DIR}"
    CACHE INTERNAL "directory holding NanoRosRustTool.cmake")

function(nros_rust_tool out name)
    # NO_CACHE and an unset first: `find_program` is a no-op on a variable
    # that is already defined (issue 0726), and a cached hit would outlive the
    # PATH it was found on.
    unset(_nros_rt_hit)
    find_program(_nros_rt_hit NAMES "${name}" NO_CACHE)
    if(_nros_rt_hit)
        set(${out} "${name}" PARENT_SCOPE)
        return()
    endif()

    set(_nros_rt_dirs "")
    if(Rust_RUSTUP)
        get_filename_component(_nros_rt_proxies "${Rust_RUSTUP}" DIRECTORY)
        list(APPEND _nros_rt_dirs "${_nros_rt_proxies}")
    endif()
    if(DEFINED ENV{CARGO_HOME} AND NOT "$ENV{CARGO_HOME}" STREQUAL "")
        list(APPEND _nros_rt_dirs "$ENV{CARGO_HOME}/bin")
    endif()
    if(DEFINED ENV{HOME} AND NOT "$ENV{HOME}" STREQUAL "")
        list(APPEND _nros_rt_dirs "$ENV{HOME}/.cargo/bin")
    endif()
    foreach(_d IN LISTS _nros_rt_dirs)
        if(EXISTS "${_d}/${name}" AND NOT IS_DIRECTORY "${_d}/${name}")
            set(${out} "${_d}/${name}" PARENT_SCOPE)
            return()
        endif()
    endforeach()

    if(name STREQUAL "cargo" AND Rust_CARGO_CACHED AND EXISTS "${Rust_CARGO_CACHED}")
        set(${out} "${Rust_CARGO_CACHED}" PARENT_SCOPE)
        return()
    endif()
    if(name STREQUAL "rustc" AND Rust_COMPILER_CACHED AND EXISTS "${Rust_COMPILER_CACHED}")
        set(${out} "${Rust_COMPILER_CACHED}" PARENT_SCOPE)
        return()
    endif()
    set(${out} "${name}" PARENT_SCOPE)
endfunction()

# --------------------------------------------------------------------------
# nros_pin_default_rust_toolchain()
#
# Issue 1914 — a project OUTSIDE this checkout (a copied-out template, a user's
# own project) has no `rust-toolchain.toml` above it, so rustup answers with the
# host's DEFAULT toolchain. That is not the toolchain `nros setup` provisions
# board targets on — it adds them to the PINNED one, `rust-toolchain.toml`'s
# `channel` — so every cross crate died with `E0463: can't find crate for
# core` / "the `thumbv7m-none-eabi` target may not be installed" (run-matrix
# 38658400956, `template-copy-out`). In-tree the root file governs and nothing
# differs, which is why only a copy saw it.
#
# Two consumers pick a toolchain, and both have to be told:
#   * Corrosion's FindRust, at configure time: it takes `Rust_TOOLCHAIN` when
#     defined, else the toolchain rustup reports as active from the configure's
#     cwd. Setting it here, before `find_package(Corrosion)`, is the seam.
#   * the cargo commands nano-ros emits itself (`nros_rust_tool`), which run a
#     rustup PROXY with a build-dir cwd. They read `nros_rust_toolchain_env()`.
#
# Pins ONLY when nothing else chose: an explicit `-DRust_TOOLCHAIN`, a set
# `RUSTUP_TOOLCHAIN`, or any rustup override for the source dir (a project's own
# `rust-toolchain.toml`, `rustup override`) is a decision and is kept. The test
# is rustup's own: `rustup show active-toolchain` says `(default)` exactly when
# no override applies. NuttX and Zephyr pin their own nightly before this runs.
#
# Idempotent across re-configures: the pinned value is cached, and the
# configure-time environment (lost when cmake exits) is re-exported from it.
function(nros_pin_default_rust_toolchain)
    if(DEFINED CACHE{NROS_PINNED_RUST_TOOLCHAIN})
        if("$ENV{RUSTUP_TOOLCHAIN}" STREQUAL "")
            set(ENV{RUSTUP_TOOLCHAIN} "${NROS_PINNED_RUST_TOOLCHAIN}")
        endif()
        return()
    endif()
    if(DEFINED Rust_TOOLCHAIN OR NOT "$ENV{RUSTUP_TOOLCHAIN}" STREQUAL "")
        return()
    endif()
    set(_pin_file "${_NROS_RUST_TOOL_DIR}/../rust-toolchain.toml")
    if(NOT EXISTS "${_pin_file}")
        return()
    endif()
    file(READ "${_pin_file}" _pin_text)
    if(NOT _pin_text MATCHES "\n[ \t]*channel[ \t]*=[ \t]*\"([^\"]+)\"")
        return()
    endif()
    set(_pin "${CMAKE_MATCH_1}")

    unset(_pin_rustup)
    find_program(_pin_rustup NAMES rustup NO_CACHE
        PATHS "$ENV{CARGO_HOME}/bin" "$ENV{HOME}/.cargo/bin")
    if(NOT _pin_rustup)
        return()
    endif()
    execute_process(
        COMMAND "${_pin_rustup}" show active-toolchain
        WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
        RESULT_VARIABLE _pin_rc
        OUTPUT_VARIABLE _pin_active
        ERROR_QUIET OUTPUT_STRIP_TRAILING_WHITESPACE)
    # A non-zero exit is rustup failing to name ANY toolchain (no default set):
    # the pin is then the only answer there is.
    if(_pin_rc EQUAL 0 AND NOT _pin_active MATCHES "\\(default\\)")
        return()
    endif()
    # Not installed: keep rustup's default rather than make FindRust refuse a
    # host build that the default could still do. Say what a cross build needs.
    execute_process(
        COMMAND "${_pin_rustup}" toolchain list
        OUTPUT_VARIABLE _pin_list ERROR_QUIET)
    string(REPLACE "." "\\." _pin_re "${_pin}")
    if(NOT _pin_list MATCHES "(^|\n)${_pin_re}(-[^ \n]+)?( |\n|$)")
        message(STATUS "nano-ros: Rust toolchain ${_pin} (rust-toolchain.toml) is not "
            "installed; using rustup's default, which may lack board targets "
            "(nros setup <board> installs ${_pin} with them — issue 1914)")
        return()
    endif()

    set(NROS_PINNED_RUST_TOOLCHAIN "${_pin}" CACHE INTERNAL
        "issue 1914 — the toolchain nano-ros pinned for a project rustup had no override for")
    set(Rust_TOOLCHAIN "${_pin}" CACHE STRING
        "Rust toolchain (nano-ros pin from rust-toolchain.toml — issue 1914)")
    set(ENV{RUSTUP_TOOLCHAIN} "${_pin}")
    message(STATUS "nano-ros: Rust toolchain ${_pin} (rust-toolchain.toml pin; "
        "rustup had no override for ${CMAKE_SOURCE_DIR} — issue 1914). "
        "Pass -DRust_TOOLCHAIN=<tc> to choose another.")
endfunction()

# nros_rust_toolchain_env(<out>) — `RUSTUP_TOOLCHAIN=<pin>` for a BUILD-time
# command's `cmake -E env`, or empty when nothing was pinned. A build-time
# command does not inherit the configure's environment (issue 0460's lesson),
# so the pin has to ride on the command. Harmless for a non-rustup cargo, which
# ignores the variable — unlike a `+<toolchain>` argument.
function(nros_rust_toolchain_env out)
    if(DEFINED CACHE{NROS_PINNED_RUST_TOOLCHAIN})
        set(${out} "RUSTUP_TOOLCHAIN=${NROS_PINNED_RUST_TOOLCHAIN}" PARENT_SCOPE)
    else()
        set(${out} "" PARENT_SCOPE)
    endif()
endfunction()
