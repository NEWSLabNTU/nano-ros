---
id: 1561
title: "`nros-board-mps3-an536-freertos` could not compile its own board C file,
  because the LAN9118 include lived only in the CMake lane"
status: resolved
type: bug
area: boards, build
severity: medium
found: 2026-09-29
resolved: 2026-09-29
resolved_in: "phase-471 W2"
related: [1309, 1527, phase-385, phase-471]
---

## What this was

`cargo build --target armv8r-none-eabihf` in
`packages/boards/nros-board-mps3-an536-freertos` failed at HEAD `8fe72f097`:

```
c/board_an536.c:52:10: fatal error: lan9118_lwip.h: No such file or directory
   52 | #include "lan9118_lwip.h"
```

`board_an536.c` carries strong netif overrides that include the LAN9118
driver's header. The board's `build.rs` was copied from
`nros-board-s32z270-freertos`, whose netif is consumer-side and which therefore
adds no such include — so the overlay compiled the C file with an include set
that could never resolve it.

The CMake lane knows: `cmake/board/nano-ros-board-mps3-an536-freertos.cmake`
puts `${_NROS_LAN9118_DIR}/include` in `FREERTOS_STARTUP_INCLUDES` and says why
in a comment ("board_an536.c includes lan9118_lwip.h for its strong netif
overrides — the S32Z270 overlay has no equivalent because its netif is
consumer-side"). That fact was stated in one of the two build systems that
compile this board's C and not in the other.

## Why nobody saw it

The crate is in the root `Cargo.toml`'s `exclude` list (cross-only), and no
`[[fixture]]` row cargo-builds it — the `workspace-cpp-mps3-an536-freertos` row
is a C++ entry whose board C is compiled by CMake. So no lane on any event ever
asked this build script to run. Same shape as issue 1309's two crates that were
in `exclude` and therefore compiled by nothing.

It was found by priming a negative control: phase-471 W2 built each FreeRTOS
overlay crate BEFORE refactoring it, to have a byte-comparison baseline. Two of
the three produced one; this one produced a compiler error instead.

## The fix

phase-471 W2's `freertos_build::run_overlay` takes a `configure_glue` hook for
exactly the includes/defines a board adds to its own glue TU set, and this
board now passes the LAN9118 include through it — one line, stating in the
crate what the CMake board file already stated in CMake.

## What is still true after it

The LAN9118 driver `.c` itself is still not compiled on the cargo side for this
board (CMake builds it as the `lan9118_lwip` target and links it). That is
correct for the C/C++ image, which is the only image this board has; a
pure-Rust AN536 entry — there is none today — would need the archive too, the
way `nros-board-mps2-an385-freertos` compiles it. The cargo build now produces
an rlib with unresolved `lan9118_*` references, which is what an rlib is for.

## Acceptance

`cargo build --target armv8r-none-eabihf` in the crate, which failed at HEAD and
succeeds now. There is no before/after image comparison for this board, because
there was no "before" build.
