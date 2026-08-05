---
id: 413
title: "Declarative Node API never registered Cyclone type descriptors (pubsub+services FIXED; actions open)"
status: open
type: bug
area: rmw
related: [phase-329, phase-337, issue-0233, issue-0234]
---

## ROOT CAUSE FOUND + FIXED 2026-08-05 (pubsub + services); actions still open

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

**STILL OPEN: actions.** `test_native_cyclonedds_rust_action` still fails. The
eight payload registrations are in place and mirror the imperative creator, so
the remaining cause is elsewhere on the action path; a plausible suspect is
issue **0418** (the action payload envelope carries one CDR header too many),
handled under RFC-0069. The pubsub and service halves are fixed and verified;
this issue stays open for the action half.

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
