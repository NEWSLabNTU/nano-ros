# Phase 376 — RMW API parity: measure the gap to upstream, then close it

**Status (2026-08-23). W1 landed (the contract is derived and capability parity
is automated). W2 landed as MEASUREMENT ONLY: `scripts/rmw-abi-shape.py` compares
our vtable to upstream slot-by-slot and arg-by-arg. It reports **0 of 79** slots
matching name and args today. W3+ (the migration) is not started.**

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

---

# W2 — the target is the ABI's SHAPE, not just the capability (landed as measurement)

The brief sharpened after W1:

> Our ABI should look mostly identical to the official ABI except the RTOS
> revision. The revision can be done by adding or removing items, or fixing
> args. All RMW functions should go into the C vtable, generic over all
> backends.

That is a stricter target than W1 measured, and it invalidates two of W1's
buckets as end states:

* **`layer` (23) is no longer an answer.** "We answer it in `nros-node`" was
  acceptable for capability parity; it is not acceptable when every RMW function
  must be a vtable slot. Those 23 move from *covered* to *to be moved into the
  vtable*.
* **capability parity is not name parity.** `try_recv_raw` covers `rmw_take`
  perfectly well and shares nothing with it — not the name, not the argument
  list.

## The rule, made mechanical

Every contract symbol gets a vtable slot named exactly the upstream name minus
its `rmw_` prefix (`rmw_take` -> `take`), with upstream's parameters, unless the
difference is DECLARED with an RTOS reason. Deliberately mechanical: it needs no
authored name mapping, and a mapping with 88 entries is a place for a mistake to
hide. The slots live inside `nros_rmw_vtable_t`, so the type carries the
namespace and a `rmw_` on each member would stutter.

`scripts/rmw-abi-shape.py` checks exactly that.

## Where we are

| | |
| --- | ---: |
| contract symbols to mirror (88 less 9 declined) | 79 |
| slots matching name **and** args | **0** |
| slots present, args differ | 8 |
| no slot at all | 71 |
| declared RTOS additions | 11 |
| extra slots not yet declared (these are the renames) | 16 |

Zero is the honest starting number, and it is not as bad as it sounds: the 8
"args differ" and the 16 "extra" are the same slots seen from two directions —
they do the upstream job under a different name and a different signature.

## The argument differences are the interesting part

They are systematic, not incidental, and each is a real RTOS revision that
should be declared per slot rather than smoothed away:

| upstream | ours | why |
| --- | --- | --- |
| `const rmw_node_t *` | `nros_rmw_session_t *` | an image has ONE session opened once; upstream's context/node split has no target-side meaning |
| `const rosidl_message_type_support_t *` | `const char *` pkg + `const char *` type | no typesupport indirection on target — the type is baked by codegen |
| returns `rmw_publisher_t *` | returns `nros_rmw_ret_t`, entity is an OUT parameter | no runtime allocation: the caller owns the storage |
| `rmw_publisher_allocation_t *` | absent | pools are baked; there is nothing to pre-size |

## Migration plan (W3+)

Each wave keeps the tree green and moves the shape counter, so progress is
measured rather than asserted:

1. **Rename the 16.** `try_recv_raw` -> `take`, `publish_raw` -> `publish`,
   `pub_loan` -> `borrow_loaned_message`, and so on. Mechanical, but it touches
   every backend and both bindgen'd mirrors (RFC-0054: the header is the SSoT,
   `scripts/gen-abi-bindings.sh` regenerates, `check-abi-bindings` gates
   staleness). Declares the arg deviations above at the same time. Expected:
   0 -> 16 matching, 16 -> 0 undeclared extras.
2. **Move the `layer` set into the vtable** (~23 slots): `wait`, the guard
   conditions, serialize/deserialize, node create/destroy, init/shutdown. Each
   needs a decision about what "generic over all backends" means for something
   currently answered above the seam — `wait` in particular, since
   `Executor::spin_once` IS our wait and a vtable `wait` would sit under it.
3. **The graph slot** (13 symbols) and **QoS read-back** (7) — the W1 gaps, now
   with upstream names and signatures fixed in advance.
4. **Turn `--check` into a gate** on the `just check` line. Deliberately NOT
   wired today: it fails by construction until the migration lands, and a gate
   that cannot pass is a gate people learn to skip.

## Open question for W3, worth settling before the renames

Whether `nros_rmw_ret_t` should become value-identical to `rmw_ret_t`. Upstream
uses `RMW_RET_OK = 0`, `RMW_RET_ERROR = 1`, `RMW_RET_TIMEOUT = 2`; ours are
negative constants. "Mostly identical" argues for adopting upstream's values,
which is a one-time break of every backend and every caller — cheaper now than
after the renames land.

