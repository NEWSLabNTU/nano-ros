---
id: 2778
title: "Three tests read a process-global log capture right after logging, so under a loaded parallel run a held or lost record fails them"
status: open
type: bug
severity: low
area: [testing, log]
related: [issue-2000, issue-2082, issue-2102]
found: 2026-11-21
---

## What happens

On a loaded `just ci gate` (load ~40), `node-std-tests` failed in the
`--features alloc,log,sim-time,param-store` run:

```
executor::tests::a_declaration_past_the_stock_carve_is_refused_naming_the_knob
no FULL line was logged; saw ["Raise `max_parameters` under [knobs.params] in system.toml, NROS_MAX_PARAMETERS or CONFIG_NROS_MAX_PARAMETERS."]
```

The capture saw a knob line, which sibling tests also log, but not the test's
own "parameter store is full" line. The same run passes on its own: 4/4 on the
failing head, and 20/20 at `--test-threads=64` on a quieter host. The race
needs load, so that stress is not a working negative control.

## Why

`nros_log` delivers to its sinks under one process-wide dispatch guard. A record
raised while another thread holds the guard is held, and the holder delivers
it. A record that does not fit the hold queue is lost and counted (issue 2000).
So a test that reads its capture straight after logging can read before its
record arrives, or after it was dropped. `nros_log::early::emit_until_delivered`
exists for exactly this, and the sibling test
`a_full_store_reports_the_refusal_and_names_its_knob` was moved onto it for
issue 2000. This one, added later (phase-556 W13), was not.

## Fix

The sweep of every in-process `LogSink` capture in `nros-node`, `nros-c` and
`nros-cpp` (`git grep -n "LogSink for" -- packages/core/nros-node/src packages/api`):

- `a_declaration_past_the_stock_carve_is_refused_naming_the_knob`: waits
  through `emit_until_delivered` for its own FULL line, keyed by its parameter
  name, and both knob lines. The report is repeatable.
- `an_exhausted_arena_decodes_to_the_knob_an_operator_must_set`: the same race
  on its "arena exhausted" line, and the same fix. A repeated failed allocation
  reports again, and the boot record keeps only the first.
- nros-cpp `two_cpp_nodes_emit_under_their_own_names`: counts its records, so a
  re-raise would add one. It now drives delivery with a bounded `flush` loop
  instead.

Already safe: the other three `nros-node` captures (two through
`emit_until_delivered`, one through a bounded `flush` wait), `nros-c`'s board
sink (not a test capture), and `boot_report`'s recorder (it counts errors and
asserts nothing on delivery).
