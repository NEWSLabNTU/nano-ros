---
id: 1371
title: "A component with no callback groups derives no tiers, and the only
  signal is a stderr line nothing in the build tree retains: four artifacts had
  to be cross-checked to establish that the Safety Island's 19 callbacks all
  run on Zephyr main"
status: open
type: bug
area: [cli, orchestration, zephyr]
severity: medium
found: 2026-09-18
related: [issue-0259, issue-1283, issue-1286, phase-296, rfc-0047, rfc-0052]
---

## The gate

`packages/core/nros-orchestration-ir/src/derive.rs:92-99`:

```rust
    for n in &plan.nodes {
        let node = bare(&n.name).to_string();
        // A ranked node with no declared callback groups has nothing for the
        // gating executor to bind - it stays on the default tier (loud note).
        let Some(groups) = callback_groups.get(&node).filter(|g| !g.is_empty()) else {
            out.groupless_notes.push(n.name.clone());
            continue;
        };
```

The reasoning is right: a tier is bound by node name AND callback group
(RFC-0047), so a node with no groups has nothing to bind. The problem is the
word "loud" in the comment. The note is pushed onto
`DerivedSchedule::groupless_notes` (`derive.rs:39`), and what happens to it
next is the issue.

## The note goes to stderr and stops there

`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:265-287` handles
both outputs of the derivation, and handles them differently:

```rust
    // Issue 0259 - surface on stderr AND carry into the plan. stderr is for the
    // person watching this bake; the plan is for everyone who reads the system
    // afterwards, and a verdict that exists only in scrollback cannot be
    // audited.
    let mut warnings = Vec::new();
    for d in &derived.degradations {
        eprintln!(...);
        warnings.push(crate::orchestration::plan::PlanSchedWarning { ... });
    }
    for name in &derived.groupless_notes {
        eprintln!(
            "codegen-system: derived-schedule note - node '{}' declares no \
             callback groups; it stays on the default tier",
            name
        );
    }
```

Degradations get both halves. Groupless notes get only the first. The comment
at `model_ingest.rs:265-268` states the exact rule the loop below it breaks:
**"a verdict that exists only in scrollback cannot be audited."**

The persisted half already exists and is already plumbed. `PlanSchedWarning`
(`packages/cli/nros-cli-core/src/orchestration/plan.rs:88-100`) carries
`node` / `dim` / `reason`; `Plan::sched_warnings`
(`plan.rs:57-58`) serialises into the plan artifact; `nros explain` renders it
at `packages/cli/nros-cli-core/src/cmd/explain.rs:143-153`. A groupless note is
a verdict about one node with a reason, which is the shape that struct was
written for. Nothing about it is new work.

## The Rust `nros::main!` path does not even print

`packages/core/nros-macros/src/main_macro.rs:1051` wraps BOTH loops:

```rust
            if !derived.tiers.is_empty() {
                for d in &derived.degradations { eprintln!(...); }
                for name in &derived.groupless_notes { eprintln!(...); }
```

`derived.tiers` is empty exactly when every node was groupless. So on the
pure-cargo Rust entry path, the image in which the note matters most is the one
image that prints nothing at all.

The CMake path is not much louder: `packages/cli/nros-cli-core/src/cmd/codegen_system.rs:310-315`
prints "derived N scheduling tier(s)" only `if derived > 0`, so a zero-tier
bake has no summary line either. The per-node notes are the entire signal.

## Measured on the Autoware Safety Island

Four components, one Zephyr image. The scheduling outcome is "no tier threads
exist; every callback runs on Zephyr main", and establishing that took four
artifacts because no single one says it.

**1. The input.** `build-zephyr/nros-metadata.json` lists four components and
every one of them carries `"callback_groups": []`:

```json
   "name": "mrm_emergency_stop_operator",  ... "callback_groups": []
   "name": "mrm_comfortable_stop_operator", ... "callback_groups": []
   "name": "stop_mode_operator",            ... "callback_groups": []
   "name": "mrm_handler",                   ... "callback_groups": []
```

So `derive.rs:96` takes the `else` arm four times out of four, and
`DerivedSchedule::tiers` is empty.

**2. The resolved model.** `build/nros/models/safety_island_bringup/system_model.yaml`
has no `execution:` key at all - consistent with `codegen_system.rs:299`
(`if model.execution.tiers.is_empty()`) taking the derive path, and with the
derive producing nothing to write back.

**3. The generated entry.** `build-zephyr/zephyr_entry_nros_main_generated.cpp:183`:

```cpp
    return static_cast<int>(::nros::board::ZephyrBoard::run_components(...));
```

`run_components`, not `run_tiers`. That is
`ExecutorShape::Single` from `packages/cli/nros-cli-core/src/codegen/entry/mod.rs:239-252`,
whose first arm returns `Single` when `resolved_tiers` is absent or
single-tier. Zephyr DOES export a `run_tiers` (`mod.rs:267-270`,
`packages/boards/nros-board-zephyr/src/entry_tiers.rs:376`), so the board is not
the reason; the empty tier table is.

**4. The link map.** `build-zephyr/zephyr/zephyr.map` puts the tier machinery
under `Discarded input sections` (that heading is at line 8436):

```
 .bss.nros_tier_threads
                0x0000000000000000      0x120 modules/nros/libnros.a(nros_platform_zephyr_shims.c.obj)
 .bss.nros_tier_index
                0x0000000000000000        0x4 modules/nros/libnros.a(nros_platform_zephyr_shims.c.obj)
```

288 bytes of thread-handle pool and the whole of `zephyr_run_tiers.c.obj`,
garbage-collected. That is the linker confirming no tier thread is ever
created.

**What actually runs on main.** Counted from the four component constructors:
4 timers (one `::nros::NodeWithTimers<1>` per component), 11 subscriptions
(1 + 7 + 3, via `NROS_SUBSCRIBE` and one `create_subscription_in`), 2 service
servers (`::nros::bind_service`) and 2 service clients
(`::nros::create_service_client_raw`). **19 dispatch callbacks, one thread, no
priority separation between them.**

None of the four artifacts says "you got no scheduling". Three of them are
absences - a missing key, a symbol that is not there, a function that was not
called - and an absence is only evidence once you already suspect it.

## Why this is a bug and not a preference

The derivation is behaving as designed. What is wrong is that its most
consequential outcome is its quietest one. A build in which the derivation
weakens one dimension of one node's schedule produces a durable, machine-
readable verdict in the plan. A build in which the derivation produces NO
SCHEDULE AT ALL produces a stderr line per node and nothing else, and on the
Rust entry path not even that.

The asymmetry is backwards. A degradation is a partial result a reader will
notice because the tier table exists and looks odd. Zero tiers looks exactly
like a single-threaded image that was never meant to have tiers, which is a
legitimate and common configuration - and that is precisely why the distinction
has to be recorded rather than inferred.

## What would fix it

1. **Persist the note.** Push a `PlanSchedWarning` for each groupless node
   beside the `eprintln!` at `model_ingest.rs:281-287` - `dim: "callback_groups"`,
   `reason` naming the node and that it stays on the default tier. Nothing new
   is needed: the struct, the field, the serialisation and the `nros explain`
   renderer are all already there, and a plan with no such notes stays
   byte-identical (`plan.rs:55-57`).
2. **Emit a summary line at zero.** `codegen_system.rs:310-315` prints only
   when `derived > 0`. An `else` arm saying "derived 0 tiers from N contracted
   nodes; M declared no callback groups, so all callbacks run on the default
   executor" turns four artifacts into one line.
3. **Unhide the macro path.** Move the two loops at `main_macro.rs:1054-1065`
   out of the `if !derived.tiers.is_empty()` guard at `main_macro.rs:1051`. The
   guard suppresses the notes in the one case they describe.

(3) is a one-line move and should not wait for the others.
