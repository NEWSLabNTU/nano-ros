---
id: 1498
title: "A Zephyr west entry sizes the zenoh queryable table for its
  transient-local publishers and not the retention pool beside it; the slot
  is a flat 1024 B; and zenoh-pico's cond pool has no floor"
status: open
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
