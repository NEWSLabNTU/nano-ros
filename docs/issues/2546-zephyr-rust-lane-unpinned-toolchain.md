---
id: 2546
title: "A fresh `nros new` Zephyr project builds its Rust app with the user's default toolchain, not the pinned one"
status: open
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
