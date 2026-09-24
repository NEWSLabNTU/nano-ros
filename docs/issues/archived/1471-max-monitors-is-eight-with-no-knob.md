---
id: 1471
title: "`MAX_MONITORS` is a hard-coded 8 with no rung on any knob ladder, and
  the C++ road bakes 14 monitor rows for the Autoware Safety Island -- the
  install REFUSES, so the entry's setup returns -6 before it creates a node"
status: resolved
type: bug
area: [core, api, cli]
severity: high
found: 2026-09-24
related: [0810, 1198, phase-462, phase-467]
resolved_in: phase-467 W1
---

## Resolution (phase-467 W1)

`MAX_MONITORS` is now two generated consts on the same ladder as `MAX_CBS`
/ `MAX_SC` / `MAX_NODES`, one per table:

| const | explicit knob | derived rung | counted by |
| --- | --- | --- | --- |
| `MAX_MONITORS` | `NROS_EXECUTOR_MAX_MONITORS` | `NROS_DECLARED_EXECUTOR_MAX_MONITORS` | `monitor_rows(model).len()` |
| `MAX_AGE_MONITORS` | `NROS_EXECUTOR_MAX_AGE_MONITORS` | `NROS_DECLARED_EXECUTOR_MAX_AGE_MONITORS` | `age_rows(model).len()` |

The entity inventory counts both from the model with the same two functions
the entry emitters bake the tables with, and carries them on all three roads
(Zephyr resolver with a `-1` Kconfig sentinel, the cmake declared road, the
cargo sidecar). No model means no count, and the crate default of 8 stands,
so an image without a contract is byte-identical. Two knobs, not one: the
island's 14 rate rows and 0 age rows size 14 and 0.

The refusal stays and now names the knob: `monitor::check_table_capacity`
returns `MonitorTableFull`, whose Display says which knob to raise; the C++
install logs it before `NROS_CPP_RET_FULL`, the generated Rust table carries
a compile-time assertion with the same words, and `Executor::set_monitor_table`
panics in a debug build rather than truncating. See
`docs/roadmap/phase-467-board-files-state-board-facts.md`.

The original report follows unchanged.

## What happens

`nros_cpp_install_monitors` refuses a table with more rows than the executor
can watch, and the generated C++ setup returns that refusal:

```rust
// packages/api/nros-cpp/src/lib.rs:3965
if t.n_rows > nros_node::executor::monitor::MAX_MONITORS
    || t.n_ages > nros_node::executor::monitor::MAX_MONITORS
{
    return NROS_CPP_RET_FULL;
}
```

```jinja
{# packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/monitor_install.jinja #}
nros_cpp_ret_t __mret = nros_cpp_install_monitors(__mexec, &__mtables);
if (__mret != NROS_CPP_RET_OK) return static_cast<int32_t>(__mret);
```

The block is rendered INSIDE the setup function, before the executor's nodes
are created (a publisher attaches its counter cell at create time, so a table
installed later would monitor nothing). So the consequence is not a warning
and not a degraded image: setup returns `NROS_CPP_RET_FULL`, which is `-6`
(`packages/api/nros-cpp/include/nros/nros_cpp_ffi.h:846`), and the image never
reaches the spin loop.

`MAX_MONITORS` is 8:

```rust
// packages/core/nros-node/src/executor/monitor.rs:202
/// Max monitored endpoints per executor (const table, no_std).
pub const MAX_MONITORS: usize = 8;
```

The Autoware Safety Island bakes 14 rows.

## The count, recomputed against the contract

One monitor row per contracted publisher endpoint carrying `min_rate_hz`
(`monitor_rows`, `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`,
around 1341-1398), keyed by endpoint ref, with latency-only rows merged in
from node paths carrying `max_latency_ms`. A separate age table comes from
`sub_endpoints` carrying `max_age_ms` (`age_rows`, around 1403-1427). Both are
filtered per entry by `monitor_table_view`
(`packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`, around 234-264),
which keeps the rows of the nodes THAT entry constructs.

Counted from
`src/safety_island_bringup/launch/safety_island.contract.yaml` in the
simple-autoware-safety-island tree, at its working state on 2026-09-24:

| node | `pub:` endpoints with `min_rate_hz` |
| --- | --- |
| `mrm_comfortable_stop_operator` | 3 (`max_velocity_candidates`, `clear_velocity_limit`, `status`) |
| `mrm_emergency_stop_operator` | 2 (`emergency_control_cmd`, `status`) |
| `mrm_handler` | 5 (`mrm_state`, `gear_cmd_out`, `hazard_lights_cmd`, `turn_indicators_cmd`, `emergency_holding`) |
| `stop_mode_operator` | 4 (`control`, `gear`, `hazard_lights`, `turn_indicators`) |
| **total** | **14** |

The three node paths that declare `max_latency` (`mrm_emergency_stop_operator
/on_timer`, `mrm_handler/on_timer`, `mrm_handler/call_mrm`) list only output
endpoints that ALREADY carry `min_rate_hz`, so they merge into existing rows
and add none: the total stays 14. No subscriber in the file declares
`max_age`, so the age table is 0 rows and its half of the check passes.

The island's launch declares no realtime tiers, so `monitor_table_single` is
the shape that applies and all 14 rows land on ONE executor. 14 > 8.

## It refuses rather than truncates, deliberately

The doc comment above the export says why, and it describes this island by
name and by number:

> Rows beyond the executor's `MAX_MONITORS` are REFUSED (`NROS_CPP_RET_FULL`)
> rather than truncated: the executor checks only the first `MAX_MONITORS`
> specs of a table, and an image that boots with six of its fourteen
> contracts silently unwatched is the class of failure this table exists to
> remove. A short or misaligned storage buffer is refused the same way.

(`packages/api/nros-cpp/src/lib.rs:3941-3945`, mirrored at
`packages/api/nros-cpp/include/nros/nros_cpp_ffi.h:1698-1699`.)

That reasoning is correct and this issue does not argue with it. The spin
loop really does inspect only the first `MAX_MONITORS` entries -- three
`.take(super::monitor::MAX_MONITORS)` sites in
`packages/core/nros-node/src/executor/spin.rs` (3009, 3041, 11417) -- so a
truncating install would produce exactly the silent image the comment
describes. Six of fourteen is `14 - MAX_MONITORS`. The refusal is the right
behaviour for a bound that cannot move. The defect is that the bound cannot
move.

## It has no rung on any knob ladder, and its siblings all do

`MAX_MONITORS` is a plain source constant. There is no `NROS_EXECUTOR_*`
environment knob, no `CONFIG_NROS_*` Kconfig symbol, no CMake cache entry and
no descriptor field anywhere in the tree -- a repository-wide search for the
name returns only the definition, its use sites in `spin.rs`, the two ABI
doc-comment copies, the refusal in `lib.rs`, and prose in
`docs/roadmap/phase-462-safety-vocabulary-on-target.md` and
`docs/issues/archived/0268-*`.

Every other executor table in the SAME crate is on the ladder.
`packages/core/nros-node/build.rs` generates `MAX_CBS`, `MAX_SC`,
`MAX_NODES` and the action-client count from
`NROS_EXECUTOR_* / NROS_DECLARED_EXECUTOR_*` pairs (lines 507, 515, 601,
642), which is RFC-0049's four-rung precedence and RFC-0100's derivation
story. `MAX_MONITORS` is the one that never got a rung, and it is the one
whose correct value is the most mechanically derivable of the set: the CLI
has already computed `monitor_rows(model).len()` and `age_rows(model).len()`
by the time it emits the entry.

## Fix direction

A rung of the same shape as its neighbours, not a bigger literal. Concretely:
move `MAX_MONITORS` into `nros-node`'s generated config beside `MAX_CBS` and
`MAX_SC`, give it the explicit knob and the derived knob the ladder expects,
and have the entity inventory emit the derived value from the row counts it
already has. Where exactly the derived rung is published (the sizing
descriptor, the inventory, the entry lowering) is a question for whoever takes
it; this issue does not pick.

Two things the fix should preserve:

- The refusal stays. A knob makes the bound settable, which is the point; it
  does not make truncation acceptable, and the refusal message should say
  which knob to raise (RFC-0065 D2, refuse and name the remedy).
- The separate `n_rows` and `n_ages` checks are both against the same
  constant today. Whether one knob or two is correct is worth deciding
  rather than inheriting -- the island's age table is empty while its rate
  table overflows, so a single knob prices storage for a table it does not
  have.

## The ratchet consideration

This is a sizing knob, so raising it is not free and the repository already
knows the shape of that cost. The executor's inline state arrays are sized by
`MAX_MONITORS` directly:

```rust
// packages/core/nros-node/src/executor/spin.rs:1554, 1565
pub(crate) monitor_states: [super::monitor::MonitorState; super::monitor::MAX_MONITORS],
pub(crate) age_states: [super::monitor::AgeState; super::monitor::MAX_MONITORS],
```

so every image pays for the knob's value whether or not it declares that many
rows, exactly as issue 1198 describes for `MAX_NODES` and `MAX_SC` and issue
0810 for the action-client arena. A DERIVED rung is what keeps that honest:
an image with 14 contracted publishers sizes for 14, and an image with two
sizes for two. Raising the DEFAULT from 8 would be the wrong fix for the same
reason 1198 is open -- it would move the constant cost onto every board image
in the tree to unblock one.

`just mem-report` should be run across the fix, because the C++ residue figure
phase-462 W1 measured (88 B on the contracted C++ twin) was taken at
`MAX_MONITORS = 8`.

## Status of the evidence

Everything above is read off the source at `5e09e377a` and computed from the
island's contract file. The `-6` return has NOT been observed on a board or in
`native_sim`: no image was built for this issue, because the count and the
comparison are both static and the C++ road's row count is a pure function of
the contract. The first real boot attempt of the island's C++ image is what
would turn this from a derivation into a log line.

## How it was found

phase-462 W1 recorded it as a follow-up when it landed the C++ monitor table
(`docs/roadmap/phase-462-safety-vocabulary-on-target.md:146`: "MAX_MONITORS =
8 is below the island's 14 rows (install refuses, not truncates)"). Filed here
with the count recomputed from the contract, because the follow-up line in a
phase document is not something anyone searching the issue tracker will find.
