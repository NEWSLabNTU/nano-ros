---
id: 1577
title: "The executor arena's per-kind model runs only on the Zephyr resolver road — every cargo and plain-cmake image is sized by the `max_cbs` fallback, so a descriptor row it states prices nothing"
status: open
type: tech-debt
area: [build, core]
severity: medium
found: 2026-09-29
related: [1340, 1522, 1255, 1122, 0196, rfc-0100, phase-454, phase-457]
---

## What

`nros-node/build.rs` sizes the executor arena two ways:

```rust
let model_required = match (declared_subs, declared_timers, declared_services,
                            declared_action_clients, declared_action_servers) {
    (Some(..), Some(..), Some(..), Some(..), Some(..)) => Some(
        sub_rows.and_then(|rows| subs_arena_from_descriptor(rows, ..))
            .unwrap_or_else(|| subs_arena(..))
        + timers * TIMER_ENTRY + ..,
    ),
    _ => None,
};
let derived_arena = match model_required {
    Some(required) => required.max(ARENA_FLOOR),
    None => (action_clients * action_client_entry
        + max_cbs.saturating_sub(action_clients) * pubsub_entry
        + ARENA_BASE_OVERHEAD).max(ARENA_FLOOR),
};
```

The per-kind MODEL — the only arm that reads the sizing descriptor's
`[[endpoint]]` rows through `subs_arena_from_descriptor` — runs only when ALL
FIVE `NROS_ENTITY_COUNT_*` carriers arrive. Only the cmake road produces them
(`cmake/NanoRosEntityInventory.cmake`), and only the Zephyr resolver lane
delivers them to cargo. `cmake/NanoRosEntityFacts.cmake` already says so:

> the arena's per-kind sum runs only where `NROS_ENTITY_COUNT_*` arrive —
> which is the Zephyr resolver road alone. On this road and on the cargo-leaf
> sidecar the model is 0 and the sum is never reached

So on a cargo leaf, a cargo workspace, or a non-Zephyr cmake image, the arena
is `max_cbs × pubsub_entry + overhead`, and **no descriptor row changes it**.

## Measured

`examples/native/rust/listener` (cargo leaf, zenoh, one generic subscription),
2026-09-29, after issue 1340's runtime fix (`c6a8b7f7bb`). A single bool —
`DeclaredSubscriptionShape::BufferedRaw.in_place_capable()` — toggled, with a
fresh metadata probe each time:

| | descriptor `registration_path` | generated `ARENA_SIZE` | `mem-report` RAM in symbols |
| --- | --- | --- | --- |
| `false` | `unbounded` | 14,424 | 157,642 |
| `true` | `in_place` | 14,424 | 157,642 |

The generated `nros_node_config.rs` is identical in every `const … usize`
between the two builds, and `arena_model::REQUIRED` is emitted as `0` — the
fingerprint of the fallback. The leaf's `build/native/nros-cargo.toml` `[env]`
carries four `NROS_DECLARED_*` knobs and none of the five counts.

## What it costs

Every descriptor fact that is priced THROUGH the arena model is inert on these
roads. Issue 1340's saving is the one measured: the executor now claims no
receive region for a generic subscription on zenoh / XRCE, and on a cargo
image those bytes become unused headroom inside a fixed arena rather than a
smaller image. #1340's own figure (`EXECUTOR_BACKING` 61,184 → 17,824 on a
four-subscription image) is only collectible where the model runs.

Worth knowing: even where the model runs, `ARENA_FLOOR` is 8,192, so a
one-subscription image clamps to the floor either way — #1522 priced this
leaf's receive row at 3,072 vs 6,144, both under it. The saving appears once
`required` clears the floor.

## Fix direction (not decided)

The comment in `NanoRosEntityFacts.cmake` names the order: **the counts come
first on these roads.** Two shapes, and they differ on the carried-facts rule:

1. **Give the cargo roads a producer for the existing carriers.** `nros sync`
   already composes an `EntityInventory` for the leaf and writes four
   `NROS_DECLARED_*` rows from it into `nros-cargo.toml`; writing the five
   `NROS_ENTITY_COUNT_*` from the same inventory is small. It is also a second
   carrier of facts the descriptor exists to hold.
2. **Read the counts from the descriptor**, env carriers as the fallback —
   the "descriptor first, env carriers second" order phase-454 W5 already
   uses for `sub_rows`, and the one CLAUDE.md asks for ("never re-carry a fact
   through an env knob"). The descriptor's `[image]` states `node_count`,
   `backend_count` and `subscriber_count` today and **no timer, service-server
   or action counts**, so this is a schema addition with a producer on every
   road.

Either way acceptance is a `mem-report --baseline` on a cargo image whose
`required` clears `ARENA_FLOOR`, showing the model arm running (a non-zero
`arena_model::REQUIRED`) and a descriptor row moving the arena.
