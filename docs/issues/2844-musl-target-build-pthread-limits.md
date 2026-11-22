---
id: 2844
title: "A musl Linux target does not build: the POSIX port misses `<limits.h>`, and two build scripts name `-lpthread` / `-lrt`"
status: open
type: bug
severity: medium
area: [platform, posix, build, rmw]
related: [phase-589]
found: 2026-11-23
---

## What

A hosted Linux image cross-built for `aarch64-unknown-linux-musl` (a static
binary for an IP camera SoC: HiSilicon Hi3519DV500, 2x Cortex-A55, aarch64
Linux 5.10, musl 1.2.3, BusyBox) fails twice before it links. Both failures
are musl facts the tree assumes away; neither is specific to that board or to
aarch64.

1. **Compile.** `packages/platform/nros-platform-posix/src/platform.c` uses
   `PTHREAD_STACK_MIN` and includes only `<pthread.h>` for it. POSIX defines
   the constant in `<limits.h>`. glibc also exposes it through `<pthread.h>`;
   musl does not, so the TU fails with `'PTHREAD_STACK_MIN' undeclared`.
2. **Link.** `nros-platform-cffi/build.rs` (feature `posix-c-port`) prints
   `cargo:rustc-link-lib=pthread`, plus `rt` on Linux, and
   `nros-zpico-build/src/runner.rs` prints `pthread` for any Linux target
   (the vendored and the system zenoh-pico arms). musl keeps both in `libc.a`
   and ships no `libpthread.a` / `librt.a`; Rust's self-contained musl sysroot
   (the default for `*-linux-musl*`) is one of those, so the final link fails:
   `rust-lld: error: unable to find library -lpthread`.

## How it was found

A downstream user's proof of concept: the Linux talker (Rust, zenoh, client
mode to a router on a host) cross-built with `nros build` and an out-of-tree
board that reuses `nros-board-linux` with `target =
"aarch64-unknown-linux-musl"`, `linker = "rust-lld"` and
`-C target-feature=+crt-static`. With a local patch for the three sites above
it built, ran on the device and was received on the host. Measured there: a
1.3 MB static binary, 1.37 MB peak RSS, ~0.1 % CPU at 1 Hz.

## Fix direction

- `platform.c` includes `<limits.h>`.
- The decision "does this TARGET's libc carry pthread and rt itself" is one
  helper in the shared build-script crate, `nros_cc_flags::libc_libs`, read
  from cargo's `CARGO_CFG_TARGET_ENV` (the target being built, not the host
  the build script runs on). Both build scripts route their `-l` lines
  through it. glibc and every other target link exactly what they did before.

Sibling sites, checked and left alone with the reason:

- `nros-board-threadx-linux/build.rs`, `nros-board-freertos-posix/build.rs`:
  glibc by construction (they link `-lc` for a `no_std` entry and start in
  glibc's crt0).
- `cyclonedds-sys/build.rs`: builds Cyclone for the HOST with cmake and has no
  cross-target path, so a musl target cannot reach it.

## Acceptance

- `nros-cc-flags` unit tests pin the decision: no `-l` line for `musl`, the
  unchanged lines for `gnu` and for an empty `target_env`.
- The Linux talker cross-builds for `aarch64-unknown-linux-musl` against this
  tree with no local patch.
- `just ci gate` green.
