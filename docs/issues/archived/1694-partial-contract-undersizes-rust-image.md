---
id: 1694
title: "A contract that describes SOME of a native Rust image's endpoints sizes the
  image from the contract alone — the undescribed timer and subscription get no
  slot and the image dies `ExecutorFull` at boot"
status: resolved
type: bug
area: [cli, codegen, sizing]
severity: medium
found: 2026-10-05
related: [issue-1676, issue-1572, rfc-0100]
---

## What happens

Found while proving issue 1676 on `examples/workspaces/rust` (copied to a
scratch dir). The stock workspace has no contract; its sizing descriptor is not
derived and every pool keeps its crate default, so talker + listener boot and
publish. Adding ONE contract file that states only the talker's rate:

```yaml
version: 1
nodes:
  talker:
    pub:
      chatter: { min_rate_hz: 0.5 }
topics:
  /chatter:
    type: std_msgs/msg/Int32
    pub: [talker/chatter]
```

flips the descriptor to `status = "derived"`, `basis = "contract"`, and the
derivation counts only what the contract describes (`nros sync && nros build
native`, then `nros-cargo.toml`):

| knob | derived | the image needs |
| --- | --- | --- |
| `NROS_EXECUTOR_MAX_CBS` | `0` | 2 (talker's 1 Hz timer, listener's subscription) |
| `ZPICO_MAX_SUBSCRIBERS` | `1` (the C-array floor, from a demand of `0`) | 1 |
| descriptor `subscriber_count` / `callback_slots` | `0` / `0` | 1 / 2 |

The image then refuses at boot:

```
[INFO] nros: session open
nros: application error: ExecutorFull("talker_pkg")
```

The descriptor itself says it is incomplete (`undeclared_endpoints = 1`), and
the derivation sizes from the described part anyway. Describing the timer
(`paths: on_tick: trigger: { timer: { rate_hz: 1 } }`) and the listener's
subscription makes the same image boot (`MAX_CBS = 2`) — so the numbers are
right for what the contract says, and wrong for the image.

## Why it matters

A contract is how an integrator states ONE promise (here a rate floor, which is
what RFC-0052's monitors and issue 1676's `/diagnostics` report act on). Adding
that one promise to a working image should not shrink every pool the contract
did not mention. RFC-0100 D6 already rules that a REFUSED field contributes the
worst case, never zero (issue 1572 for durability); an endpoint the contract
does not describe at all is the same situation one level up, and today it
contributes zero.

## What a fix needs

* When `undeclared_endpoints > 0`, the entity-kind counts the contract cannot
  see must not fall below what the recorded source metadata (the `nros sync`
  sidecars) or the crate default gives — `max(contract, recorded)` per kind,
  the rule `MAX_CBS` already applies to the model's wiring vs the sidecars
  (phase-307 W4), extended to subscriptions/timers the contract never names.
* Or refuse the derivation for an incomplete contract (descriptor `status`
  not `derived`), so the image keeps the defaults it booted with.
* A regression on the scratch-copy shape above: a one-row contract on the
  stock Rust workspace boots and publishes.

Not measured: the C and C++ roads with a partial contract (their entity
inventory reads the register declarations, which may already cover this).

## Resolution

Fixed 2026-10-05 on `fix/1694-partial-contract-sizing`. The rule, in ONE
place: **a contract is a statement about the endpoints it names** — it may
refine their sizing, and it never shrinks a count below what the launched
nodes' code creates.

* `EntityInventory::merged_per_kind_max` (the composition the cmake road
  already used for `nros-metadata.json`) now keeps the CONTRACT's rows whole
  — they carry the resolved topic and authored QoS, and win a tie — and
  appends the declaration's rows the contract does not account for, so each
  kind counts `max(contract, recorded)` (`floor_by_declaration`). The old
  "longer list wins whole" dropped every contract row and its QoS when the
  code created one more endpoint of a kind.
* A component the launch tree STARTS and the contract does not describe is no
  longer reclassified `NotLaunched` (which counted zero): `from_model` records
  the launch tree's nodes, so an `Absent` declaration for one refuses the
  derivation (crate defaults, the worst case). That is the cmake road's shape,
  whose `nros-metadata.json` states no entities since phase-412.
* Every road that feeds a count from a contract composes through
  `metadata_refresh::contract_inventory`, which floors the contract by the
  workspace's FRESH probe sidecars (`EntityInventory::recorded_for_launch`,
  keyed by the launch node's `(pkg, exec)`): `nros build`'s stage-3.5 resolve
  (the cargo road and the cmake seed), `nros ws entity-inventory --workspace`
  and `nros ws sizing-descriptor --from-model --workspace` (single and
  shared-runtime). `nano_ros_entry` passes the workspace, and now passes
  `NROS_WORKSPACE_DIR` — `_ws_root` was the entry's grandparent, which for a
  generated entry is `build/<coord>/`, so issue 1594's observation join was
  silently skipping the workspace on that road too.

**Measured** — `examples/workspaces/rust` copied to an untracked scratch dir
at the same depth, `launch/system.contract.yaml` naming only
`talker.pub.chatter.min_rate_hz: 0.5`, `nros sync && nros build native`, run
against a private `rmw_zenohd`:

| | before | after (rebased on #1684) |
| --- | --- | --- |
| `resolved.toml` | `max_cbs 0, nodes 1` | `max_cbs 2, nodes 3` (talker, listener, reporter) |
| `NROS_EXECUTOR_MAX_CBS` / `MAX_NODES` | `0` / `1` | `2` / `3` |
| descriptor `subscriber_count` / `callback_slots` | `0` / `0` | `1` / `2` |
| boot | `nros: application error: ExecutorFull("talker_pkg")` | `session open`, `talker publishing chatter seq=0..6` in 8 s |

**C/C++ road** — the same partial contract on a scratch copy of
`examples/workspaces/cpp`: before the `NROS_WORKSPACE_DIR` fix the configure
refused ("`listener_pkg::listener` declares no entities", crate defaults) and
`native_entry`'s descriptor stated 1 endpoint; after, the six-component
inventory derives and the descriptor states 2 (the listener's subscription
with an observed `registration_path = "in_place"`); `native_entry` boots and
delivers (`Published`/`Received` 0..4). Its native runtime is shared by six
entries, so the union of their models masks a partial contract there anyway.

Unit: `partial_contract_tests` (4) — one endpoint of two named sizes at least
the registered count; contract rows survive and win a tie; a launched node
nobody describes refuses; no sidecar + undescribed node refuses. Negative
control: with the old merge rules three of the four fail.

Sweep: `git grep -n "EntityInventory::from_model(" -- packages/cli/nros-cli-core/src`
(remaining non-test callers: `contract_join`, the leaf road's monitor rows,
the shared-runtime QoS reconcile — none feeds a count without the floor; the
cargo LEAF road's base was already the probe).

**Not measured:** a Zephyr / RTOS image with a partial contract; a node
launched twice under one component name (the merge keys by component name, as
before); a C/C++ component with a stale or missing probe sidecar, which now
refuses the derivation rather than trusting a contract that does not describe
it.
