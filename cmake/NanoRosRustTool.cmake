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
# BY NAME WHEN THE NAME RESOLVES. That is not a fallback kept for safety: a
# contributor's `cargo` on PATH is `scripts/bin/cargo`, the shim that injects
# `--locked` project-wide (CLAUDE.md, issues 0359/0378), and it can only reach
# a command that asks for `cargo` by name. So the answer is unchanged for
# every host where the name already worked, and differs only where it failed.
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
