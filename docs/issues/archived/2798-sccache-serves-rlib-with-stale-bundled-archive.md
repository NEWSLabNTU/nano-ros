---
id: 2798
title: "sccache serves a board rlib that bundles the OLD platform archive after a C-port edit"
status: resolved
type: bug
area: [build, testing, threadx]
severity: high
found: 2026-11-21
resolved: 2026-11-23
related: [1646, 0475, 2779]
---

## What fails

A fixture built through `just` has the platform C port's OLD code linked in after
that port's source changed. Cargo sees the change and rebuilds everything above
it, so the build looks fresh and the artifact is a museum binary. Every verdict
from such a fixture describes code the tree no longer holds.

Measured on threadx-linux (`examples/workspaces/rust`, row
`workspace-rust-threadx-linux-listener`), while taking the negative control for
issue 2779. Only `packages/platform/nros-platform-threadx/src/net.c` was toggled
between the issue-2779 fixed and unfixed `nros_platform_tcp_read`:

| `net.c` | built by | listener sha256 | `nros_platform_tcp_read` in the binary |
| --- | --- | --- | --- |
| fixed | `NROS_FIXTURE_ID=<row> just build threadx_linux` | `ce0c4a690df0…` | fixed |
| unfixed | the same recipe | `ce0c4a690df0…` | **fixed** |
| unfixed | the same recipe, `CARGO_TERM_VERBOSE=true` | `b29a7b923a53…` | unfixed |
| unfixed | `nros build pubsub_bringup:threadx_listener --workspace . --offline -- --profile nros-relwithdebinfo --target-dir target-fixtures/threadx-listener` (no `RUSTC_WRAPPER`) | `b29a7b923a53…` | unfixed |

In the stale rows cargo reports `Dirty nros-board-threadx … net.c has changed`.
The build script recompiles `net.o` and `libnros_platform_threadx.a` from the
unfixed source: objdump of both shows the unfixed read. The listener is relinked
after them (archive 13:36:57.33, binary 13:36:57.58). Yet the binary is
byte-identical to the fixed build. `addr2line` puts its EOF branch at
`net.c:333`, a line that exists only in the fixed source. Every run in the stale
rows reported 6 Rust sccache hits and 0 misses.

## Why

The recipe compiles Rust through `scripts/bin/rustc-wrapper/sccache`; the direct
`nros build` above runs without one. `nros-board-threadx`'s build script links
its archives with `cc::Build::link_lib_modifier("+whole-archive")`, so rustc
gets `-l static:+whole-archive=nros_platform_threadx` and BUNDLES the archive
into the crate's rlib. The rlib's sources, `--extern`s and flags are unchanged by
a C edit, so sccache presents the old key and serves the old rlib, still carrying
the old archive. The binary links that archive.

Not yet measured: whether sccache hashes a plain `-l static=` archive and skips
only the modifier spelling, or skips bundled native archives altogether. Issue
1646 says it hashes `-l static=` libs. The `CARGO_TERM_VERBOSE` run missed
because sccache keys on `CARGO_*` variables, which is why it was right.

This is issue 1646 one crate type over. 1646 excluded `staticlib` units from
the wrapper. This unit is an `rlib`, which the wrapper still caches.

## Reach

The sites that BUNDLE a native archive with a modifier:
`git grep -n 'link_lib_modifier' -- '*.rs'`

- `packages/boards/nros-board-threadx/build.rs`: `threadx_kernel` and
  `nros_platform_threadx` (the measured case; ThreadX Linux and riscv64).
- `packages/boards/nros-board-threadx-linux/build.rs`: `nsos_netx` and `glue`.
- `packages/platform/nros-platform-cffi/build.rs:100`.

The `-bundle` sites (NuttX in `nros-board-common`, the Cyclone sys crate,
`nros-platform-cffi:137`) put the archive on the final link instead. A final link
is a bin or staticlib, which sccache or the wrapper does not cache, so they are
outside this.

## Closes when

A C edit under a bundled archive changes the linked image when it is built
through the wrapper. A gate states that for every bundling site, with this
repro as its negative control. The fix is the wrapper's or the key's, never a
wipe: the build dir that reproduces this is
`examples/workspaces/rust/target-fixtures/threadx-listener` in
`nros-wt-2779b` (newslab 118).

## Resolved

**Fix** (85e521ac85, 2026-11-21): `scripts/bin/rustc-wrapper/sccache` runs
uncached any rustc call that bundles a native static library: an
`-l static...` whose modifiers do not include `-bundle`. That is issue 1646's
staticlib rule one crate type down. The rule is derived from the invocation,
so every bundling site is covered, including one added later, with no crate
list.

**Gate:** `just check rustc-wrapper-staticlib` routes three bundled spellings
direct, and keeps two `-bundle` spellings, a dylib, and a crate named
`static` cached. Mutation-checked: the old wrapper fails the three bundled
cases.

**End to end** (2026-11-23, sccache 0.17.0-nros1): a throwaway two-crate
workspace whose library crate's build script compiles `net.c` with
`link_lib_modifier("+whole-archive")`, built `--release` twice through the
wrapper, with `net.c` changed from returning 1 to returning 2 between the
builds:

| wrapper | after the edit, the binary prints | Rust cache hits on the second build |
| --- | --- | --- |
| before 85e521ac85 (negative control) | 1 (the old archive) | 1 |
| current | 2 | 0 |

A debug build does not reproduce it: sccache does not cache an incremental
rustc call. The fixture profiles are release-shaped, which is why the
original build hit it.
