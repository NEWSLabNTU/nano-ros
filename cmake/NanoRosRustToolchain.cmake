# NanoRosRustToolchain.cmake — the pinned nightly, read from its one source.
#
# Issue 2312. The Zephyr C road builds `nros-c` / `nros-cpp` for an embedded
# triple on the pinned nightly with `-Z build-std`, and the NuttX toolchain
# files hand Corrosion the same channel. Each spelled it as a literal
# (`nightly-2026-04-11`), outside every gate, so a nightly bump left them
# behind, and nothing that provisions a host could see which channel the build
# would ask rustup for.
#
# The channel is the first `nightly-*` row of `config/rust-toolchains.txt`,
# the generated fold of every tracked `rust-toolchain.toml` (its `nightly-*`
# row carries `tools/rust-toolchain.toml`). That is the file an installed SDK
# root ships, and the row the CLI reads too: `builder::preflight::pinned_nightly`
# (what `nros build` checks) and the SDK index's
# `[rust.toolchain.nightly-pinned]` (what `nros setup` installs, kept equal to
# the file by `check-rust-targets-covered`). That gate also refuses a
# `nightly-<date>` literal in any tracked cmake file.

# nros_pinned_nightly(<out>) — the pinned nightly channel. A missing or
# rowless file is a FATAL_ERROR naming it: building on a guessed channel is the
# defect this exists to remove.
#
# The path is taken from `CMAKE_CURRENT_FUNCTION_LIST_DIR`, never a variable
# set at include time: the Zephyr FFI crate calls this from a DEFERRED call in
# the application's directory scope, where a variable of this file's scope is
# not visible (measured: the list path read as empty).
function(nros_pinned_nightly out)
    set(_NROS_RUST_TOOLCHAINS_LIST
        "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../config/rust-toolchains.txt")
    if(NOT EXISTS "${_NROS_RUST_TOOLCHAINS_LIST}")
        # issue 2312.
        message(FATAL_ERROR
            "nano-ros: no ${_NROS_RUST_TOOLCHAINS_LIST} -- the pinned nightly "
            "channel is read from it. An SDK root ships it; a "
            "checkout regenerates it with `python3 scripts/gen-rust-toolchains.py`.")
    endif()
    file(STRINGS "${_NROS_RUST_TOOLCHAINS_LIST}" _rows REGEX "^nightly-")
    if(NOT _rows)
        # issue 2312.
        message(FATAL_ERROR
            "nano-ros: ${_NROS_RUST_TOOLCHAINS_LIST} has no `nightly-*` row "
            "-- the pinned nightly channel is read from it. A checkout regenerates "
            "it with `python3 scripts/gen-rust-toolchains.py`.")
    endif()
    list(GET _rows 0 _row)
    string(REGEX MATCH "^[^ \t]+" _channel "${_row}")
    set(${out} "${_channel}" PARENT_SCOPE)
endfunction()

# nros_rust_target_builds_std(<triple> <out>) — whether the runtime for
# <triple> builds `core`/`alloc` from source on the pinned nightly: the
# embedded triples (armv7a, thumbv*, riscv32). The CLI's
# `builder::preflight::zephyr_runtime_builds_std` is the same rule, for what
# `nros setup board` installs and `nros build` checks.
function(nros_rust_target_builds_std triple out)
    if(triple MATCHES "^(armv7a|thumbv|riscv32)")
        set(${out} TRUE PARENT_SCOPE)
    else()
        set(${out} FALSE PARENT_SCOPE)
    endif()
endfunction()
