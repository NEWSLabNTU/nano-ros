---
id: 674
title: "The threadx-riscv64 Cyclone fixture fails to link: `undefined symbol: stdout` / `stderr`, because startup.c defines them only under `#if defined(__PICOLIBC__)` while the link supplies picolibc"
status: open
type: bug
severity: high
area: build, boards
related: [issue-0657, issue-0664, phase-251]
---

## Symptom

`just build-test-fixtures lane=tier2` fails the whole `threadx_riscv64`
platform:

```
rust-lld: error: undefined symbol: stderr
>>> referenced by main.c
>>>               CMakeFiles/c_listener.dir/src/main.c.obj:(subscription_callback)
>>> referenced by descriptors.cpp
>>>               descriptors.cpp.obj:(nros_rmw_cyclonedds::find_descriptor(char const*))
>>>               in archive .../libnros_rmw_cyclonedds.a
>>> referenced 5 more times

rust-lld: error: undefined symbol: stdout
>>> referenced by printf.c:42 (../../../newlib/libc/tinystdio/printf.c:42)
>>>               libc_tinystdio_printf.c.o:(printf) in archive
>>>               /usr/lib/picolibc/riscv64-unknown-elf/lib/rv64imafdc/lp64d/libc.a
>>> referenced by ddsi_config.c
>>>               ddsi_config.c.obj:(ddsi_config_fini) in archive .../libddsc.a
```

Both `threadx-riscv64-c-cyclonedds` rows (`fixture-0000`, `fixture-0001`)
die; `== threadx_riscv64 == FAILED (rc=2)`. Every other platform in the same
lane passed (`zephyr`, `threadx_linux`, `freertos`, `qemu` all OK).

## The definitions exist, and are conditional

`packages/boards/nros-board-threadx-qemu-riscv64/startup.c` (crate root, NOT
the `c/` subdir — the comment in `c/syscalls.c` saying "the board's
`startup.c`" is correct but easy to misread as a sibling):

```c
#if defined(__PICOLIBC__)
static int _uart_put(char c, FILE *f) { (void)f; uart_putc((int)c); return 0; }
static FILE _uart_file = FDEV_SETUP_STREAM(_uart_put, NULL, NULL, _FDEV_SETUP_WRITE);
FILE *const stdout = &_uart_file;
FILE *const stderr = &_uart_file;
#endif /* __PICOLIBC__ */
```

That guard is [issue 0657](0657-riscv64-lane-cannot-use-provisioned-toolchain.md)'s
work, and its reasoning is sound as written: this board now builds with EITHER
Debian's `picolibc-riscv64-unknown-elf` OR the newlib in the xPack
`riscv-none-elf` toolchain `nros setup` provisions, and

> picolibc declares `stdout`/`stderr` as UNDEFINED externs and expects the
> image to define them … newlib defines them itself (macros over its
> reentrancy struct), so a definition here is a syntax error before it is a
> duplicate symbol.

So the definitions are compiled only when the preprocessor believes it is
building against picolibc.

## The contradiction

The LINK is unambiguously picolibc — the undefined `stdout` is referenced from
`/usr/lib/picolibc/riscv64-unknown-elf/lib/rv64imafdc/lp64d/libc.a`'s own
`printf.c`. So at link time picolibc is supplying libc, while at compile time
`__PICOLIBC__` was evidently NOT defined for `startup.c`, or the definitions
would be present.

That is a **compile-time / link-time C-library disagreement**: one half of the
build thinks newlib, the other supplies picolibc. `cmake/toolchain/riscv64-threadx.cmake`
is where the two are chosen — it resolves the compiler by prefix
(`_nros_riscv64_find_prefix`, which #657 taught to prefer a provisioned
toolchain) while separately forcing picolibc's headers onto every target
(`-isystem ${_RISCV_THREADX_PICOLIBC_SYSROOT}/include`, with a hardcoded
Debian fallback path) and linking picolibc's libc.

## What is verified, and what is not

**Verified:**

* the failure, reproducibly, on `lane=tier2`;
* only `threadx-riscv64-c-cyclonedds` fails; the other four platforms in the
  lane build clean;
* `startup.c` does define both symbols, behind `#if defined(__PICOLIBC__)`;
* the link pulls Debian picolibc's `libc.a`;
* `startup.c` is compiled by CMake (`cmake/board/nano-ros-board-riscv64-qemu.cmake`
  sets `_NROS_BOARD_STARTUP_C`), not by the crate's `build.rs`.

**NOT verified — do not treat as diagnosis:**

* that `__PICOLIBC__` is genuinely undefined for that TU. It is the only
  explanation consistent with the evidence above, but it was not confirmed by
  dumping the preprocessor state (`-dM -E` on `startup.c` with the real flag
  set), which is the first thing the fix should do;
* WHICH compiler CMake actually resolved on this host;
* whether this also breaks the zenoh variant — no zenoh riscv64 row ran in
  this lane, so the Cyclone-only appearance may be a coverage artifact rather
  than a property of the bug. Cyclone references `stderr` from
  `ddsi_config.c` and `descriptors.cpp`, so it may simply be the first
  consumer to reference what was always missing.

## Not caused by the change that found it

Surfaced by a tier-2 sweep run for
[issue 0671](archived/0671-contract-monitor-reports-nothing-on-diagnostics.md),
whose commit touches exactly one file — `nros-node/src/executor/spin.rs`, a
Rust epoch guard. That cannot produce an undefined C symbol in a picolibc
link.

The suspicious neighbourhood is #0657 (`a19e1fdfb`, which rewrote both the
riscv64 toolchain file and added the `__PICOLIBC__` guard) and #0664
(`aa23718a9`, +48 lines in `c/syscalls.c` and a `link.lds` change) — but which
of them, or whether the combination, is exactly what "not verified" above
covers.

## Direction

1. **Dump the preprocessor state first.** `-dM -E` on `startup.c` with the
   flags CMake really passes, and print the resolved `CMAKE_C_COMPILER`. That
   settles compile-vs-link in one step instead of reasoning from a link error.
2. **Make the disagreement impossible rather than conditional.** A guard on
   `__PICOLIBC__` silently produces an image with no stdio when the macro is
   absent; the failure then lands on whichever consumer happens to reference
   `stderr` first. Either assert the C library at configure time (fail loudly
   when the compiler's libc and the linked libc differ), or route the console
   through a symbol this board always defines rather than through libc's
   `stdout`/`stderr`.
3. **Give the zenoh riscv64 variant a lane row** if it does not have one, so
   "only Cyclone fails" stops being unfalsifiable.
