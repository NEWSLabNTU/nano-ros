---
id: 1988
title: "Three tier-2 tests grep for `log::trace!`/`debug!` markers that #1949's threshold now drops, and `RUST_LOG=trace` no longer brings them back"
status: resolved
type: bug
area: [testing, logging, zenoh]
severity: medium
found: 2026-10-20
related: [1949, 0429]
---

## Measured

- Run: run-matrix 39380448771 (tier 2, schedule, 2026-10-20 06:27 UTC, main
  at `22a482fae`).
- Job: 118488124531, `tier 2 (1-wise matrix)`, step `just ci matrix`.
  `Real failures: 3 / 3 total failures` (from the junit rewrite). The other
  305 raw nextest failures were `skip!`s.
- The three, from the job's `post-submit-junit` artifact:

```
nros-tests::nano2nano test_gid_consistency           (20.4 s)
nros-tests::nano2nano test_sequence_number_increment (20.4 s)
  panicked at packages/testing/nros-tests/tests/nano2nano.rs:303:13:
  talker emitted fewer than 2 MessageInfo trace lines: Process failed:
  native-rs-talker did not print `with attachment:` 2 time(s) within 20s

nros-tests::qos test_qos_keyexpr_encoding            (1.3 s)
  panicked at packages/testing/nros-tests/tests/qos.rs:507:5:
  Should log liveliness keyexpr with QoS info
```

The talker kept publishing (`[INFO] Publishing: 'Hello World: 19'`). Only
the markers are missing.

## Cause

Both markers come from the `log` crate at levels below `Info`:

- `with attachment:` is `log::trace!` in
  `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/publisher.rs` (the publish
  path, `#[cfg(feature = "std")]`).
- `liveliness keyexpr:` is `log::debug!` in
  `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`.

The tests ask for them by spawning the native talker with `RUST_LOG=trace`.
Before `19dda97dd` (issue 1949, merged 2026-10-19), every board `log::Log`
sink answered `enabled() -> true`, so these lines printed whatever
`RUST_LOG` said. Since that commit, each sink asks
`nros_log::log_compat::admits`, and that answers from the logger threshold:
the process default is `Info`. Nothing in nros-log or `nros-board-linux`
reads `RUST_LOG`, so the variable the tests set has had no effect since
that commit. The commit message says "No test marker is emitted at
debug/trace". These three tests show that is not true.

Yesterday's tier 2 run (39210994277, at `c75d1265e`) passed all three. It
predates `19dda97dd`. This is the first tier 2 run on a tree that contains
it.

## What it is NOT

- Not a delivery or GID defect: the talker publishes, and the tests fail
  before they compare anything.
- Not a flake: it is deterministic. Every native talker image built after
  `19dda97dd` drops these lines.
- Not issue 0429. That was the same test grepping the LISTENER for a marker
  the binary had stopped emitting. Here the marker is still emitted, and the
  sink now filters it out.
- Not issue 1983 (the `rmw_coordinate_truth` timeout). That test did not
  fail in this run.

## What would close it

Two sites are defensible:

1. **The host board honours a runtime level again.** `nros-board-linux`, or
   nros-log's `std` arm, sets the default threshold from `RUST_LOG` (or a
   named `NROS_LOG_LEVEL`) at init. This keeps #1949's quiet default and
   restores the opt-in the tests rely on.
2. **The tests stop grepping log text for facts.** 0429's lesson, applied
   to the talker: read the sequence number, GID and QoS keyexpr from an
   observable (a received `MessageInfo`, or a liveliness query) rather than
   from a trace line. Log text is formatting, not an interface.

Whichever lands, a check should cover a test that sets `RUST_LOG` against
an image that ignores it, so the next threshold change cannot silence a
marker unnoticed.

## Resolution (2026-10-22)

Site 1: the host board honours a runtime level again. `nros-board-linux`'s
stdout `log` sink, when it is the sink installed, reads `RUST_LOG` and moves
nros-log's process default threshold (`Logger::set_default_level`) to the bare
level it states — env_logger's global directive, last one winning, `off` as
`Fatal`. #1949's quiet `Info` default is unchanged when `RUST_LOG` is unset. An
application that installed `env_logger` keeps reading the variable itself.

Per-target directives (`nros_rmw_zenoh=trace`) are deliberately ignored, and
the parser's doc says why: a `log` target is a `::` module path while nros-log's
hierarchy is dotted, so an exact-name logger would match one module rather than
the prefix env_logger means — a directive that half-works reads as working.

Verified: `rust_log_states_a_default_level_only_through_a_bare_level` (unit,
pure, so it does not move the process default the 1949 test reads). End to end
on fresh `just build-test-fixtures lane=linux` fixtures, the three tests this
issue measured pass: `test_qos_keyexpr_encoding`, `test_gid_consistency`,
`test_sequence_number_increment`, 3/3. The same three fail on `main` in
run-matrix 39555555017, both tiers.

Not done: site 2 (the tests reading facts from an observable rather than log
text) and the check the issue asks for. The three tier-2 tests ARE that check
now — each sets `RUST_LOG` against an image that must honour it — and they run
in the scheduled lane that caught this.
