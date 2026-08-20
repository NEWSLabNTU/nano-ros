---
id: 708
title: "The ThreadX and NuttX boot funnels never call `nros_log::init`, so every library-emitted `nros_*!` record is dropped"
status: open
type: bug
severity: medium
area: platform, testing
related: [issue-0589, issue-0420, issue-0697, rfc-0069]
---

## Measured, not surveyed

`packages/testing/nros-tests/bins/logging-smoke-threadx-linux` emits all six
severities. It also calls `init(sinks::default())` itself. Removing only that
call — so the image relies on what the BOARD installed — and rebuilding:

```
$ ./logging-smoke-threadx-linux
========================================
  nros ThreadX Platform (bare)
========================================
[app_define] Creating byte pool...
[app_define] Running board network init...
[app_define] Creating app thread...
```

**0 of 6 records.** The board boots completely; every record is constructed,
dispatched and dropped. Restoring the fixture's own `init`: 6 of 6.

## Cause

`nros_log::dispatch_to_sinks` returns when `SINKS_PTR` is null, and the Rust
macro path — unlike the C entry point `nros_log_emit`, which calls
`ensure_default_sinks()` — has no lazy install. So a record needs
`nros_log::init` to have run.

Three boards call it at their boot funnel, each having written the same line
with its own comment explaining the same hazard:

| board | site |
|---|---|
| `nros-board-linux` | `lib.rs:283`, `:403` |
| `nros-board-zephyr` | `entry_tiers.rs:326` |
| `nros-board-freertos` | `entry.rs:704` — "without it Node-pkg `nros_info!` output is silently dropped" |

Two FAMILIES do not, and both wire a *different* facade instead, which is what
makes it look handled: ThreadX calls `install_uart_logger` and NuttX
`install_stdout_logger`, both of which install a **`log`-crate** logger.
Bridging `log` and `nros_log` needs `LogCrateSink` inside an `nros_log::init`
list, and that type appears nowhere outside `book/src/user-guide/logging.md`.

Boards affected (verified by delegation, not by grep — see below):

* `nros-board-threadx`, and its leaves `nros-board-threadx-linux`,
  `nros-board-threadx-qemu-riscv64`
* `nros-board-nuttx`, and its leaf `nros-board-nuttx-qemu`

`nros-board-mps2-an385-freertos` is NOT affected: it delegates to
`nros_board_freertos::run_entry`, which does init.

## Why it matters, and why 0420 does not cover it

Issue 0420 asked the neighbouring question — does `nros_platform_log_write`
exist on these platforms — measured it, and answered YES for both families
(NuttX gets it by reuse of the POSIX `platform.c`). That is the half BELOW the
facade. It was measured with the smoke fixtures, which supply their own `init`,
so it could not and did not test the half ABOVE.

The consequence is specific to LIBRARY records. An application that logs can see
its own missing output. A library cannot: issue 0589 deliberately moved the
zenoh session-pool diagnostic from a `cfg(feature = "std")` `eprintln!` to
`nros_log` **so it would reach `no_std` targets** — and on these two families it
reaches nothing at all. The `std` arm was visibly absent; this is silently
dropped, which is worse. Issue 0697 asks for a firmware cell asserting that
message appears on a console; on ThreadX and NuttX that cell cannot pass today,
for reasons having nothing to do with the pool.

## A wrong fix, recorded because it is the tempting one

The obvious patch — put `init_default()` next to the existing
`install_uart_logger::<B>()` / `install_stdout_logger()` calls — was tried and
**does not work**. Those calls live in `spawn_next_tier` and `run_app_thread`,
which are inner helpers; the fixture above boots through `run_bare`, a separate
funnel, and still printed 0 of 6 after the patch. Fixing the call sites of a
neighbouring function is not the same as fixing the funnels.

The funnel set to cover, enumerated:

```
nros-board-threadx/src/entry.rs : run_tiers_entry, run_app_thread, run_entry, run_bare
nros-board-nuttx/src/lib.rs     : run_entry, run_tiers
```

## Direction

1. `nros_log::init_default()` — one named spelling, so this is greppable and
   gate-able rather than five hand-written copies of `init(sinks::default())`
   plus five comments. `nros-board-threadx` and `nros-board-nuttx` need an
   `nros-log` dependency added; neither has one today.
2. Call it at every funnel above, and convert the three existing hand-written
   copies to it.
3. Gate: a board crate exposing a `pub fn run*` boot funnel must reach
   `init_default`. Without this the next board repeats it — five boards have
   now made this decision independently and two got it wrong.
4. Acceptance is the measurement at the top of this issue, as a test rather than
   a one-off: a logging fixture that supplies NO `init` of its own must still
   emit, on every board family. That is the assertion whose absence let this sit.
