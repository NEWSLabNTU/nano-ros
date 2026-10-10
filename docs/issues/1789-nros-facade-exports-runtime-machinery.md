---
id: 1789
title: "`nros::` still exports runtime machinery beside the user API: 64 ledger rows say a name should be hidden or moved, and nothing tracked it once issue 0784 closed"
status: open
type: tech-debt
area: [api]
found: 2026-10-11
related: [issue-0784, phase-483, rfc-0089, rfc-0036]
---

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
