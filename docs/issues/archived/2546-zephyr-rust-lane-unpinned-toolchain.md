---
id: 2546
title: "A fresh `nros new` Zephyr project builds its Rust app with the user's default toolchain, not the pinned one"
status: resolved
resolved: 2026-11-15
type: bug
severity: low
area: [build, zephyr, tooling]
related: [issue-1447, issue-2521, phase-557]
found: 2026-11-14
---

## What

A project made by `nros new` for nucleo-f767zi (Zephyr) lives outside the
nano-ros checkout and has no `rust-toolchain.toml`. Its Rust app lane
(`rust/target/…/librustapp.a`) is built by whatever toolchain the user's
rustup defaults to, not the one `rust-toolchain.toml` pins (issue 1447).

## Measured

A fresh `talker/rust/zenoh` project, built from this tree on 2026-11-14:

- the link map's `compiler_builtins` units are
  `compiler_builtins-8e38540563db7fec`, which is
  `~/.rustup/toolchains/stable-*/lib/rustlib/thumbv7em-none-eabi/lib/libcompiler_builtins-8e38540563db7fec.rlib`;
- that is `stable` (1.98.1 here). The pinned toolchain is 1.99.0, whose rlib
  is `libcompiler_builtins-dfc34ed782ea8c89.rlib`.

So two users with different default toolchains build different code from one
template, and the stack walk (`scripts/nros-stack-floor.py`) measures
different frames for it. Before issue 2521 the walk's `--sysroot` read the
PINNED toolchain's bitcode for this lane, i.e. code the image did not link;
it now picks the rlibs the link used, by crate hash.

## Fix

Have the generated project (or the Zephyr Rust lane's cargo invocation) name
the pinned toolchain, the way the in-tree build does.

## Acceptance

A fresh `nros new` Zephyr Rust project links the pinned toolchain's sysroot
crates whatever the user's rustup default is.

## Resolution (2026-11-15)

The Zephyr module's Rust lane now carries the pin. `zephyr/CMakeLists.txt`
calls `nros_pin_default_rust_toolchain()` and appends
`nros_rust_toolchain_env()`'s `RUSTUP_TOOLCHAIN` to the lane's environment
(`NROS_BOARD_FACTS_ENV`), the same predicate and helper the C lane has used
since issue 2370. It pins only when rustup reports its "(default)" for the
project and nothing else chose a toolchain, so a user's
`rust-toolchain.toml` or `RUSTUP_TOOLCHAIN` still wins. Unlike the C lane
there is no build-std arm: zephyr-lang-rust links the prebuilt `rust-std`
`nros setup` installs on the pinned stable, so every triple gets the pin.

**Measured.** A fresh `nros new` nucleo talker, on a host whose rustup
default is `stable` (1.98.1), now links `compiler_builtins-dfc34ed782ea8c89`
(the pinned 1.99.0's), and its `build.ninja` sets `RUSTUP_TOOLCHAIN=1.99.0`.

The pin changes the code, so every `[[observed]]` static in
`config/stack-floors.toml` was re-walked:

| image | before | after | why |
| --- | ---: | ---: | --- |
| nucleo Rust action server | 68,608 | 68,752 | this fix: pinned toolchain |
| nucleo Rust talker | 67,672 | 67,688 | this fix: pinned toolchain |
| nucleo C talker | 99,140 | 99,140 | unchanged: the C lane was already pinned |
| mps2 bare-metal Rust talker | 95,420 | 100,660 | not this fix: board changes on main since phase-557 W7e (phase-553 W6, issue 2555) |

The mps2 talker is built by cargo inside the checkout, under the root
`rust-toolchain.toml`, and this fix touches only the Zephyr module, so its
move is drift the table had not caught up with. The recorded peaks are
unchanged; they are runtime observations, and every new floor still covers
its peak.
