---
id: 1372
title: "A path's `trigger.timer.rate_hz` never reaches the realizer: the period
  it derives comes from the first output endpoint's `pub.min_rate_hz` instead,
  and nothing reports the two disagreeing"
status: open
type: bug
area: [cli, launch, contract, orchestration]
severity: medium
found: 2026-09-18
related: [issue-1339, issue-1256, issue-0760, rfc-0052, rfc-0078, phase-454]
---

## Two fields that look like the same fact

A launch contract can state a periodic path's rate twice:

```yaml
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 30 } }
        output: [emergency_control_cmd, status]
    pub:
      emergency_control_cmd: { min_rate_hz: 30 }
      status: { min_rate_hz: 30 }
```

Only the second one is read. The first is parsed, validated, and then has no
field to land in.

## Where it is dropped and where the substitute comes from

`contracts.node_paths` in the resolved model carries `input` and `output` and
no trigger - issue 1339 tabulates that drop at the schema boundary, and the
fixture header at
`packages/cli/nros-cli-core/tests/fixtures/queue_buffer/launch/queue.contract.yaml:13-14`
states it outright:

> `listener.paths.drain.trigger.timer` -> DOES NOT (PathContract has no
> trigger; only `output` survives)

nano-ros then reconstructs a rate from what DID survive.
`packages/core/nros-orchestration-ir/src/mapper_input.rs:69-81`:

```rust
        // Trigger: an empty `input` is a timer/periodic path (fires on a
        // clock at the output's contracted rate); otherwise it is event-driven
        // on its input endpoints.
        let effective_trigger = if pc.input.is_empty() {
            let rate_hz = pc
                .output
                .iter()
                .find_map(|o| pub_rate_hz(model, o))
                .unwrap_or(0.0);
            EffectiveTrigger::Timer { rate_hz }
        } else {
            EffectiveTrigger::Input(pc.input.clone())
        };
```

and `pub_rate_hz` at `mapper_input.rs:46-52` is a lookup in
`contracts.pub_endpoints[*].min_rate_hz`.

So the number that reaches the realizer is the first output endpoint that
carries a `min_rate_hz` - `find_map`, so an output with no contracted rate is
skipped rather than ending the search - and `0.0` if no output has one.

It does reach the scheduler from there.
`packages/core/nros-orchestration-ir/src/rtos_realizer.rs:275-280` turns it
into the node's period:

```rust
            if let EffectiveTrigger::Timer { rate_hz } = &p.effective_trigger
                && *rate_hz > 0.0
            {
                let per = 1000.0 / rate_hz;
                period_ms = Some(period_ms.map_or(per, |cur: f64| cur.min(per)));
            }
```

`period_ms` is a `NodeFacts` field, which is what rate-monotonic ranking reads.
The authored `trigger.timer.rate_hz` has no path to this expression. The value
under `pub:` does.

## Why nobody has noticed

They agree on every contract in reach.

On the Autoware Safety Island
(`src/safety_island_bringup/launch/safety_island.contract.yaml`), all four
nodes state the rate twice and state it identically:

| node | `trigger.timer.rate_hz` | first output's `min_rate_hz` | period derived |
| --- | --- | --- | --- |
| `mrm_comfortable_stop_operator` | 10 (line 97) | `status` 10 (line 102) | 100 ms |
| `mrm_emergency_stop_operator` | 30 (line 114) | `emergency_control_cmd` 30 (line 119) | 33.3 ms |
| `mrm_handler` | 10 (line 144) | `mrm_state` 10 (line 160) | 100 ms |
| `stop_mode_operator` | 30 (line 175) | `control` 30 (line 182) | 33.3 ms |

The resolved model confirms the drop. `build/nros/models/safety_island_bringup/system_model.yaml:343-351`:

```yaml
  node_paths:
    /mrm_comfortable_stop_operator/on_timer:
      output:
      - /mrm_comfortable_stop_operator/status
    /mrm_emergency_stop_operator/on_timer:
      output:
      - /mrm_emergency_stop_operator/emergency_control_cmd
      - /mrm_emergency_stop_operator/status
```

No `input`, no trigger, no rate. The derived periods are right, and they are
right by coincidence of authoring.

The same holds for the in-tree fixtures. `queue.contract.yaml:37` and `:52` are
the one place a 50 and a 10 sit on different paths, and that fixture exists to
demonstrate the drop rather than to run a schedule.

## What an author is entitled to assume, and what happens instead

`trigger` is the field whose NAME says "this is when the callback fires".
`min_rate_hz` is a publication guarantee about an endpoint. A reader editing
`trigger: { timer: { rate_hz: 10 } }` to `20` and rebuilding gets a byte-
identical schedule, with no diagnostic, because the realizer never saw either
value.

The substitute is also not merely a different spelling of the same fact:

* **It is absent for a path that publishes nothing.** A timer that only reads
  state or calls a service has an empty `output`, so `find_map` yields `None`
  and the rate becomes `0.0` at `mapper_input.rs:77`. `rtos_realizer.rs:276`
  then skips it and the node contributes no period at all.
* **It depends on output ORDER.** `mrm_emergency_stop_operator` publishes
  `emergency_control_cmd` and `status` from the same tick. Today both are
  30 Hz. If they were not, the derived period would follow whichever the
  resolver listed first.
* **The resolver advises deleting it.** Issue 1339 records the resolver calling
  a `min_rate_hz` that duplicates a topic-level rate "redundant and can be
  deleted". Taking that advice removes the only spelling of the fire rate that
  survives into the model.

## Relationship to issue 1339

1339 is the SCHEMA defect: `ros-launch-manifest`'s `SubContract` has no
`buffer` and its `PathContract` has no `trigger`, so two facts the resolver
parses, validates and reasons about are dropped at the model boundary. Its fix
list (`docs/issues/1339-queue-buffer-and-path-trigger-dropped-by-system-model.md`,
"What would fix it", item 2) is exactly "`PathContract` gains the effective
trigger - at minimum the timer rate."

This issue is the DIVERGENCE half, and it stays open after 1339 lands if
nothing is added to detect disagreement. The two fields remain separately
authorable; a contract can state `trigger.timer.rate_hz: 10` and
`pub.status.min_rate_hz: 30` today and get a 33.3 ms period with no output of
any kind. That is the part a schema field alone does not fix.

It is filed separately for that reason, and because the detection can land
before the schema change: a resolver-side comparison of the two numbers needs
no new model field, only the two values the resolver already holds.

## What would fix it

1. **Report the mismatch where both values exist** - in the resolver, which
   parses both. A periodic path whose `trigger.timer.rate_hz` differs from the
   `min_rate_hz` of any endpoint in its own `output` is either a typo or a
   distinction the author meant, and both are worth one line of output. This is
   the same shape as the `[queue-drain-rate]` diagnostic 1339 quotes, which
   already joins a path's rate against its subscriptions' producer rates.
2. **Carry the trigger** (issue 1339, item 2), then read it at
   `mapper_input.rs:72-78` in preference to the output's promise, keeping the
   `pub_rate_hz` fallback for a contract that states only one.
3. **Say so at the fallback.** Until (2), the comment at
   `mapper_input.rs:69-71` should name this issue: "fires on a clock at the
   output's contracted rate" is accurate about the code and reads as though it
   were the authored trigger.

## Correction to the report that opened this

The claim was that `trigger.timer.rate_hz` "does not reach the scheduler". A
rate does reach the scheduler - `rtos_realizer.rs:275-280` derives `period_ms`
from it and RM ranking consumes that. What does not reach the scheduler is the
AUTHORED number: the value the realizer uses is
`contracts.pub_endpoints[first output with a rate].min_rate_hz`, and the value
under `trigger.timer` is discarded one layer earlier.
