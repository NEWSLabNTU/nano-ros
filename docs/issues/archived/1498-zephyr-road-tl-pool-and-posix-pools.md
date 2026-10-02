---
id: 1498
title: "A Zephyr west entry sizes the zenoh queryable table for its
  transient-local publishers and not the retention pool beside it; the slot
  is a flat 1024 B; and zenoh-pico's cond pool has no floor"
status: resolved
type: bug
area: rmw, build, cli
severity: high
related: [1341, 1378, 1393, 1485, phase-455, phase-412]
found: 2026-09-25
---

# What happens

Measured on the Autoware Safety Island (a downstream Zephyr 4.4 west entry,
zenoh, QEMU `mps2/an385` and the S32K344 board image under Renode and on
silicon). Five publishers are `QoS(1).transient_local()` and the contract
states it. The image fails at boot:

    [nros] FATAL: node "stop_mode_operator" failed to construct at create_publisher_in (code=-100)

Three separate gaps, found in this order.

## 1. The pool never hears of the count the table counted

`EntityInventory::derive` adds one cache queryable per transient-local
publisher to `NROS_DERIVED_MAX_QUERYABLES` (issue 1378), so the table derived
31. `nros-rmw-zenoh/build.rs` sizes the retention pool
(`MAX_TL_PUBLISHERS`) from the sizing descriptor only, and a west entry names
no descriptor to cargo (issue 1393), nor any `NROS_DECLARED_TL_PUBLISHERS`
(that carrier is the CMake road's, `nros_entity_facts_env`). The pool fell to
`TL_PUBLISHERS_DEFAULT = 2`, and the third `create_publisher` found no slot.

Separately, and correctly: the rule (`transient_local_publishers_over`)
refuses to count when any publisher states no `durability`, and the table then
counts zero. The island stated durability on five of fourteen publishers, so
until it stated all fourteen the table stayed 26 too. That half is the
contract's to fix, and the island did.

## 2. The retention slot is 1024 B whatever is published into it

`ZPICO_TL_RETAIN_BYTES` defaulted to 1024. The island's five latched types
serialize to at most 105 B (`VelocityLimit`; the vehicle commands are 13 B).
With the pool sized right (5), the S32K344 image overflowed its RAM by 3,352 B.

## 3. zenoh-pico's condition-variable pool had no floor, and the mutex floor counted subscribers only

Every declared subscriber and queryable gets a sync group
(`_z_sync_group_create`), which takes one `pthread_mutex_init` and one
`pthread_cond_init`. `nros_cargo_build.cmake` floors
`CONFIG_MAX_PTHREAD_MUTEX_COUNT` at subscribers + 22 + 4 and nothing floors the
cond pool (Kconfig default 16). With gdb on `pthread_cond_init` under QEMU:
two fixed conds (the session's sync group, `zpico_open`), then exactly one per
subscriber and per queryable; the 8th subscriber failed with the pool at
`0x0000ffff`. With the cond pool raised, the 30th queryable failed on the
mutex pool at 64 (11 subscribers, 29 queryables, 24 other mutexes).

# The fix

1. The entity inventory publishes `NROS_DERIVED_TL_PUBLISHERS` (only when the
   rule states a count; a refusal is written as a comment), and the Zephyr
   resolver forwards it as `NROS_DECLARED_TL_PUBLISHERS` on the derivable
   ladder. `nros-rmw-zenoh/build.rs` reads that carrier as the pool's demand
   when no descriptor is named, the way `nros-zpico-build` already reads it
   for the table.
2. The message-bound join derives `NROS_DERIVED_TL_RETAIN_BYTES`: the largest
   `_TX` bound over the types the durability table marks transient-local,
   refused for an image with an action server (its `/status` type is in no
   table) or any unbounded such type. The resolver forwards it as
   `ZPICO_TL_RETAIN_BYTES`.
3. For an image whose queryable table is DERIVED, the resolver floors both
   POSIX pools at subscribers + queryables + fixed + 4 (conds: fixed 2;
   mutexes: fixed 24) and refuses the configure below either, naming the
   number.

Measured on the island's QEMU image with the fix: `MAX_TL_PUBLISHERS = 5`,
`TL_RETAIN_BYTES = 105`, `ZPICO_MAX_QUERYABLES = 31`, and at FirstSpin the
pools held 45 conds and 66 mutexes against floors of 48 and 70.

# Still open

- The leaf (sidecar) and CMake (declared) roads carry no retain-bytes
  derivation; `check-declared-fact-carriers` reports both as OPEN under this
  issue.
- The fixed overheads (2 conds, 24 mutexes) are measured on one image, as the
  existing mutex floor's 22 was.
- `_nros_load_derived_message_bounds` re-exports a hand-kept list that does
  not name `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`. That is why the island's
  `check-knob-delivery` has shown that one knob red since phase-412 W4 ("was
  DERIVED but NROS_RESOLVED_NROS_SUBSCRIBED_TYPE_BOUNDS never reached the
  resolver"). Not changed here: delivering it re-prices the executor arena of
  every Zephyr image that subscribes.

# Progress, 2026-10-01 (branch `fix/zephyr-heap-1424-1425-1498-1324`)

**"The fixed overheads are measured on one image" -- re-measured on a second
line, and one was wrong.** Zephyr 3.7 `native_sim/native/64`, gdb counting every
`pthread_mutex_init` / `pthread_cond_init` to `--stop_at=6` against a live
router (an init count is an UPPER bound on live slots, the safe direction):

| image | subs | qrys | mutex inits | cond inits |
| --- | --- | --- | --- | --- |
| c/talker | 0 | 0 | 11 | 4 |
| c/listener | 1 | 0 | 11 | 5 |
| c/service-server | 0 | 1 | 11 | 5 |

One cond per subscriber and per queryable is confirmed. The FIXED cond count is
**4** here against the island's 2, so the derived-table cond floor was spending
two of its four headroom slots on fixed demand it did not count.
`_nros_zpico_cond_overhead` is now 4 (the larger measurement). The mutex count
did not move with a sync group on this line (11 throughout), so the island's 24
stays the binding fixed term.

Still open, unchanged by this pass:

- The leaf (sidecar) and CMake (declared) roads still carry no retain-bytes
  derivation (`check-declared-fact-carriers` reports both OPEN under this issue).
- `_nros_load_derived_message_bounds` still does not re-export
  `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`. The reason given above for not
  delivering it ("re-prices the executor arena of every Zephyr image that
  subscribes") is weaker now that executor storage is `.bss` (issues
  1551/1568/1571), but it is still a size change across every subscribing Zephyr
  image and wants its own measured pass.
- Not measured: an mps2-an385 image's pool counts (only native_sim was run),
  and whether any in-tree DERIVED-table image now crosses the raised cond floor
  at its `CONFIG_MAX_PTHREAD_COND_COUNT=16` (subs + qrys must stay <= 8; the
  in-tree derived tables read 0-4 queryables, and a crossing is a configure
  FATAL_ERROR naming the knob, not a silent failure).

## Resolution (2026-10-02, branch `fix/zephyr-1498-1611-1613`)

Every item the two notes above left open is closed.

**1. The retention SLOT on the leaf and CMake roads.** Both now derive it with
the cmake road's own refusals, so the three roads agree about when an answer
exists:

- leaf (sizing descriptor): `nros_sizing_descriptor::transient_local_retain_bytes`
  -- the largest `wire_bound_bytes` over the publisher rows stating
  `transient_local`; refused when the TL count is not stated, when an
  `action_server` row is present (its `/status` type is in no row), or when a
  TL row is unpriced; absent at a count of 0. `wire_bound_bytes` is the
  type's RECEIVE bound (`BoundState::rx`, never below `tx`), so this road can
  only over-size the slot, by the transport framing and XCDR2 header.
- CMake (declared): `_nros_payload_facts_env` carries the derived
  `NROS_DERIVED_TL_RETAIN_BYTES` as `NROS_DECLARED_TL_RETAIN_BYTES`, ahead of
  the payload guards (the slot depends on the published types only).
- `nros-rmw-zenoh/build.rs` (`transient_local_retain_demand`) takes either as
  the rung under `ZPICO_TL_RETAIN_BYTES`, which still outranks both; the
  descriptor wins when both exist, as it does for the count.

`check-declared-fact-carriers` reports no OPEN road any more (it reported both
of these under this issue). Checked: `NROS_DECLARED_TL_RETAIN_BYTES=105` builds
`TL_RETAIN_BYTES = 105`, unset builds 1024; `cmake-message-bounds-tests.sh`
106/106 (three new Q cases: crosses even when the payload join refused, rides
beside the payload keys, absent stays absent); `nros-sizing-descriptor` 55/55
(two new cases).

**2. `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS` is delivered.** The loader no longer
keeps a list: it re-exports every `set(NROS_DERIVED_...)` the knobs file holds,
so a new fact needs no second edit. Measured on a real Zephyr image that
derives the join -- `examples/workspaces/cpp` `[image.zephyr]` (talker +
listener, `std_msgs/msg/Int32`, native_sim/native/64), built twice in one build
dir with only the loader swapped:

| | `check-knob-delivery` | `NROS_RESOLVED_NROS_SUBSCRIBED_TYPE_BOUNDS` | `NROS_EXECUTOR_SIZE` | `.bss` |
| --- | --- | --- | --- | --- |
| old loader | RED: "was DERIVED but ... never reached the resolver" | absent | 20,680 | 443,552 |
| new loader | every knob reached the compile | `std_msgs/msg/Int32=12` | 20,680 | 443,552 |

The "re-prices every subscribing image" concern did not materialise here, and
cannot raise a price anywhere: the table charges each subscription its OWN
type's bound instead of the image-wide maximum, and a type missing from it falls
back to that maximum, so delivery can only keep or lower a subscription's
region. This image has one subscribed type, which IS the maximum, so nothing
moved; it ran (`Published: 0..5`, `Received: 0..`). The Rust example confs no
longer state an executor backing (issue 1611), so no stated claim can be broken
by a smaller default either.

**3. Pool counts on mps2_an385** (QEMU, lan9118 slirp, live `rmw_zenohd`,
`gdb-multiarch` counting `pthread_{mutex,cond}_init` over 25 s; inits are an
UPPER bound on live slots):

| image | subs | qrys | mutex | cond |
| --- | --- | --- | --- | --- |
| c/talker | 0 | 0 | 9 | 2 |
| c/listener | 1 | 0 | 9 | 3 |
| c/service-server | 0 | 1 | 9 | 3 |
| rust/talker | 0 | 0 | 9 | 2 |

One cond per subscriber and per queryable holds on a second board. The FIXED
cond count is 2 on mps2 (the island's figure) and 4 only under native_sim's
NSOS, so the floor's fixed term stays 4, the larger. The mutex count did not
grow with a sync group on either line (9 here, 11 on native_sim), so the
island's 24 -- read off a pool that FILLED at 11 subscribers + 29 queryables --
remains the only measurement of the per-entity mutex demand and stays the
binding term; it over-provisions these small images, which is the safe side.
Recorded beside the floor in `zephyr/cmake/nros_cargo_build.cmake`.

The one in-tree DERIVED-table image built here (the cpp workspace, 1 sub + 1
queryable) needs a cond floor of 10 against 16; it configures.

Not measured: a real image whose retention slot is DERIVED on the leaf or CMake
road -- no in-tree leaf or CMake entry states a `transient_local` publisher
(the cpp workspace's one TL slot is 1572's worst case for an undeclared
durability, and the slot correctly refuses there: "the durability table names
0 transient-local publisher(s) and the inventory counts 1"). The per-entity
mutex demand on mps2 at the island's scale. Other DERIVED-table Zephyr images
were not built.
