# phase-461 -- the service inbox is sized per family, and the parameter family brings its own

**Status (2026-09-21). PLANNED, nothing landed.** Numbered highest + 6 because
five other phases were being opened concurrently (456-460 are theirs). Owns
[issue 1352](../issues/1352-param-set-request-exceeds-service-buffer.md), which
PR #1043 FILED (merged 2026-09-13) and did not fix. Completes the half of
phase-454 W6.a that its own text left open ("lowering it is a separate question
that needs the built-in service surface to become a declared one"), and is the
board blocker for the Autoware Safety Island on MR-CANHUBK344: the image is
over RAM by 53,912 B and the zenoh service inbox table is 115,128 B of it.

## Parallel plan

Six waves, one claim id each (`just claim phase-461-Wk`; advisory, TTL in
hours, an open PR supersedes it). W1 is the only wave that starts alone; W2
through W5 hang off it in the order the table gives, and W6 is last by the
text's own rule ("only after W1-W2 are the plan"). W1 and W4 both edit
`service.rs`, W2 and W4 both edit `parameter_services.rs`, and W1 and W3 both
edit the zenoh `build.rs`: each pair is too coupled to split further and is
serialised below rather than merged, because the second wave of each pair has
its own gate. Every path below exists in the tree today except the ones marked
new.

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-461-W1` | none | `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs`, `packages/rmw/zenoh/nros-rmw-zenoh/build.rs` (the two shim tables and their knob pairs; `declared_service_request_bytes` is W3's), `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs`, `packages/rmw/zenoh/zpico-sys/src/ffi.rs` and `packages/rmw/zenoh/zpico-sys/c/include/zpico.h` if the new header crosses the FFI, `zephyr/Kconfig` (the six inbox symbols, the `NROS_SERVICE_BUFFER_SIZE` help and alias), `zephyr/cmake/nros_cargo_build.cmake` (six ladder resolves beside :1208), `packages/api/nros/src/guide/configuration.rs` (the knob rows) | `just check ffi-struct-mirrors`; the `zenoh_integration` service and action tests; `just mem-report` on `bins/sim-clock-listener` | yes |
| `phase-461-W2` | `phase-461-W1` (the ring type and `InboxSpec`), `phase-460-W2` (the same function in `nros-params/build.rs`) | `packages/core/nros-rmw/src/` (the service-server seam and `SUPPORTS_CALLER_INBOX`), one const each under `packages/rmw/cyclonedds/nros-rmw-cyclonedds`, `packages/rmw/xrce/nros-rmw-xrce` and `packages/rmw/cffi`, `packages/core/nros-node/src/parameter_services.rs` (`PARAM_INBOX`, the two const asserts), `packages/core/nros-node/src/lifecycle_services.rs`, `packages/core/nros-node/build.rs` (the knob pair's carrier), `packages/core/nros-params/build.rs` only if the capacities need a new export | the const assert's negative control (a `-D` one byte short fails the build); `the_worst_messages_fit_the_derived_bound` with the inbox leg; `just check knob-single-reader` | no |
| `phase-461-W3` | `phase-461-W1` (the `NROS_RESOLVED_*` twin that `check-knob-delivery` demands for every `NROS_DERIVED_*`) | `packages/cli/rosidl-codegen/src/bounds.rs` (`BoundInventory::record_message` for `_Request` types), `packages/cli/nros-cli-core/src/entity_inventory.rs` (the two `NROS_DERIVED_*_INBOX_BYTES` rows), `cmake/NanoRosEntityInventory.cmake` (their carrier), `packages/rmw/zenoh/nros-rmw-zenoh/build.rs` (`declared_service_request_bytes`, :471-527, the floor removed) | `just check knob-delivery`; the island's `OperateMrm` request priced below 1,024 in `entity_inventory.cmake`; an uncapped unbounded member refuses naming it | yes for the pricing in `bounds.rs`; the carriers land after W1 |
| `phase-461-W4` | `phase-461-W1`, `phase-461-W2` | `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs` (the two counters, after W1), `packages/core/nros-node/src/parameter_services.rs` (the issue-1271 once-log arm, after W2), `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs` | the depth-1 double-request test (counter reads 1, one log line) and the 2,408 B overflow test | no |
| `phase-461-W5` | `phase-461-W1`, `phase-461-W2` (W3 optional: user services stay at 1,024 without it) | nano-ros half: `packages/cli/nros-cli-core/src/cmd/image_facts.rs` (the three inbox families with bytes, depth, origin), `book/src/internals/measuring-static-memory.md` (the three symbols and their formula); the island half is `island-W5` in the island's own phase doc | `just check knob-delivery`; `nros image-facts` on the island image; the image links | no |
| `phase-461-W6` | `phase-461-W1`, `phase-461-W2`; on `emit_cpp.rs` after `phase-459-W2`, `phase-462-W1` and `phase-463-W2`; on `params_shim.rs` after `phase-463-W1`; on `entity_inventory.rs` after `phase-460-W3` and `phase-461-W3` | `packages/cli/cargo-nano-ros/src/capability_resolver.rs` (the `params` axis), the `param-store` / `param-services` split in `packages/api/nros-cpp/Cargo.toml`, `packages/api/nros/Cargo.toml` and `packages/core/nros-node/Cargo.toml`, `packages/api/nros-cpp/src/params_shim.rs`, `packages/cli/nros-cli-core/src/entity_inventory.rs` (`param_nodes()`, :785-809), `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (:1086) and `emit_rust.rs` (`apply_param_store`), `packages/api/nros/src/lib.rs` (:1397, one assert per axis), the book's parameters page | `just check infra-queryable-counts` (`params` counts 0); the island bringup with `features = ["params"]` boots native_sim to VERDICT PASS; `nros image-facts` reports `max_queryables = 2` | no |

Files two waves touch, and the order they serialise in. Within this phase:
`service.rs` and `zenoh_integration.rs` are W1 then W4; `parameter_services.rs`
is W2 then W4; the zenoh `build.rs` is W1 then W3 (W1 owns the tables and
knob pairs, W3 the `declared_service_request_bytes` function); `entity_inventory.rs`
is W3 then W6. Across phases: `packages/core/nros-params/build.rs` is
phase-460 W2 first, then W2 here - 460 W2 is a one-day change to the same
capacity-reading function and W2 here reads what it leaves. `packages/cli/nros-cli-core/src/entity_inventory.rs`
is 460 W3 (the `NROS_ENTITY_PLAIN_TYPES` carrier), then W3 here (the derived
inbox carriers), then W6 here (`param_nodes()`); phase-463 W3 and W6 read
the inventory JSON and own no line of the file. `zephyr/cmake/nros_cargo_build.cmake`
is 460 W3 (the rx-ceiling block), then 460 W5 (the heap-gate comment), then
W1 here (six new resolves), then W5 here (pairing fixes only); disjoint
regions, so the later wave rebases, and two claims on the file are not held
open at once. `zephyr/Kconfig` is 460 W3, 460 W7 and W1 here on distinct
symbols in distinct menus: land order, no dependency. `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`
is phase-459 W2 (the tier-table tail), then phase-462 W1 (the monitor install
before entity creation), then phase-463 W2 (at most the native funnel call),
then W6 here (the registration call at :1086): W6 is behind W1-W2 anyway, and
it swaps a call the other three leave in place. `packages/api/nros-cpp/src/params_shim.rs`
is phase-463 W1 (the `on_param_declare` hook call in the
`nros_cpp_node_declare_param_*` family) then W6 here (the feature split);
`packages/api/nros/src/lib.rs` is phase-460 W1 (`load_for_build_script`) then
W6 here (:1397). Phase 457 shares no file with this phase.

## Why

`nros-rmw-zenoh` gives every queryable the same inbox: a ring of
`SERVICE_REQUEST_RING_DEPTH` (4) slots of `SERVICE_BUFFER_SIZE` (1,024) bytes
(`shim/service.rs:33-64`). A queryable is a service server, and since issue
1270 the parameter family is six of them per node. On the island that is 24 of
the image's 26 queryables, and the table is:

```
per queryable   4 x (1,024 + 12) + 284 = 4,428 B     (12 B per ring entry: len, seq,
                                                     overflow; 284 B fixed: 256 B reply
                                                     keyexpr, two cursors, waker, session)
table           26 x 4,428            = 115,128 B    measured in the map, exact
parameter share 24 x 4,428            = 106,272 B    (108,096 B when removed, measured)
```

Two things are wrong with one flat number, and they pull in opposite
directions, which is why neither half alone fixes it (issue 1352's own point):

1. **The size is checked against nothing.** A request larger than the slot
   sets `ServiceRequestSlot::overflow`, the payload is skipped
   (`service.rs:283-286`), and `take_request` pops it as
   `TransportError::MessageTooLarge` (`service.rs:530-533`). Nothing at build
   time computes what a parameter request CAN be. phase-446 F3 computes exactly
   that bound, but for the EXECUTOR-side buffer pair
   (`PARAM_SERVICE_BUFFER_SIZE`, `parameter_services.rs:1264`), one layer above
   the inbox the request has to land in first. The two buffers are sized by
   two unrelated numbers, and the lower one is the one that drops.
2. **The depth is the action path's.** Depth 4 was chosen for "a burst of
   queries delivered in one read-task batch -- concurrent goals under load"
   (`service.rs:28-32`), and it equals `ZPICO_MAX_PENDING_REPLIES`, the C
   shim's reply-slot table (`zpico.c:266`; `zenoh_integration.rs:803` records
   that they are equal on purpose). Parameter traffic is one request, one
   reply, from a client that waits. Sized per type at depth 4 the parameter
   family would cost MORE than today (37,336 B per node, issue 1352), so the
   depth is the half that makes per-type sizing pay.

phase-454 W6.a landed `declared_service_request_bytes` in
`nros-rmw-zenoh/build.rs:471-527`, which reads the sizing descriptor and may
RAISE `ZPICO_SERVICE_BUFFER_SIZE` and never lower it, for a reason it states:
the builtin families (`[param_services]` 6, `[lifecycle]` 5) receive through
the same pool and appear in no `[[endpoint]]` row, so sizing down to what the
app declared would under-size a surface the declaration cannot mention. That
is correct as far as it goes, and it is also why the flat table cannot shrink
until the builtin families stop sharing it. Two more facts fix the road this
has to travel:

- phase-454 W11 measured that the descriptor reaches ONE road of three. The
  cmake / Zephyr west road -- the island's road -- has no producer. So this
  phase derives on the working road, the `NROS_DECLARED_*` carriers that F3's
  `NROS_PARAM_SERVICE_SHAPE` already rides (`nros_cargo_build.cmake:1208`),
  and does not wait on W9 or a second descriptor producer.
- the parameter request bound is finished in nros-node and can be finished
  nowhere else without a second reader: it needs the store's capacities
  (`nros_params::MAX_*`), which meet the `[knobs.params]` board rung only in
  nros-params' build script. F3 put `param_service_bound()` beside the
  serializers as a `const fn` for that reason
  (`parameter_services.rs:1242-1270`). An RMW build script cannot price the
  parameter inbox; the crate that owns the family can.

### A correction to the numbers the island's report carries

Issue 1352 and `nxp-deployment.md` section 8 price `set_parameters` at
**2,408 B for 25 parameters on one node**. 25 is `NROS_MAX_PARAMETERS`, the
EXECUTOR's store capacity across all four nodes (21 declared + one
`use_sim_time` seed per node). No island node declares 25; the shapes the
inventory writes are `[4, 40]`, `[6, 69]`, `[8, 170]`, `[7, 103]` (params,
name bytes; `parameter_services.rs:3331-3341`). F3's own rule is that each
request addresses ONE node's six, so the worst NODE decides. Through
`node_bound` (`parameter_services.rs:1520-1552`; header 11, 8 B per CDR
string, 53 B per value):

```
set_request(node) = 11 + name_bytes + params x (8 + 53)
   [8, 170]  = 11 + 170 + 488 = 669 B     the worst island node
   [7, 103]  = 11 + 103 + 427 = 541 B
   [6,  69]  = 11 +  69 + 366 = 446 B
   [4,  40]  = 11 +  40 + 244 = 295 B
   25 x 35-byte names        = 2,411 B    the issue's figure, to within 3 B
```

So on this main a WELL-FORMED `set_parameters` to any island node fits the
1,024 B slot with room. The 2,408 B request is one naming 25 parameters at a
node that declares 8, which F3 already classes as "refused and logged rather
than sized for" -- except that at the inbox it is neither refused nor logged
but dropped with a flag, which is the defect. The island's report should be
corrected to say so (docs to update, below). The RAM half of 1352 is
unaffected by this and is the blocker; the drop half is real but its trigger
is a malformed client, not `ros2 param load` of the node's own file.

## What it does

Three families, three inboxes, each owned by the crate that knows its bound:

| family | who owns the inbox | slot bytes | ring depth | why |
| --- | --- | --- | --- | --- |
| parameters (6 per node) | nros-node, a `static` beside `ParamServiceBuffers` | `param_service_bound(shapes, caps).request_max()`, const-evaluated | 1 | one client, one request, one reply; see "why depth 1 is safe" |
| lifecycle (5) | nros-node, same shape | `LIFECYCLE_SERVICE_BUFFER_SIZE` (already an alias of the param size, `lifecycle_services.rs:547`) | 1 | same traffic class |
| user services | the RMW's table, as today | the declared service request bound, from the bound inventory; 1,024 until W3 prices `_Request` types | 4, knob | unchanged behaviour until a bound exists |
| actions (4 queryables per server, issue 1378) | the RMW's table | the declared action request bound, W3 | 4, knob | the family depth 4 was designed for; stays the twin of `ZPICO_MAX_PENDING_REPLIES` |

The RMW's `ServiceBuffer` stops being one monomorphic type with a baked
`[u8; SERVICE_BUFFER_SIZE]` and becomes a header over a caller-visible ring
(`slot_bytes`, `depth`, a pointer to the backing bytes). The zenoh shim's own
tables supply backing for the user and action families; a builtin family
registers with `InboxSpec::Caller(&'static InboxRing)` and supplies its own.
The SPSC contract does not change: the producer is the zenoh read task, the
consumer is the executor spin, for every family.

### Why depth 1 is safe for the parameter family

- The ROS 2 parameter clients are sequential: `ros2 param` and rclcpp's
  `SyncParametersClient` send one request and wait for its reply before the
  next; `AsyncParametersClient` callers await a future per call. One queryable
  therefore holds at most one request per client at a time.
- The six services of every node are polled serially in one spin
  (`ParameterServiceServers::process`, issue 1270's argument for ONE shared
  executor-side buffer pair), so a slot is drained within one spin period.
- Two clients addressing the SAME service of the SAME node inside one spin
  period is the only case depth 1 loses. The loser hits the ring-full arm
  (`service.rs:268`), which drops the newest. To the client that is a timeout
  and a retry, the failure mode every ROS 2 parameter client already handles;
  no data path, no control path and no contract field depends on a parameter
  request landing. W4 makes that drop COUNTED and said once, the way phase-455
  W1 did for the reply-slot table, so "safe" is observable rather than argued.
- The island's contract has no parameter client at all: values are launch
  seeds (`nros_cpp_declare_param` before construction), and `ros2 param set`
  is operator traffic. Depth 1 is a DEFAULT on a knob, not a ceiling; an image
  that serves a parameter dashboard states 2.

The action family keeps 4 because its reason still holds and because the C
shim's `stored_queries[ZPICO_MAX_QUERYABLES][ZPICO_MAX_PENDING_REPLIES]`
(`zpico.c:538`) is uniform per queryable; lowering that per family is a
follow-up this phase names and does not take.

### The knobs, on the ladder

RMW-agnostic names on the Kconfig / env side (phase-403's rule: one
backend-agnostic name, the backend-spelled token only at the consumer's rung),
each on the `env > Kconfig > derived > crate default` ladder
(`nros_cargo_build.cmake:194`):

| knob | rung | default | consumer |
| --- | --- | --- | --- |
| `NROS_PARAM_SERVICE_INBOX_BYTES` | plain ladder; `-1` DERIVE sentinel like `NROS_PARAM_SERVICE_BUFFER_SIZE` | derived: `param_service_bound(...).request_max()` rounded to 4 | nros-node `PARAM_INBOX` |
| `NROS_PARAM_SERVICE_INBOX_DEPTH` | plain ladder | 1 | nros-node `PARAM_INBOX` |
| `NROS_SERVICE_INBOX_BYTES` | derivable: max declared service `_Request` bound (W3); stated wins | 1,024 (today's `NROS_SERVICE_BUFFER_SIZE`, which this RENAMES with a one-release alias) | zenoh `USER_SERVICE_INBOX` via `ZPICO_SERVICE_INBOX_BYTES` |
| `NROS_SERVICE_INBOX_DEPTH` | plain ladder | 4 | zenoh, via `ZPICO_SERVICE_INBOX_DEPTH` |
| `NROS_ACTION_INBOX_BYTES` | derivable: max declared action request bound (W3) | 1,024 | zenoh `ACTION_INBOX` |
| `NROS_ACTION_INBOX_DEPTH` | plain ladder | 4 | zenoh; `check-knob-delivery` pairs it with `ZPICO_MAX_PENDING_REPLIES` |

`NROS_MAX_QUERYABLES` is unchanged: it still sizes the C shim's per-queryable
tables and the ENTITY count, and the inventory's `queryables()`
(`entity_inventory.rs:802-809`) still adds 6 per parameter node. What changes
is that the 24 parameter entries no longer index a `[ServiceBuffer;
ZPICO_MAX_QUERYABLES]`; the RMW table is sized `max_queryables -
infra_queryables` (both already on the inventory, `entity_inventory.rs:892-895`)
plus the action share.

### The gate

A `const` assert in nros-node, next to `param_service_buffer_bytes()`:

```rust
const _: () = assert!(
    PARAM_INBOX_SLOT_BYTES >= param_service_bound(shapes, ParamWireCaps::THIS_BUILD).request_max(),
    "NROS_PARAM_SERVICE_INBOX_BYTES is smaller than the largest parameter request the \
     contract's declared parameters can produce (set_parameters over the worst node); \
     raise it or drop the override"
);
```

When the inbox is DERIVED the assert is a tautology and costs nothing. When a
rung STATES the byte knob, or when `DECLARED_PARAM_SERVICE_SHAPES` is present
and the stated value is short, the build refuses and names the knob, the node
count N and the bytes. The same assert is added for the executor-side pair
(`PARAM_SERVICE_BUFFER_STATED` vs the derived `total()`), which today is only a
unit test (`parameter_services.rs:3384`). An image with no declared shape
cannot be gated this way and keeps the runtime once-log of issue 1271, now
extended to the inbox drop (W4).

### Expected RAM on the island, from the report's numbers

Today (nxp-deployment.md section 5): needed 381,592 B, available 327,680 B,
over by 53,912 B; inbox table 115,128 B.

After W1 + W2, at the contract's shapes:

```
parameter family  24 queryables x [1 x (672 + 12) + 284]  = 24 x 968  =  23,232 B
                  (672 = 669 B worst-node set_request, rounded to 4)
user services      2 queryables x 4,428 (unchanged until W3)            =   8,856 B
actions            0
new table                                                                 32,088 B
saving             115,128 - 32,088                                    =  83,040 B
needed             381,592 - 83,040                                    = 298,552 B
                   of 327,680 B = 91.1 %, headroom 29,128 B             LINKS
```

Two controls, so the reader can see which half pays:

- The report's own figure (six per-service sizes at the 25-parameter bound,
  depth 1, its flat 332 B overhead): 43,344 + 8,856 = 52,200 B; saving
  62,928 B; needed 318,664 B, 97.2 %, headroom 9,016 B. Links, barely. With
  the exact 284 + 12 overhead it is 42,480 B for the family.
- One family size at the STORE's capacity (2,408 B, depth 1): 24 x (2,408 +
  12 + 284) = 64,896 B; table 73,752 B; saving 41,376 B; needed 340,216 B --
  **still over by 12,536 B**. Sizing the family for a request no node can
  receive does not fit the board. The per-node shape is what makes it pay,
  and it is what F3 already computes.

Every figure above is a symbol the map prices; W5's acceptance is the map,
not this arithmetic. Not counted: the C shim's `stored_queries` (in the
"zenoh-pico session pool 24,992 B" line), unchanged by this phase.

## Waves

### W1 [rmw-zenoh] -- the inbox ring is a header over caller-visible storage

`ServiceBuffer` gains `slot_bytes` and `depth` fields and a pointer to its
ring bytes; `queryable_callback` and `take_request` index by those instead of
the two consts. The shim's own static tables become two, `USER_SERVICE_INBOX`
and `ACTION_INBOX`, sized by the two new knob pairs, and the buffer-index space
is partitioned by family at registration. The 12 B per-entry atomics stay per
entry; the 284 B fixed header stays per queryable.

Acceptance: `check-ffi-struct-mirrors` green; `zenoh_integration` service and
action tests green with the table split; `just mem-report` on
`bins/sim-clock-listener` shows `SERVICE_BUFFERS` replaced by the two symbols
and the sum unchanged for an image with no builtin family.

Claim: phase-461-W1. Depends on: none. Owns: packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs, packages/rmw/zenoh/nros-rmw-zenoh/build.rs (tables and knob pairs), packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs, packages/rmw/zenoh/zpico-sys/src/ffi.rs and c/include/zpico.h if the header crosses the FFI, zephyr/Kconfig (inbox symbols), zephyr/cmake/nros_cargo_build.cmake (six resolves beside :1208), packages/api/nros/src/guide/configuration.rs. Gate: just check ffi-struct-mirrors, the zenoh_integration service and action tests, just mem-report on bins/sim-clock-listener. Status: not started.

### W2 [nros-node, rmw] -- the parameter and lifecycle families bring their own inbox

`RmwServiceServer::create` (the RMW-agnostic seam) takes an `InboxSpec`:
`Backend` (today's behaviour, the default every user service passes) or
`Caller(&'static InboxRing)`. A backend answers `SUPPORTS_CALLER_INBOX` as a
const: zenoh `true`; cyclone, xrce, cffi `false` (they hold requests in reader
history, a C `req_ring`, or forward). nros-node picks at compile time and a
`Caller` spec on a backend that does not support it is a `const` refusal
naming the backend, never a silent fallback (RFC-0052: no dropped knob).

nros-node declares `static PARAM_INBOX: [InboxRing<PARAM_INBOX_SLOT_BYTES,
PARAM_INBOX_DEPTH>; PARAM_SERVICE_QUERYABLES * MAX_PARAM_SERVICE_SETS]` with
both consts from the knob pair above, the byte default being the `const fn`
bound. It is static, not heap, because phase-391 keeps payload buffers static
and because the heap gate (`arena + 24576 <= NROS_ZEPHYR_HEAP_SIZE`) would
otherwise need a new term. The lifecycle set does the same with its existing
size alias. The gate above lands here.

Acceptance: `the_worst_messages_fit_the_derived_bound` gains the inbox: the
worst `set_parameters` request for the island shapes is delivered through the
real zenoh queryable callback into `PARAM_INBOX` and taken, on the native
lane; a `-D` override one byte short fails the BUILD with the message above
(negative control). Cyclone and xrce parameter e2e cells unchanged.

Claim: phase-461-W2. Depends on: phase-461-W1, phase-460-W2. Owns: packages/core/nros-rmw/src/, one const each under packages/rmw/cyclonedds/nros-rmw-cyclonedds, packages/rmw/xrce/nros-rmw-xrce and packages/rmw/cffi, packages/core/nros-node/src/parameter_services.rs (PARAM_INBOX and the asserts), packages/core/nros-node/src/lifecycle_services.rs, packages/core/nros-node/build.rs, packages/core/nros-params/build.rs only if a new export is needed. Gate: the const assert's negative control, the_worst_messages_fit_the_derived_bound with the inbox leg, just check knob-single-reader. Status: not started.

### W3 [cli] -- service and action request types are priced

W6.a's refusal names it: `BoundInventory::record_message` sees `.msg` only, so
`pkg/srv/Name_Request` and `pkg/action/Name_SendGoal_Request` have no bound
and `wire_bound_bytes` is REFUSED on every service row. Price them the way
messages are priced (RFC-0033 capacities apply), write
`NROS_DERIVED_SERVICE_INBOX_BYTES` and `NROS_DERIVED_ACTION_INBOX_BYTES` on the
derivable ladder from the DECLARED service surface only (the builtins are no
longer in this pool, so W6.a's floor argument no longer applies and D7's
unfloored demand is restored). `declared_service_request_bytes` in the zenoh
build script reads the same fact on the descriptor road, minus its 1,024
floor.

Acceptance: the island's `OperateMrm` request is priced and the derived user
inbox lands below 1,024 in `entity_inventory.cmake`; a service whose request
carries an uncapped unbounded field refuses the derivation naming the member,
as the message road already does.

Claim: phase-461-W3. Depends on: phase-461-W1. Owns: packages/cli/rosidl-codegen/src/bounds.rs, packages/cli/nros-cli-core/src/entity_inventory.rs (the NROS_DERIVED_*_INBOX_BYTES rows), cmake/NanoRosEntityInventory.cmake, packages/rmw/zenoh/nros-rmw-zenoh/build.rs (declared_service_request_bytes). Gate: just check knob-delivery, the OperateMrm pricing and the unbounded-member refusal. Status: not started.

### W4 [rmw-zenoh, nros-node] -- an inbox drop is counted and said once

Two conditions, both silent today: the ring-full drop (`service.rs:268`) and
the overflow flag (`service.rs:283`). Both become counters per queryable
beside phase-455 W1's `zpico_reply_slot_stats` (Rust side, since the ring is
Rust), and one `nros_log` line per TRANSITION, not per spin (phase-444 W6's
rule). `take_request`'s `MessageTooLarge` is already distinct; the parameter
service's issue-1271 once-log gains an arm for it that names
`NROS_PARAM_SERVICE_INBOX_BYTES` and the request's length.

Acceptance: a test drives one parameter queryable with two requests in one
read-task batch at depth 1 and asserts the counter reads 1 and the log line
fires once; a 2,408 B request at a node declaring 8 parameters is counted as
an overflow and logged with both numbers.

Claim: phase-461-W4. Depends on: phase-461-W1, phase-461-W2. Owns: packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs (counters), packages/core/nros-node/src/parameter_services.rs (the issue-1271 once-log arm), packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs. Gate: the depth-1 double-request test and the 2,408 B overflow test. Status: not started.

### W5 [zephyr, downstream] -- the island links, and the map says why

Rebuild the MR-CANHUBK344 image on this phase; record the region report and
the three inbox symbols in `nxp-deployment.md` section 5's table beside the
knobs they trace to. The number to beat is 298,552 B needed; anything above
318,664 B means a control above was the one that shipped.

Acceptance: the image links; `nros image-facts` reports the three inbox
families with bytes, depth and origin (derived / stated); `check-knob-delivery`
pairs every new `NROS_DERIVED_*` with its `NROS_RESOLVED_*`.

The island half (the MR-CANHUBK344 rebuild and the section 5 table) is a
separate unit, `island-W5`, in the island's own phase doc; the nano-ros half
above is what this claim covers.

Claim: phase-461-W5. Depends on: phase-461-W1, phase-461-W2. Owns: packages/cli/nros-cli-core/src/cmd/image_facts.rs, book/src/internals/measuring-static-memory.md; the island half is island-W5 in the island's own phase doc. Gate: just check knob-delivery and nros image-facts on the island image. Status: not started.

### W6 [cli, api] -- the workaround: a store without a server

Second, and only after W1-W2 are the plan, because this is a capability the
island may not want to give up (`ros2 param get` on a safety island is how an
operator confirms a threshold).

The island's `system.toml` says that since phase-426 W4 the only store is the
one `param_services` links in, and that without it every component halts boot
at its first `declare_parameter` with code -16. **Both halves are still true
on this main, and for a narrower reason than the comment gives.** The store
itself is unconditional in nros-node: `ensure_parameter_store`
(`spin.rs:9339`) is reached from `declare_parameter_on` (`spin.rs:9516`)
whether or not `register_parameter_services` ever ran, and the six services
go up only when `ParamState::requested` is set (`spin.rs:8661-8677`). What is
gated is the C++ SHIM: `nros_cpp_declare_param` and its siblings are compiled
only under `feature = "param-services"` (`params_shim.rs:78`, the comment at
`:300-326`), and the unconditional stubs answer `NROS_CPP_RET_UNSUPPORTED`
(-16, `nros-cpp/src/lib.rs:244`), which `ComponentNode` makes boot-fatal
through `set_error`. So `features = []` halts boot not because there is no
store but because the C++ entry points to it were compiled out with the
server.

What a `features = []` image needs to declare parameters against a store with
0 queryables:

1. a second capability axis in `capability_resolver.rs` `CAPABILITIES`:
   `params` -> nros feature `param-store`, `c_define
   NROS_SYSTEM_PARAM_STORE`, no backend feature; `param_services` implies it.
   The nros and nros-cpp `param-services` cargo feature splits into
   `param-store` (the store, `declare`/`get`/`set`, launch seeds, the
   `use_sim_time` seed) and `param-services` (adds the six queryables);
2. the entity inventory's `param_nodes()` counts 6 per node only for
   `param_services`; `params` alone contributes 0 to `max_queryables`
   (`entity_inventory.rs:785-809`);
3. the entry emitters (Rust `apply_param_services`, C++
   `nros_cpp_register_parameter_services(__exec)`, `emit_cpp.rs:1086`) emit
   `apply_param_store` / `nros_cpp_register_parameter_store(__exec)` for
   `params`: `ensure_parameter_store()` plus the per-node `use_sim_time`
   seeds, WITHOUT `requested = true`. Launch `<param>` seeds keep going
   through `nros_cpp_declare_param`, unchanged;
4. `nros::main!`'s `PARAM_SERVICES_ENABLED` const-assert (`nros/src/lib.rs:1390`)
   becomes two, one per axis, so a `params` bringup on a build without the
   store feature still fails at compile time rather than at boot.

Store sizing is untouched: phase-446 W4's `NROS_MAX_PARAMETERS` and friends
derive from the contract exactly as today, and the A/B in the island's report
already shows the store costs 0 B either way.

Expected on the island: the 24 parameter queryables go, which the report
measured at 108,096 B: 381,592 - 108,096 = 273,496 B, 83.5 % of RAM. That is
the control column of section 8 (281,192 B) minus the 7,696 B kernel-heap
recovery since, so the two measurements agree.

**Stated limit.** With `params` and not `param_services`, `ros2 param
get|set|list|describe` returns nothing for the image's nodes, `ros2 param dump`
sees no node, and the `use_sim_time` runtime switch is unreachable (the seed
exists; nothing can set it). The bringup says which axis it declares, and the
contract stays the same file: which parameters exist is a contract fact,
whether they are served is a bringup fact.

Acceptance: `features = ["params"]` on the island's bringup builds, boots on
native_sim to VERDICT PASS with all four components past their
`declare_parameter` calls, and `nros image-facts` reports
`max_queryables = 2`; `ros2 param list` on that image is empty and the book
page for parameters says why.

Claim: phase-461-W6. Depends on: phase-461-W1, phase-461-W2, phase-459-W2, phase-462-W1, phase-463-W2, phase-463-W1, phase-460-W3, phase-461-W3. Owns: packages/cli/cargo-nano-ros/src/capability_resolver.rs, packages/api/nros-cpp/Cargo.toml, packages/api/nros/Cargo.toml, packages/core/nros-node/Cargo.toml, packages/api/nros-cpp/src/params_shim.rs, packages/cli/nros-cli-core/src/entity_inventory.rs (param_nodes), packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs (:1086), packages/cli/nros-cli-core/src/codegen/entry/emit_rust.rs, packages/api/nros/src/lib.rs (:1397), the book's parameters page. Gate: just check infra-queryable-counts, the island bringup with features = ["params"] to VERDICT PASS on native_sim, nros image-facts max_queryables = 2. Status: not started.

## Gates

- `check-knob-delivery`: every new `NROS_DERIVED_*` paired with its resolved
  twin (W3, W5).
- `check-knob-single-reader`: the parameter inbox bound has ONE finishing site
  (nros-node); no build script re-reads `NROS_MAX_*` (W2).
- `check-infra-queryable-counts`: still 6 per parameter node; W6 adds the
  `params` axis as 0.
- `check-ffi-struct-mirrors`: `ServiceBuffer`'s new header (W1).
- the `const` assert in W2, with its negative control in the same commit.
- `check-roadmap-claims` R1: this header says nothing landed and the body has
  no ticked box.

## Limits

- The C shim's reply-slot table stays uniform at `ZPICO_MAX_PENDING_REPLIES`
  per queryable. A per-family depth there is the natural next step and is
  named, not taken; phase-455 W1's counter says whether it matters.
- One ring PER QUERYABLE is kept. A single ring per FAMILY would be legal
  (the pair is SPSC for every queryable, so the family's requests still have
  one producer and one consumer) and would make the parameter family cost one
  slot per image rather than 24, but `take_request(handle)` would then have to
  dispatch by tag through `ParameterServiceServers::process`. Not in this
  phase; the arithmetic above is enough for the board.
- Cyclone and xrce do not get per-family inboxes here; W2 only makes their
  answer explicit.
- The descriptor road (phase-454 W9, a second producer) is not advanced; this
  phase derives on the carriers because that is the road the island's image
  is on.
- W6 is a workaround and says so; it is not a reason to skip W1-W2.

## Docs to update

- `docs/issues/1352-param-set-request-exceeds-service-buffer.md`: a line
  naming this phase (done with this doc); stays OPEN until W2 and W4 land.
  Its "25 parameters" arithmetic is the store capacity, not a node's; say so.
- `nxp-deployment.md` (island) sections 5, 8, 10: the corrected request
  bound (669 B for the worst node, 2,408 B only for a malformed client), the
  post-fix table, and W6's numbers and limit.
- `book/src/reference/static-pool-inventory.md`: the three inbox symbols and
  their formula, replacing the stated-reason non-annotation for
  `SERVICE_BUFFERS` (phase-454 W6.a).
- `packages/api/nros/src/guide/configuration.rs`: `ZPICO_SERVICE_BUFFER_SIZE`
  row becomes the three families.
- the island's `system.toml` comment on `param_services`: the reason is the
  shim's feature gate, not the store's existence; after W6 it names the
  `params` axis as the alternative.
- `zephyr/Kconfig`: `NROS_SERVICE_BUFFER_SIZE` help text and the new symbols.
