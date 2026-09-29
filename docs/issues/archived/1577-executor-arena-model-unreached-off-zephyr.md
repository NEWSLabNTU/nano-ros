---
id: 1577
title: "The executor arena's per-kind model runs only on the Zephyr resolver road — every cargo and plain-cmake image is sized by the `max_cbs` fallback, so a descriptor row it states prices nothing"
status: resolved
resolved_in: 2026-09-29
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

## Resolution (2026-09-29)

Fix direction 2: **the counts live in the descriptor.** `[image]` gains five
facts — `subscription_entities`, `timer_entities`, `service_server_entities`,
`action_client_entities`, `action_server_entities` — stated from
`EntityInventory::derive`'s `per_kind`, the same table the cmake road emits as
`NROS_ENTITY_COUNT_*`, and refused (never zero) wherever that derivation is.
`nros-node/build.rs` reads each descriptor-first and falls back to the env
carrier; where both arrive and disagree it takes the larger and says so.

Why these are `[image]` facts although the rows already carry four of the five
kinds: a row is written only for an entity in a STATED component with a type
and a topic, so counting rows under-counts exactly where a declaration is
missing — and the arena is `BufferTooSmall` when short. A timer has no row at
all. `subscription_entities` is not `subscriber_count`: the latter is session
SLOTS and includes each action client's feedback subscription, which the arena
prices inside the action-client entry.

### The flaw the review found, and the guard for it

Turning the model on made a latent under-size reachable. The producer states
`registration_path = "in_place"` by the entry's `rmw` NAME, and one descriptor
serves builds that link another backend: a single-package leaf's fixture rows
switch backend by cargo FEATURE over one image (`native/rust/listener` has
zenoh, xrce and cyclonedds rows over one `system.toml` that says zenoh). So the
cyclonedds build read an `in_place` row and priced no receive region for a
backend that buffers.

The build now answers it, not the descriptor. Each backend crate enables one
feature on `nros-rmw` — `in-place-dispatch` (nros-rmw-zenoh, nros-rmw-xrce-cffi)
or `buffered-dispatch` (nros-rmw-cyclonedds, nros-rmw-metadata) — carried to
`nros-node` as `links` metadata. An `in_place` row is honoured only when
in-place is CLAIMED and buffered is not: buffered vetoes because features
unify, and silence prices the full region because Cyclone and uORB under cmake
reach cargo as a bare `rmw-cffi` with no crate to declare anything. Gate
`check-backend-dispatch-declared` holds the classification (every
`packages/rmw/` crate, both directions), the producer's `backend_dispatch()`
table against it, and the carrier's three ends.

### Measured

`examples/native/rust/listener`, `nros sync` + `nros build`, nothing exported:

| | `[image] subscription_entities` | `arena_model::REQUIRED` | `ARENA_SIZE` |
| --- | --- | --- | --- |
| before (this issue's table) | — | 0 (fallback) | 14,424 |
| after, zenoh | 1 | 3,072 | 8,192 (floor) |
| after, cyclonedds row, same descriptor | 1 | 6,144 + warning | 8,192 (floor) |

Five subscriptions (an untracked copy of the listener, deleted after), where
the difference clears the floor:

| build | `REQUIRED` | `ARENA_SIZE` | runs |
| --- | --- | --- | --- |
| zenoh, rows `in_place` honoured | 7,168 | 8,192 | yes — all five register |
| cyclonedds, guard ON (rows overridden, 5 warnings) | 22,528 | 22,528 | yes |
| cyclonedds, guard OFF (temporary edit) | 7,168 | 8,192 | **no** — `arena exhausted at arena::SubBufferedRawEntry: 3688 B short, 7920/8192 used` → `NodeError::BufferTooSmall` |

The last row is the guard earning its keep: without it the model ships an
image that dies at registration.

Stated-size images still build: `threadx-linux` action-server / action-client
(`REQUIRED` 20,096 against `backing_u64s = 11069`) and service-server (5,120);
esp32's stated 16,384 is above any esp32 leaf's model (one subscription or one
timer). `examples/native/rust/action-server` registers every entity at 20,096.

### Not in this change

- The model's buffered subscription term is an OVER-size: five buffered
  subscriptions are priced at 22,528, and the guard-off failure above shows the
  real claim is at least 11,880 (7,920 used + 3,688 short, with the rest still
  unregistered). Safe direction, pre-existing, and what the Zephyr lane has
  always used.
- `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS` (issue 1255) still does not travel on
  these roads. Where the descriptor states rows, their `claimed_slot_bytes`
  already price the region, so the table is no longer the only way to reach
  the model; its comment in `NanoRosEntityFacts.cmake` is updated.
