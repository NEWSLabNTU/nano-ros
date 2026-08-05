---
id: 413
title: "Declarative Node API never registered Cyclone type descriptors"
status: resolved
type: bug
area: rmw
related: [phase-329, phase-337, issue-0233, issue-0234]
resolved_in: "phase-337 session"
---

## ROOT CAUSE FOUND + FIXED 2026-08-05 — pubsub, services AND actions

**The declarative Node API never registered Cyclone type descriptors.**

Cyclone resolves topic types through a RUNTIME registry: `publisher.cpp` calls
`find_descriptor(type)` and returns `NROS_RMW_RET_UNSUPPORTED` when it is
absent. The IMPERATIVE API's typed creators
(`nros-node::Node::create_publisher_with_qos::<M>`) call `register_type::<M>()`
first, so they were fine. The DECLARATIVE Node API does not reach them:
`NodeContext` records `EntityMetadata` and the sink calls the type-ERASED
`create_generic_publisher_with_qos(topic, type_name, type_hash, qos)`, which has
a type NAME and no `M` — so nothing could register the descriptor.

That is why every symptom looked the way it did:

* **C and C++ were never affected** — they use the static `descriptors.cpp`
  table, so `c/talker` published normally against the same backend.
* **zenoh / XRCE were never affected** — no descriptor registry; the seam
  (`nros_rmw::register_type_descriptor`) returns `Ok` when no registrar is
  installed.
* **It surfaced only now** because every native Rust example was
  `[package.metadata.nros.application]` (imperative) until phase-338 W3 made
  them Node-class.

**Why it took three sessions to place.** The cause is four collapses away from
the message: `NROS_RMW_RET_UNSUPPORTED` ->
`TransportError::PublisherCreationFailed` -> `decl_err_from_node`'s
`_ => NodeDeclError::Runtime` -> the macro's
`map_err(|_| RuntimeError::NodeRegister(<pkg>))`. The operator sees only
`application error: NodeRegister("native_rs_talker")`, which names the package
and nothing else.

**The fix** registers the descriptor at the last point that still knows the
type — the declarative API in `packages/api/nros/src/node.rs`:

| Declarative funnel | Registers |
|---|---|
| publisher | `M` |
| subscription | `M` |
| service server / client | `S::Request`, `S::Reply` |
| action server / client | `A`'s 8 wire types + `A::register_protocol_types()` |

The bounds are associated-type bounds on the declarative METHODS
(`S: RosService<Request: MessageForRmw, …>`), not on `RosService` / `RosAction`
themselves: those live in `nros-core`, which cannot depend on `nros-node` where
`MessageForRmw` is defined. `MessageForRmw` collapses to plain `RosMessage`
when no descriptor-needing backend is linked, so zenoh / XRCE builds are
unchanged.

**Verified** on freshly built cyclone binaries:

```text
rust talker   -> Publishing: 'Hello World: 1' …      (was: NodeRegister)
rust listener -> I heard: [Hello World: 1] …         (the pair delivers)
rust service  -> Result of add_two_ints: 5           (round-trips)
```

plus `native_api::test_native_cyclonedds_rust_talker_to_listener` (C and C++
peers) green, having been red.

**The action half: NOT #0418 — a missed sibling funnel.** The first pass added
the registration by pattern-matching the funnel bodies, which quietly reached
only ONE of the TWO `EntityKind::ActionClient` sites (the second spells its
metadata `id: EntityId::new(name)` rather than `id,`) and neither of two
`Subscription` funnels. The action SERVER worked; the CLIENT still failed
`NodeRegister`, which looked like a protocol problem and was really the same
missing registration one funnel over.

Fixed by ENUMERATING every `entity_metadata(EntityMetadataSpec { … kind: … })`
in the declarative API and asserting each message-typed one registers, instead
of matching text:

| funnel | registers |
|---|---|
| Publisher ×1, Subscription ×3 | `M` |
| ServiceServer ×1, ServiceClient ×1 | `S::Request`, `S::Reply` |
| ActionServer ×1, ActionClient ×2 | `A`'s 8 wire types + protocol types |
| Timer, Parameter | nothing — no message type, correctly excluded |

Textbook CLAUDE.md "fix the CLASS, not the reported site": three of the eleven
funnels were missed on the first pass, and the audit is what found them.

Full action round-trip on cyclone now:

```text
client: Sending goal / Goal accepted / Next number in sequence received: [0, 1, 1]
        Result received: [0, 1, 1]
server: Received goal request with order 1 / Executing goal / Publish feedback
        Goal succeeded
```

**One budget change rode along.** `native_example_pubsub_e2e` then TIMED OUT: it
is ONE test iterating nine cells (3 langs x 3 RMWs) in a single process, so its
wall clock is the sum of nine router-start + delivery waits — measured 93 s
against nextest's 60 s default kill. Same consolidation cost
`workspace_features_e2e` already carries a 120 s budget for. Left alone it is a
timeout that reads exactly like the delivery failure this issue was about.

**Verified green:** every `native_api` cyclone cell including
`test_native_cyclonedds_rust_action`, plus `native_example_pubsub_e2e` (92.9 s)
and `native_example_reqresp_e2e` (51.7 s). The remaining `native_api`
`threadx_linux_cyclonedds_*` failures are `[SKIPPED]` precondition panics —
they need `just threadx_linux build-fixtures`, a different lane.

## Diagnosis trail (2026-08-05) — kept, because it is what the collapse hides

`da26485e9` was right that the binaries were stale, and right that rebuilding
clears `Transport(ConnectionFailed)` at `Executor::open`. It was wrong that "the
code was always correct": with fixtures rebuilt from scratch on a fresh clone
(`just build-test-fixtures lane=native`, 2026-08-05), the Rust cyclone talker
gets FURTHER and still fails — and the Rust-vs-C asymmetry the issue was
originally filed about is intact.

Run by hand, same env (`LD_LIBRARY_PATH=build/install/lib`, a private
`ROS_DOMAIN_ID`, no router needed for cyclone):

```text
$ ./examples/native/rust/talker/target-cyclonedds/nros-fast-release/talker
nros: session open
nros: application error: NodeRegister("native_rs_talker")

$ ./examples/native/c/talker/build-cyclonedds/c_talker
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
...
```

So: the session OPENS (the stale half is genuinely fixed), and then the node
package's `register(runtime)` fails — `nros-build`'s emitted
`::<pkg>::register(runtime).map_err(|_| RuntimeError::NodeRegister(..))`. The C
talker on the same backend, same domain, same libddsc publishes normally. The
zenoh build of the SAME Rust source works against a live router, so this is
neither the Node-class migration nor the entry shape — it is cyclone-specific
and Rust-specific, which is exactly what this issue said in the first place.

Ruled out while re-diagnosing:

* **Not phase-337 W8.a fallout.** The merged `nros-board-linux` carries the
  cyclone arm of `register_linked_rmw()` byte-for-byte from the deleted
  `nros-board-native` (`#[cfg(feature = "rmw-cyclonedds")] { let _ =
  nros_rmw_cyclonedds_sys::register(); }`), and it now sits on the ONE hosted
  boot funnel, so it cannot be skipped by a boot path.
* **Not a missing registrar install.** `nros_rmw_cyclonedds_sys::register()`
  calls `nros_rmw_cyclonedds::install_descriptor_registrar()` before returning,
  and the board calls it before `Executor::open` — which is consistent with the
  session opening.
* **Not feature forwarding.** `examples/native/rust/talker`'s
  `rmw-cyclonedds` forwards all three of `nros/rmw-cyclonedds`,
  `dep:nros-rmw-cyclonedds-sys` and `nros-board-linux/rmw-cyclonedds`.

Next step is the one thing this run could not get: the ERROR the emitted
`map_err(|_| …)` throws away. `nros-build`'s emitter should carry the inner
error into `NodeRegister` (or log it) — a register failure that names only the
package is why two sessions have now mis-diagnosed this.

The cells `da26485e9` un-carved (`native_example_pubsub_e2e`,
`native_example_reqresp_e2e`, and 6 `native_api` cyclone cases) are red on
`main` because of this.

## Original resolution (kept — its stale-binary half is correct)

## Resolution — it was a stale fixture, NOT a code bug

Surfaced by the phase-329 W4 native-example consumers: a same-language rust
cyclone/xrce talker+listener pair delivered nothing while C/C++ pairs did.

Root cause: the rust cyclone/xrce example binaries were **7 days stale** — never
rebuilt because no test had ever exercised those matrix cells — and the stale
binary panicked `Failed to open session: Transport(ConnectionFailed)` at
`Executor::open` (a session-OPEN failure, downstream of which the listener
printed nothing and read as "no delivery").

Wiping the example `target-cyclonedds` / `target-xrce` dirs and letting the
fixture harness rebuild fresh made every cell deliver:
- `native_example_pubsub_e2e` 9/9 green, `native_example_reqresp_e2e` all
  service+action cells green.

So the code was always correct — verified along the way: `register_type::<M>()`
runs on both the plain and `message_info` subscription paths; the Cyclone
descriptor registrar is installed by the board's `nros_rmw_cyclonedds_sys::
register()`; the CFFI `try_recv_raw_with_info` already falls back to plain take +
optional info; and both `Cargo.toml`s carry the `nros/rmw-cyclonedds` marker.
The carves in both consumers were dropped (`da26485e9`).

## Recurrence note

The staleness was the "untested cell rots" class: a matrix cell that no runtime
lane exercised sat un-rebuilt while its runtime dependency (the cyclonedds
install lib) moved underneath it. Now the W4 consumers run these cells in
`test-all`, so cargo keeps them current. A clean build was always green. If it
recurs on an incremental tree, it is the general fixture-freshness discipline
(rebuild after a `build/install/lib` cyclonedds re-provision), not a code fault.
