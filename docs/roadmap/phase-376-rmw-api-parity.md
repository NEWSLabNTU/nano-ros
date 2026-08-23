# Phase 376 — RMW API parity: measure the gap to upstream, then close it

**Status (2026-08-23). W1 landed: the contract is derived and the comparison is
automated (`scripts/rmw-api-parity.py`, `--check` gates it). 88 contract symbols
— 29 vtable, 23 answered at another layer, 9 declined for RTOS reasons, and
**27 gaps**. W2+ (closing them) is not started.**

Goal, from the campaign brief: feature completeness against official RMW, where
every remaining difference is traceable to an RTOS design consideration and
nothing else.

## W1 — what the contract actually is (landed)

### The obvious comparison is wrong twice over

Comparing `rmw.h` against `nros/rmw_vtable.h` overstates the gap in one
direction and understates our coverage in the other.

**Upstream's headers overstate the contract.** The `rmw` package declares **177**
`RMW_PUBLIC` functions across 40 headers. Most are utilities rmw itself DEFINES
— allocators, error handling, `names_and_types` init/fini, qos string
conversions, `validate_*`. An implementation links those; it does not supply
them. Comparing against 177 manufactures ~90 phantom gaps.

**Our header understates ours.** `rmw_vtable.h` is the BACKEND seam — what a
zenoh or Cyclone backend plugs into. Much of what upstream calls rmw lives one
layer up (`Executor::spin_once` *is* `rmw_wait`), one layer down (Cyclone's
`graph.cpp`), or in codegen (serialize/deserialize). Comparing only the vtable
manufactures gaps for things we ship.

### So take it empirically

The contract is the set of `rmw_*` symbols a real implementation **defines**:

| library | `rmw_*` symbols defined |
| --- | ---: |
| `librmw_cyclonedds_cpp.so` | 88 |
| `librmw_fastrtps_cpp.so` | 88 |
| `librmw_zenoh_cpp.so` | 88 |
| **intersection** | **88** |

Three independent implementations, three different transports, **the same 88
symbols and not one private extra**. That is a far better definition of "what an
rmw must provide" than any reading of the headers, and it is re-derivable rather
than asserted: `scripts/rmw-api-parity.py --contract`. Recorded at
`docs/reference/rmw-implementation-contract.txt` so the comparison runs on a host
with no ROS.

### Where we answer them

| bucket | count | meaning |
| --- | ---: | --- |
| `vtable` | 29 | a slot in `nros/rmw_vtable.h` |
| `layer` | 23 | we answer it, elsewhere (each one names where) |
| `declined` | 9 | deliberately absent, RTOS reason recorded |
| `gap` | **27** | missing, and not for a defensible reason |

**61 of 88 covered.** The mapping is authored, not inferred — the tool's job is
to make an *unclassified* symbol impossible to ignore when upstream grows one or
a distro bump moves the set, so the parity claim cannot quietly stop being true.

## The 27 gaps, grouped by what closing them needs

### A. Graph / introspection — 13 symbols, one missing vtable capability

`rmw_get_node_names`, `_with_enclaves`, `rmw_get_topic_names_and_types`,
`rmw_get_service_names_and_types`, the four `*_by_node` variants,
`rmw_get_publishers_info_by_topic`, `_subscriptions_info_by_topic`,
`rmw_count_publishers`, `rmw_count_subscribers`,
`rmw_node_get_graph_guard_condition`.

This is one gap wearing thirteen names: **the vtable has no graph query at all.**
`service_server_available` is the single graph-derived answer we expose, and its
own doc comment shows every backend already tracks the underlying state — zenoh
via matched queryables, Cyclone via built-in topic readers, XRCE not at all.
Cyclone even has `nros-rmw-cyclonedds/src/graph.cpp` with node names and GIDs;
there is simply no portable seam to reach it.

RTOS consideration is real but **partial**: a full graph cache costs RAM that a
128 KiB target does not have, and XRCE genuinely cannot enumerate participants.
That argues for the answer being optional per backend — which the vtable already
expresses as a NULL slot meaning `UNSUPPORTED` — not for the capability being
absent from the ABI.

### B. QoS introspection — 7 symbols, one design decision

The six `*_get_actual_qos` and `rmw_qos_profile_check_compatible`.

We bake the REQUESTED profile and never read back the GRANTED one. On DDS these
differ whenever a writer and reader negotiate, and the difference is exactly what
a consumer needs to diagnose "why is nothing arriving". `rmw_qos_profile_check_compatible`
is a **pure function over two profiles** — no transport, no allocation, no
discovery — so it is hard to see an RTOS argument for its absence at all.

### C. Matched counts — 2 symbols

`rmw_publisher_count_matched_subscriptions`,
`rmw_subscription_count_matched_publishers`. Same family as A; every backend
tracks this to implement liveliness events.

### D. Callbacks for services and clients — 2 symbols

`rmw_service_set_on_new_request_callback`,
`rmw_client_set_on_new_response_callback`. We have `set_wake_callback` for
subscriptions only, so a service-heavy image polls where a subscription-heavy one
sleeps. No RTOS reason — the same primitive, two more call sites.

### E. GIDs — 2 symbols

`rmw_get_gid_for_publisher`, `rmw_compare_gids_equal`. Cyclone's `graph.cpp`
already has GIDs. The second is a pure comparison.

### F. Clean shutdown — 1 symbol

`rmw_publisher_wait_for_all_acked`. A reliable backend knows its unacked count;
without this, an embedded image that publishes and immediately halts cannot know
whether anything left the box.

## The 9 declined, and whether each reason holds

| symbol(s) | reason | holds? |
| --- | --- | --- |
| `rmw_{init,fini}_{publisher,subscription}_allocation` (4) | no runtime allocation to pre-size — pools are baked at build time | **yes**, this is the design |
| `rmw_{publisher,subscription}_get_network_flow_endpoints` (2) | enumerates OS-level flows (DSCP, multicast egress); zenoh-pico and XRCE have no such notion | **yes** |
| `rmw_subscription_{set,get}_content_filter` (2) | a DDS-only expression evaluator; would bloat every non-DDS backend | **yes**, though a NULL slot returning `UNSUPPORTED` would cost nothing and let a DDS backend answer |
| `rmw_set_log_severity` (1) | log level is a build-time constant; a runtime setter implies a mutable global | **defensible**, but it is a policy choice rather than a constraint |

## W2+ — not started

Proposed order, cheapest-and-most-useful first:

1. **D and the pure functions** (`qos_profile_check_compatible`,
   `compare_gids_equal`) — no ABI growth for the pure ones; D is one existing
   primitive at two more call sites.
2. **B** — a `get_actual_qos` slot. One slot, six upstream entry points.
3. **A + C + E** — the graph slot. The big one: needs a shape that a
   128 KiB target can decline and a Cyclone target can answer, which the NULL-slot
   convention already supports.
4. **F** — an unacked-count slot.

Each wave should move symbols from `gap` to `vtable`/`layer` in
`scripts/rmw-api-parity.py`, so the count in this doc's Status line is a
measurement rather than a claim.

## Running it

```
scripts/rmw-api-parity.py            # the report above
scripts/rmw-api-parity.py --check    # non-zero if anything is unclassified
scripts/rmw-api-parity.py --contract # re-derive from an installed ROS (in the box)
scripts/rmw-api-inventory.py         # the raw 177-function header inventory
```
