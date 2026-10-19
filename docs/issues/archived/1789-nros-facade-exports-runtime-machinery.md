---
id: 1789
title: "`nros::` still exports runtime machinery beside the user API: 64 ledger rows say a name should be hidden or moved, and nothing tracked it once issue 0784 closed"
status: resolved
type: tech-debt
area: [api]
found: 2026-10-11
resolved: 2026-10-18
related: [issue-0784, phase-483, rfc-0089, rfc-0036, phase-506]
---

**Home: [phase-506](../../roadmap/archived/phase-506-api-crates-carry-only-the-api.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What

Issue 0784 had two halves. The NODE half (four node-shaped names, and which
one `nros::` leads with) was settled by phase-483 W2–W4 and the issue was
resolved and archived. The EXPORT half was not: the facade still publishes,
at the top level of `nros::`, names that only a macro expansion, a
generated entry point, a backend or the metadata probe uses.

The API-parity ledger says so in 64 rows, and each of those
rows cited issue 0784 as where the complaint lives. With 0784 archived, they
pointed at a closed issue for an open problem. They now point here. This query lists them:

```sh
python3 - <<'EOF'
import json, glob
for f in sorted(glob.glob("docs/reference/api-parity-ledger/*.json")):
    for k, v in json.load(open(f)).items():
        if isinstance(v, dict) and "1789" in v.get("why", ""):
            print(f.split("/")[-1], k)
EOF
```

By file, on 2026-10-11: action 14, node 13, service 12, other 11, exec 3,
timer 3, metadata 2, pubsub 2, serde 2, param 1, qos 1.

The complaints come in three kinds:

- **Should be `#[doc(hidden)]`**, like `__private_node_state_into_raw`
  already is: items only the `nros::node!` expansion or the generated entry
  calls, such as the `install_node_typed*` family.
- **The RMW seam reaching `nros::`**: `Session`, `Rmw`, `Transport`,
  `TopicInfo`, `RmwPublisher` and their methods are what a backend
  implements, not what a node calls, and they sit beside the user API.
- **Executor internals exported at the top level**:
  `Executor::action_client_core_mut`, `service_client_entry_mut`,
  `register_*_raw*`, the core action types.

## Why it matters

Phase-483 made `nros::` read as rclrs (`use nros as rclrs;`). A porting user
who browses `nros::` for rclrs's `Subscription` still finds the backend seam
and the executor's plumbing at the same level, which is the "three audiences"
problem 0784 was named for.

## Direction

Not decided here; it is a facade export policy, and RFC-0036 / RFC-0089 own
that. The cheap move is the first kind: `#[doc(hidden)]` on the
expansion-only items, which keeps them reachable for generated code. The
other two kinds need a decision on where the seam lives, for example an
`nros::rmw` / `nros::internals` module, since `internals` already exists and
says what it is. Run `just check rustdoc-links` and `rustdoc-workspace` when
moving any public name.

## Resolved (2026-10-18) — RFC-0071 D12, phase-506 W1

The direction was decided as RFC-0071 D12 (maintainer ruling 2026-10-18), and
the three kinds went three ways:

* **The RMW seam left `nros::`.** `Session`, `Rmw`, `RmwConfig`, `Transport`,
  `TransportConfig`, `TopicInfo`, `ServiceInfo`, `ServiceRequest`, the
  `Publisher` / `SubscriptionTrait` / `ClientTrait` / `ServiceTrait` entity
  traits: no hand-written file outside the facade reached any of them through
  `nros::` (one comment did), because every backend imports `nros-rmw`
  directly. `SessionMode` stays: it is the type of `ExecutorConfig`'s `mode`.
  66 ledger rows described names the facade no longer exports and were
  deleted, and one name collision surfaced: the seam's `ServiceInfo` had been
  correlated with rclrs's per-request `ServiceInfo`, masking a real gap, now
  issue 1942.
* **Macro machinery moved to a doc-hidden `nros::__macro`:** `ActionExecutor`,
  `ClientDispatch`, `PublisherResolver`, `ExecutorNodeRuntime`, the
  `install_node_typed*` family, `record_node_metadata`, `register_node`,
  `__record_with_context`, `__private_node_state_into_raw`. `nros-macros`, the
  metadata probe generator, the boards and the tests follow the path.
* **Executor internals:** the untyped action types (`ActionClientCore`,
  `ActionServerCore`, `ActionServerRawHandle`, `RawAction{Client,Server}Spec`)
  moved to `nros::internals`; the raw registration methods, the component and
  dispatch-slot tables and the `*_handle` accessors are `#[doc(hidden)]` on
  `Executor`, since a method cannot change module.

What a component author writes (`declarative_component!`, `DeclarativeNode`,
`NodeId`, `ParameterDefault`, the source ids) stays at the crate root, and its
ledger rows now say so instead of citing this issue. Of the 64 rows that cited
it, the 10 naming a now-hidden `Executor` method left the extraction and were
deleted; the other 54 are re-filed with the disposition they got.
`just check api-parity` passes.
