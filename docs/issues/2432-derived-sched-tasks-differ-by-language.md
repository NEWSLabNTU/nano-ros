---
id: 2432
title: "The same workspace and contract get a derived rate-monotonic schedule in C and C++ and none in Rust, silently, so the Rust image's 100 Hz loop misses its periods"
status: open
type: bug
severity: medium
area: [scheduling, codegen, cli, macros]
related: [rfc-0079, 2269]
found: 2026-11-11
---

Found while building the two-tier lab of issue 2269 on `mps2-an385-freertos`.
**This is a parity defect, and it is silent.** No build or boot line says
that the Rust image took a different schedule from its C and C++ twins.

## Measured

2026-11-11, `main` after #3508 (cb799bc93b).

One workspace shape in all three languages:
- `nros new control_ws --board mps2-an385-freertos --template talker --lang <L>`.
- `nros new telemetry_node --template talker --lang <L> --to control_node_bringup`.
- `control_node` runs a 10 ms timer in callback group `ctrl` and logs, per 100
  ticks, the longest gap between two ticks against a 50 ms budget. After
  1000 ticks it prints `10 windows, <n> late`.
- `telemetry_node` runs a 100 ms timer in group `telem` whose callback
  busy-waits 60 ms.
- The bringup's contract (`system.contract.yaml`) states `params: {}` and one
  timer path per node: 100 Hz and 10 Hz.
- `system.toml` authors **no** `[tasks]` and no `group_tasks`.

| Language | Schedule the image got | `nros run` result |
|---|---|---|
| C | the entry codegen derives two sched tasks from the contract's rates: `derived-control_node` at FreeRTOS priority 3 and `derived-telemetry_node` at 2. Boot: `scheduler core=0 levels=2`, `multi-level run — 2 level(s) over one executor` | `10 windows, 0 late` (longest gaps 15–19 ms) |
| C++ | the same derivation | `10 windows, 0 late` (13–18 ms) |
| Rust | nothing derived: no `derived-*` line at build, no multi-level line at boot. Both nodes run on one task | `10 windows, 10 late` (longest gaps 146–153 ms) |

The C build prints the plan:

```text
codegen entry: sched task `derived-telemetry_node` spin period 100000 us, derived from the sched task's own period
codegen entry: priority plan (freertos, bigger-is-urgent): transport [4, 4], pool.app [1, 3] - the sched task key's projection
codegen entry:   sched task derived-control_node: priority 3 (derived)
codegen entry:   sched task derived-telemetry_node: priority 2 (derived)
codegen entry:   level 0: priority 3, threshold rank 0, members /control_node (rank 0)
codegen entry:   level 1: priority 2, threshold rank 1, members /telemetry_node (rank 1)
```

The Rust build and boot print none of it.

With `[tasks]` AUTHORED, the three languages agree. With both groups on one
task `main` (priority 2), each prints `10 windows, 10 late`. With `ctrl` on
`[tasks.high]` (priority 3), each prints `10 windows, 0 late`.

## Where the derivation lives

All three roads reach the same shared core,
`nros_orchestration_ir::derive`. Each asks `placement_is_unauthored` and then
calls `derive_sched_tasks_from_contracts` / `derive_sched_tasks_in_plan`:

- C/C++ entry: `packages/cli/nros-cli-core/src/codegen/entry/mod.rs` (the
  `placement_is_unauthored` guard, then the derive call). This is the road
  that derived above.
- Rust entry: `nros::main!`, `packages/core/nros-macros/src/main_macro.rs`
  (the same guard and call, "so a pure-cargo Rust entry gets an identical
  table"). This is the road that derived nothing above.
- `packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (the `codegen-system`
  verb).

So the Rust side calls the derivation and yet the image ran on one task. Why
it produced no table is not determined here. Two inputs differ between the
roads and are candidates:
- the contract the macro's model folds in: this workspace's contract is the
  leaf `system.contract.yaml`, not a launch-file contract;
- the callback groups it sees (`node_groups`).

## What is not decided here

Which side is right is a design call, and this issue does not make it:

- **Derive everywhere.** The Rust road gets the C/C++ table: an image whose
  contract states rates is scheduled rate-monotonic unless `[tasks]` says
  otherwise.
- **Derive nowhere** on a leaf contract. C/C++ stop deriving, and an
  unauthored image runs on one task in every language.

Either way, the result should not depend on the language. A build that
derives, or declines to, should say which it did.

## Next

Rule the design, then make the three roads agree. Add a cell that builds one
contract in all three languages and compares the sched-task tables. The
two-tier lab authors `[tasks]` explicitly, so its lesson does not depend on
this.

## Findings (2026-11-12)

**Root cause: the roads fed the shared derivation different callback-group
maps.** The model, the contract and the derivation core were the same on
every road. What differed was the third argument of
`derive_sched_tasks_from_contracts`, the per-node callback-group map.

- `codegen entry` (C/C++) builds it from the cmake `CALLBACK_GROUPS` keyword
  (`nros-metadata.json`, via `metadata::enrich_plan`), plus the model's
  bindings. An unbound declared group lands on `DEFAULT_SCHED_TASK`, which is
  `placement_is_unauthored` rule 4, so the derivation runs.
- `nros::main!` (Rust) built it from the model's resolved bindings ONLY
  (`<fqn>/<group>` keys, i.e. `[[component]] group_tasks`). A Rust node states
  its groups in code (`node.callback_group("ctrl")`), and no part of the
  macro's input read that. With no `group_tasks` the map was empty,
  `placement_is_unauthored` asked `any` over nothing and answered `false`,
  and the derivation never ran. Nothing was printed.

The groups the Rust code states DO reach disk. `nros configure`'s metadata probe
writes them into each package's source-metadata sidecar
(`src/<pkg>/metadata/<exec>.json`, `callbacks[].group`). The macro already reads
that sidecar for executor sizing (`source_metadata_sidecars::collect`), but only
for the slot counts.

Reproduced on the issue's shape (`nros new ... --lang rust`,
`mps2-an385-freertos`, contract 100 Hz / 10 Hz, `params: {}`, no `[tasks]`,
`node.callback_group("ctrl" | "telem")`). `nros build freertos` printed, with
the new decline line:

```text
nros::main!: derived schedule: declined - no node declares a callback group, so every callback runs on the default sched task
```

The sidecars it had just written held `"group": "ctrl"` and `"group": "telem"`.
The leaf-vs-launch contract candidate is ruled out: every road reads the same
`build/nros/models/<bringup>/system_model.yaml`.

**RFC-0079 reading.** §3 says "ORDER is derived from the contract, not stated".
CLAUDE.md's issue 1426 entry says an unauthored placement is derived, and all
three roads ask the one predicate. Neither RFC-0079 nor RFC-0052 exempts a
leaf contract or a language. On the rules as written, "derive everywhere" is
the stated design and the Rust road is the defect.

**The two options, priced.**

- *Derive everywhere.* The macro takes declared group ids from the sidecars
  through the same row builder the C entry uses
  (`entry_groups::node_callback_groups`). Cost: one more sidecar field read
  (no new producer, since the probe already records the group). A Rust image
  whose contract states rates and whose code names groups now runs
  multi-level. That is a behaviour change for existing Rust images in that
  shape, so it needs a changelog line. A Rust node whose code names no group
  is unchanged (groupless note, default sched task), exactly as in C.
- *Derive nowhere* on an unauthored image. `codegen entry` and
  `codegen-system` stop deriving unless `[tasks]` or `group_tasks` say so.
  Cost: this reverses phase-459 W1/W2 and issue 1426, and it turns RFC-0079
  §3 into "order is derived only when asked". The cmake `CALLBACK_GROUPS` +
  contract road, the shape `derived-sched-tasks-cpp` and the Safety Island use,
  would go back to one task. Gates and tests built on that
  (`derived_sched_tasks_{bake,entry,precedence}`) would have to be inverted.

**Said either way.** Each road now prints one line: the derived table
(`derived schedule: sched task ... priority ...`), or why it declined
(`entry_groups::Declined`: no callback group, or every placement authored).

**A sibling divergence, not changed here.** `codegen-system`'s
`collect_callback_groups` lets a non-empty `group_tasks` hide every keyword
group it does not bind. `codegen entry` keeps such a group on the default sched
task. The two agree when a node binds all of its declared groups or none of
them. The other case is pinned on purpose by
`an_authored_binding_still_outranks_the_keyword`, so it needs its own ruling.

**Gate.** `nros-cli-core/tests/derived_sched_tasks_parity.rs` feeds one model
to all three roads and compares the resolved tables. The first commit on branch
`fix-2432-rust-derived-sched` pins today's divergence. The second commit
implements "derive everywhere": `nros::main!` reads the sidecar's groups through
`entry_groups::groups_of_component`, and the gate asserts that the Rust table
equals the C one. The two commits are separate, so the direction can still be
ruled the other way by dropping the second.
