---
id: 708
title: "The ThreadX and NuttX boot funnels never call `nros_log::init`, so every library-emitted `nros_*!` record is dropped"
status: resolved
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

## Resolution

`nros_log::init_default()` — one named spelling — called at every board boot
funnel, plus a per-funnel gate.

**The funnel set was bigger than this issue first said, in two directions.**
Writing the gate is what found the rest:

| crate | funnels fixed |
| --- | --- |
| `nros-board-threadx` | `run_entry`, `run_tiers_entry`, `run_app_thread`, `run_bare` |
| `nros-board-nuttx` | `run_entry`, `run_tiers` |
| **`nros-board-freertos`** | **`run_entry`, `run_bare`** |
| **`nros-board-esp32-qemu`** | **`run_bare`** |
| **`nros-board-mps2-an385`** | **`run_bare`** (found by the gate, after I thought I was done) |

The bolded rows are the correction. This issue named FreeRTOS as NOT affected
because `nros-board-freertos` contains the call — it does, in
`run_tiers_entry`, and 2 of its 3 funnels had none. `nros-board-mps2-an385` had
it in `entry.rs` and `rtic.rs` and not in `node.rs`. So of the five boards that
made this decision independently, **three got it partly wrong**, and every
per-crate reading of the tree — including mine, twice — passes them.

That is why the gate is per-FUNNEL. `check-board-log-sink.py` requires every
`pub fn run*` in a board crate to reach `init_default`, directly or by
delegating to a funnel that does (`nros-board-threadx-qemu-riscv64::run_app_thread`
forwards, and is credited). `nros-board-common` is excluded by path with the
reason recorded: its `run*` functions drive image links at BUILD time.

Mutation-checked: reverting `nros-board-threadx::run_bare` to its shipped state
fails the gate naming that exact line. The script's `--self-test` covers the three
shapes it needed to learn — a multi-line signature, delegation, and a
neighbouring `install_uart_logger` NOT counting.

### Acceptance, as a test rather than a one-off

`logging-smoke-threadx-linux` no longer publishes its own sink list. It used to,
which is precisely why it proved the platform half (0420's question) and nothing
about the board half. Now it emits 6 of 6 only because the funnel published one,
so the assertion is about the board. The fixture carries a comment saying not to
"fix" a future silence by adding an `init` back.

Measured: 0 of 6 before, 6 of 6 after, same fixture, same host, freshly compiled
both times (the first attempt at this measurement read a stale binary from a
failed build and had to be redone).

### Also folded in

The five surviving hand-written `init(sinks::default())` copies — each with its
own comment explaining the same hazard — now call `init_default()`. Six leaf
lockfiles were regenerated through `just lock-update` for the new `nros-log`
dependency on `nros-board-threadx` / `nros-board-nuttx`; the diff is six lines
and zero added packages.

### Not done

Only the ThreadX-linux fixture's acceptance was executed here — it is the one
host-runnable board. The other six `logging-smoke-*` fixtures still publish
their own sink list, so they still cannot detect a regression in their board's
funnel. Converting them is the same one-line change each, but it needs their
QEMU lanes to verify and those were not run. Worth a follow-up.

## Direction (as filed)

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
