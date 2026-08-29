---
id: 827
title: "Static RAM is a property of the RMW, not of the node — a talker reserves
  275 KB of service and large-payload pools it can never reach"
status: open
type: performance
area: rmw
related: [phase-392, phase-391, issue-0815, issue-0739]
---

## Problem

Measured with `just mem-report` over the four native Rust example roles, at
knob defaults, one row per (role, RMW):

| role | zenoh | cyclonedds | xrce |
| --- | ---: | ---: | ---: |
| `talker` | 345,379 | 69,381 | 10,340 |
| `listener` | 345,379 | 69,381 | 10,340 |
| `service-server` | 345,379 | 69,381 | 10,340 |
| `action-server` | 345,395 | 69,381 | 10,356 |

RAM attributed to symbols, bytes, `nros-relwithdebinfo`, measured on the
fixtures `just build-test-fixtures lane=native` writes under
`build/cargo-fixtures/linux*/nros-relwithdebinfo/`.

**Every role costs within 16 bytes of every other, and three of the four are
identical.** The 16 bytes on `action-server` are its own statics; no pool moves.
What the node does has essentially no influence on what it reserves. A `talker` — one publisher, no subscription, no service, no action —
reserves the same 144,128 bytes of `SERVICE_BUFFERS` as the service server, and
the same 131,072 bytes of `LARGE_PAYLOADS` as a node that subscribes to point
clouds.

For the talker, from `just mem-report`:

```
       144,128   40.4%  nros_rmw_zenoh::shim::service::SERVICE_BUFFERS
       131,072   36.7%  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
        32,768    9.2%  nros_rmw_zenoh::shim::subscriber::SMALL_PAYLOADS
        24,416    6.8%  g_sessions
```

275,200 of 345,379 bytes — **80% of the image's static RAM** — is two pools the
node cannot reach. `SMALL_PAYLOADS` is the only one it can, and it uses one of
its eight subscriber slots.

## Why it is this way

The pools are unconditional `static mut` arrays in the backend crate. Linking
the backend reserves all of them; nothing about the node's entity set reaches
the decision. This is deliberate and correct as a starting point — a static pool
is what makes the allocation statically provable, which is the property
[phase 392](../roadmap/phase-392-static-memory-space-campaign.md) explicitly
protects when it declines to move payload buffers to the heap.

The defect is not that the pools are static. It is that their SIZE is fixed at
the backend, where the entity set is unknown, rather than at the image, where it
is known. Two of the three inputs already exist:

- **The entity set is known at build time for a generated entry.** `nros sync`
  resolves the SystemModel; the entry codegen emits the entities. Phase 392 W2
  is already planning to sum the executor arena from exactly that source
  (`NROS_ARENA_REQUIRED`).
- **The knobs already exist and are already enumerable.**
  `ZPICO_MAX_QUERYABLES`, `ZPICO_MAX_LARGE_SUBSCRIBERS`, `ZPICO_MAX_SUBSCRIBERS`
  are in the [static pool inventory](../../book/src/reference/static-pool-inventory.md).
  A node with no service server wants `ZPICO_MAX_QUERYABLES = 0`.

So the saving is available without inventing a mechanism: it is one more
consumer of the resolved model, setting knobs that are already read. What is
missing is anything that connects the two.

## The trap in the obvious fix

`ZPICO_MAX_QUERYABLES` cannot simply be lowered to the app's service count.
Per CLAUDE.md and issue 0460, **a service server IS a zenoh queryable**, and
`[param_services]` (6) plus `[lifecycle]` (5) claim eleven slots before the app
declares anything. A knob derived from "how many services does the app create"
would be wrong by eleven and fail at runtime with an exhausted table — the
failure mode 0460 already cost a phase.

Any derivation has to count the infrastructure queryables the runtime creates on
the node's behalf, which means it belongs next to the code that creates them,
not in a codegen template that only sees the user's entities.

## Also worth noting

`LARGE_PAYLOADS` is 131,072 bytes at defaults (`ZPICO_MAX_LARGE_SUBSCRIBERS *
ZPICO_SUBSCRIBER_RING_DEPTH * ZPICO_SUBSCRIBER_LARGE_SIZE`) and is reserved even
when the image has no subscription at all. It is the single easiest win in the
list: an image whose resolved model contains zero subscriptions needs zero of
it, and that derivation needs no infrastructure accounting — unlike the
queryable one above, the runtime creates no large-payload subscriber of its own.

## Measured 2026-08-29 — the "easiest win" is NOT available with the existing knob

The section above calls `LARGE_PAYLOADS` "the single easiest win ... an image
whose resolved model contains zero subscriptions needs zero of it". Zero is
**not expressible**. `packages/rmw/zenoh/nros-rmw-zenoh/build.rs:45`:

```rust
let max_large: usize = env_usize("ZPICO_MAX_LARGE_SUBSCRIBERS", 2).max(1);
```

The `.max(1)` floor means the smallest reachable pool is
`1 * ZPICO_SUBSCRIBER_RING_DEPTH(4) * ZPICO_SUBSCRIBER_LARGE_SIZE(16384)` =
**65,536 bytes**, not 0. So wiring the resolved model to this knob — the fix
this issue proposes — buys half of what the issue claims: 131,072 -> 65,536 on
a talker, with the other half structurally out of reach until the floor goes.

MEASURED, not read — three builds of `examples/native/rust/talker`, the knob
varied, `llvm-nm -S` on each binary:

| `ZPICO_MAX_LARGE_SUBSCRIBERS` | `LARGE_PAYLOADS` |
| ---: | ---: |
| 2 (default) | 131,072 |
| 1 | 65,536 |
| **0** | **65,536** |

Note the third row: asking for zero does not fail, it silently yields one. A
codegen deriving "no subscriptions -> 0" would emit a config that reads as
satisfied while still reserving 64 KiB — the same shape as every other defect
this campaign has found, a value that looks applied and is not. If the floor
stays, the knob should REJECT 0 rather than round it up.

Baseline confirming the arithmetic, `just mem-report` on
`build/cargo-fixtures/linux-14372940/nros-relwithdebinfo/talker`:

```
       131,072   35.8%  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
       131,072  LARGE_PAYLOADS  — agrees with `ZPICO_MAX_LARGE_SUBSCRIBERS *
                                  ZPICO_SUBSCRIBER_RING_DEPTH * ZPICO_SUBSCRIBER_LARGE_SIZE`
```

Removing the floor is not free to reason about: `.max(1)` exists so a pool
index is always valid, and the sibling `max_nodes` floor documents exactly that
intent ("so a session always has room for its own primary node"). A zero-length
pool needs the *lookup* path to refuse a large subscription rather than index an
empty array — a code change, not a knob change. Whoever takes this should price
both halves separately: the knob wiring (65,536, mechanical) and the floor
removal (a further 65,536, needs the refusal path).

The same floor applies to `ZPICO_MAX_QUERYABLES`-style derivations. Check for it
before quoting a saving off the inventory: the inventory prints the formula, not
the floor.

## Reproduce

```sh
just build-test-fixtures lane=native
just mem-report build/cargo-fixtures/linux-3263301353/nros-relwithdebinfo/talker
just mem-report build/cargo-fixtures/linux-3263301353/nros-relwithdebinfo/service-server
```

Both print the same pool figures.

**Measure the fixtures, not `examples/**/target-*/`.** The first draft of this
issue took its numbers from
`examples/native/rust/talker/target-zenoh/nros-fast-release/talker`, which was
three weeks stale: phase 340 P2 moved fixture builds into the shared cargo group
under `build/cargo-fixtures/`, and the per-leaf directories are leftovers that
nothing rewrites. `just build-test-fixtures` reported success without touching
them. The pool figures happened to be unchanged, so the conclusion survived, but
the totals were wrong by 2,417–2,598 bytes and the "identical to the byte"
claim was wrong by 16. Trust the group directory, and check an artifact's mtime
against its sources before quoting it. `--json` plus `--baseline` shows the delta
between any two images, which is how a fix should be reported.
