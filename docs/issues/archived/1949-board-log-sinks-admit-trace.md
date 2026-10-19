---
id: 1949
title: "Five board `log` sinks admit TRACE with no threshold, so a hosted image writes and flushes a stdout line on every publish"
status: resolved
type: bug
area: [boards, logging]
severity: medium
found: 2026-10-18
related: [1948, 506, phase-462]
---

## Measured

The phase-462 W6a flood-cell rerun (nano-ros-rt-eval, 2026-10-18). Its load
generator, a hosted Rust image publishing at 500 Hz, wrote **2,018 lines,
200,727 bytes in 4 s** to stdout. 1,994 of them were one line:

```
[TRACE] Publishing 106 bytes with attachment: seq=N, ts=N, gid=[..]
```

That is `log::trace!` in `nros-rmw-zenoh/src/shim/publisher.rs` (the
`#[cfg(feature = "std")]` arm of every publish), reaching
`nros-board-linux`'s `install_stdout_log_bridge`.

It cost a whole experiment. The eval driver spawned its load generators with
`stdout=PIPE` and never read it, so at ~50 KB/s per generator each one
filled its 64 KiB pipe in about 1.3 s and blocked in `write()` for the rest
of the run. Every flood cell recorded in the earlier rerun was therefore a
flood for one second and no flood after it. The driver's pipe is the eval's
bug (now `DEVNULL`); the volume is ours.

## Why

`log::set_max_level(LevelFilter::Trace)` with `enabled() -> true` and no
downstream filter, in five board sinks:

- `nros-board-linux/src/lib.rs` (`install_stdout_log_bridge`)
- `nros-board-freertos/src/entry.rs` (`install_uart_logger`)
- `nros-board-mps2-an385/src/log_bridge.rs`
- `nros-board-nuttx/src/lib.rs`
- `nros-board-threadx/src/entry.rs`

`nros-log`'s own bridge (`log_compat::install_log_crate_bridge`) sets the
same `Trace` ceiling, correctly: it dispatches through `nros_log`'s per-logger
thresholds, whose default is `Severity::Info`. The board sinks bypass that,
so every `debug!`/`trace!` in every crate prints — on a hot path, with a
flush per record. ESP32-QEMU is the one board that already sets `Info`.

## Fix direction

Each board sink filters on `nros_log`'s process default threshold
(`Logger::default_level`, `Info` unless an application lowers it), in ONE
shared helper rather than five copies of a `log::Level` → `Severity` map.
Before changing it, find the tests and harnesses that grep `[DEBUG]` /
`[TRACE]` lines (issue 1361's diagnosis quoted this exact line), since a
default of `Info` removes them.

## Resolution (2026-10-19)

One answer for every `log::Log` in the tree:
`nros_log::log_compat::admits(&log::Metadata)`. It is the record's
logger threshold (`get_logger(target).is_enabled(..)`: own level, then
ancestors, then the process default), under the compile-time ceiling. The
nros-log bridge's `enabled` already computed exactly this; it now calls the
helper. All six board sinks call it in BOTH `enabled` and `log`, because the
`log` macros test only `log::max_level()` before calling `Log::log` and never
call `enabled`.

`set_max_level(Trace)` stays on every sink, ESP32-QEMU included, because it
is a ceiling, not a threshold. ESP32-QEMU's `Info` there was the right default
in the wrong place: no later `Logger::set_default_level(Debug)` could get
below it. That makes six sinks, not the five listed above.

Nothing that asserts on output was relying on the old behaviour: no
`nros_tests::output` marker is emitted at `debug`/`trace`. `logging_smoke`'s
`[TRACE]`/`[DEBUG]` lines go through `nros_log` macros and `PlatformSink`,
not these sinks.

Tests: `log_compat::tests::admits_filters_on_the_target_loggers_threshold`,
`bridge_enabled_is_admits`, and
`nros-board-linux::stdout_log_bridge_filters_below_the_default_threshold`,
which asserts that the publish path's own target is refused at `Trace` and
admitted at `Info` once the hosted sink is installed.

