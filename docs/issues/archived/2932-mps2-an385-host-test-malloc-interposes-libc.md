---
id: 2932
title: "The MPS2-AN385 platform crate's host unit test replaces libc's malloc with its single-threaded bare-metal heap, and once aborted with an rlsf \"bitmap contradiction\""
status: resolved
type: bug
area: [platform, testing, memory]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
resolved_in: "check-host-libc-symbols + target_os = \"none\" on every libc stub"
related: [594, 1640]
---

## What happened

On 2026-11-24 a full `just ci gate` on host 142 (aarch64 Linux, 80 cores) failed one
gate, `build::check::bare-metal-wall-clock`, which runs
`cargo test --lib wall_clock` in `nros-platform-stm32f4` and then in
`nros-platform-mps2-an385`. The stm32f4 test passed. The mps2-an385 test binary
aborted:

```
running 1 test
thread 'main' (2683737) panicked at .../rlsf-0.2.3/src/tlsf.rs:902:17:
bitmap contradiction
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

thread 'main' (2683737) panicked at .../rlsf-0.2.3/src/tlsf.rs:902:17:
bitmap contradiction
panicked at .../rlsf-0.2.3/src/tlsf.rs:902:17:
bitmap contradiction
thread panicked while processing panic. aborting.
error: test failed, to rerun pass `--lib`

Caused by:
  process didn't exit successfully: `.../nros-platform-mps2-an385/target/debug/deps/nros_platform_mps2_an385-8b296fd420efcd98 wall_clock --quiet` (signal: 6, SIGABRT: process abort signal)
```

The tree was the phase-591 W6/W8 branch at 00ceafa032 (main d5c0c0d5b4 plus
CI-only changes that touch no platform crate). The gate ran 2026-11-24 about
10:45-11:06 +08:00. The host's load average just after was 20 / 31 / 41
(1 / 5 / 15 min); a 2840 disk measurement and other sessions' work shared the
host. The gate run before it, on the same tree minus one script edit, passed
this test, and the test then passed **20 of 20** run solo on the same host.

`rlsf` panics "bitmap contradiction" when its free-list bitmap and its lists
disagree, i.e. the allocator's own state is corrupt. That is a memory-safety
signal, not a timing flake, however rarely it fires.

## Why it happens (measured on the failing binary)

`src/libc_stubs.rs` defines `malloc`, `free`, `realloc` and `calloc` with
`#[unsafe(no_mangle)]`, always, and routes them to `crate::memory`'s
`FreeListHeap` (an `rlsf` TLSF arena). Its comment says this is safe because
"the mps2-an385 platform is bare-metal-only (`target_os = "none"`), so there's
no hosted libc to collide with".

The `bare-metal-wall-clock` gate builds this crate for the HOST, where there is
a libc to collide with. In the failing test binary:

- `nm` shows `T malloc`, `T free`, `T calloc`, `T realloc` defined by the
  binary itself, so they interpose glibc's for the whole process;
- `__rust_alloc` branches to `__rdl_alloc` (the default "system" allocator),
  which calls `malloc`, so every Rust allocation in the process lands in the
  board arena too.

So the test harness's own allocations, the stdout capture, the test thread and
`main`, all go through `FreeListHeap`, whose `Sync` impl says:
"Safety: bare-metal single-threaded" (`packages/rmw/zenoh/zpico-alloc/src/lib.rs`).
libtest runs the test on a spawned thread while `main` waits and prints, so two
threads can enter the arena at once. A race there corrupts the TLSF bitmap, and
the panic's own formatting allocates from the same corrupt arena, hence the
"panicked while processing panic" abort.

`nros-platform-stm32f4` defines no `malloc` (no `libc_stubs.rs`), which fits
it passing in the same gate. This is the only crate under `packages/` that
exports `malloc` (`git grep -n 'extern "C" fn malloc' -- packages`).

## Why it matters

- Every host test of this crate runs on an allocator that is unsound off the
  target. A green run says nothing about the heap; a red one looks like a flake.
- The same shim on a target with more than one context (an ISR that allocates,
  a second core) would have the same race. The comment's premise is about the
  host, but the `Sync` claim is the load-bearing part.

## Directions (not decided)

1. Emit the shims only where the comment's premise holds:
   `#[cfg(target_os = "none")]` on `libc_stubs` (or on the `no_mangle`), so a
   host build links glibc's allocator. Then sweep for any other host-built crate
   that exports libc names (`nm` over the gate's test binaries).
2. Or make the arena safe for its callers (a critical section around
   `FreeListHeap`), if a target needs the shims from more than one context.
3. A gate: a host test binary built from a bare-metal platform crate must not
   define `malloc`/`free`/`calloc`/`realloc`.

## Closes when

The mps2-an385 host test binary no longer defines the libc allocator symbols
(or the arena is shown safe for concurrent callers), and a check holds that
for every bare-metal platform crate's host build.

## Resolution

Fixed 2026-11-24, both directions, and the class rather than the site.

**The exports are bare-metal only.** Every `no_mangle` libc stand-in under
`packages/` is now compiled only for `target_os = "none"`:

- `nros-platform-mps2-an385`: the `libc_stubs` module (`malloc`/`free`/
  `realloc`/`calloc`). Its comment's "bare-metal-only, no hosted libc to
  collide with" premise is corrected: the image is bare metal, the crate is
  also built for the host.
- `nros-baremetal-common`: its `libc_stubs` module (`memcpy`, `strlen`,
  `strtol`, `snprintf`, ...), which `nros-platform-stm32f4` and mps2-an385
  enable. On the host these replaced glibc's string functions the same way.
- `qemu-smoltcp-bridge` (`qemu-rs-common`): its `libc_stubs` module.
- `nros-board-threadx-qemu-riscv64`: its `__assert_func`. The crate builds as
  a host shell so host tooling can depend on it, and that shell exported it.

The last three were found by the new gate, not by reading. Before cfg-gating,
consumers were checked on a target that is not `none`: every board on these
platforms targets `thumbv7m-none-eabi`, `armv8r-none-eabihf` or
`riscv64gc-unknown-none-elf` (all `target_os = "none"`), and nothing reaches the
stubs by path (`git grep 'libc_stubs::'`). The cross builds still define them:
`nm` on the thumbv7m rlibs shows `T malloc/free/calloc/realloc`
(mps2-an385) and `T memcpy/strlen/strtol` (baremetal-common). The riscv64 board
cross build needs ThreadX provisioned and was not run here. Its target's
`target_os` is `"none"` (`rustc --print cfg`). `just check
bare-metal-wall-clock` passes. Each new predicate is classified in
`check-rtos-target-os.py` ("is there a libc?", not "is this hosted?").

**The gate.** `just check host-libc-symbols`
(`scripts/check/check-host-libc-symbols.py`, build tier) derives three things:

- the NAMES: the allocator four plus every `no_mangle` `extern "C" fn` in any
  `libc_stubs.rs`;
- the CRATES: every crate under `packages/` that exports one;
- the host BUILD of each crate: its `--lib` artifacts, or its host unit-test
  binary when the library alone does not build for the host (mps2-an385's
  `asm!`).

It fails if `nm --defined-only` finds a name defined in any of them. Its
self-test runs on every invocation. As a negative control, it compiles a C
object that defines `malloc` and one that does not, and requires exactly the
first to be flagged. On the unfixed tree it named the two crates above; on the
fixed tree, all 4 crates and 22 names are clean.
