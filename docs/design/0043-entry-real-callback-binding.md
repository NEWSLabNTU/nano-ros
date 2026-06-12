---
rfc: 0043
title: "C++/C Entry real-callback binding — executor-routed, no callback naming"
status: Draft
since: 2026-06
last-reviewed: 2026-06-12
implements-tracked-by: [phase-236]
supersedes: []
superseded-by: null
---

# RFC-0043 — Entry real-callback binding (executor-routed, no naming)

## Summary

Resolves [RFC-0032 §8a](0032-entry-codegen-pipeline.md)'s open **"callback
bodies"** item. The codegen Entry path runs **real user logic** by routing to the
existing Rust executor (the same crate the native imperative API targets),
**not** by extending the type-erased declarative register seam. The user's
component becomes a **stateful object** that binds its real callbacks (where the
executor offers them) and drives the real poll API (where it doesn't yet) through
the thin typed/raw API — bound **by identity, never by a string name**. The
synthesizing C++ `EntryNodeRuntime` interpreter and the `DeclaredNode` /
`record_callback_effect` string-descriptor layer are **retired** — they were a
v1 scaffold that existed only because the declarative register had no callback to
hand the executor.

This honours two standing principles: **(1) no user-named callbacks** — a
callback is bound by being written, like the imperative API and the Rust
`nros::node!()` macro; **(2) C/C++ = thin Rust wrapper** ([RFC-0019](0019-nros-c-thin-wrapper-discipline.md))
— the C++ Entry surface holds no interpreter; the Rust executor + transport own
entity lifetime, buffering, and dispatch
([RFC-0041](0041-unified-callback-receive-model.md)). The C++ side is the
component object + a `spin_once` loop.

## Motivation / problem

Phase 235/236.A/B landed a C++ `EntryNodeRuntime` (in
`packages/core/nros-cpp/include/nros/main.hpp`): a ~700-line interpreter that
reads string descriptors from the declarative register, constructs entities via
the raw FFI, polls them in its own spin loop, and **synthesizes** fake logic
(monotonic `Int32` for a timer-publisher, `a+b` for a service, a fixed result
for an action — Phase 238 extended this to all three transports on NuttX). It
runs **no real user callback bodies**. RFC-0032 §8a flagged this as a hard
blocker: a real consumer (ASI's MPC/PID `Controller`) creates entities but no
control logic runs.

Two forces shape the resolution:

- **No callback naming.** Today the declarative API makes the user invent a
  string the runtime later matches:
  `declare_callback(on_tick, "on_tick"); create_timer(t, "1000", on_tick)`.
  The name carries no body — it is exactly the anti-pattern to remove. A
  callback should be bound by **identity** (the actual method/lambda), as the
  imperative API and Rust macro already do.
- **Thin Rust wrapper.** The synthesizing interpreter re-implements in C++ what
  the Rust executor already does (entity lifetime, the spin/dispatch loop,
  buffer draining). That violates [RFC-0019](0019-nros-c-thin-wrapper-discipline.md).
  The interpreter exists only as a *consequence* of the no-callback-body
  declarative model: no callback to register → nothing to spin → synthesize.

**The binding primitive already exists** and already carries everything needed —
no new seam is required. But two delivery models coexist in the FFI today, and
the RFC must not conflate them:

- **Callback-bound** (executor dispatches an identity-bound callback fed by a
  QoS-depth `BufferStrategy` — [RFC-0041](0041-unified-callback-receive-model.md);
  `packages/core/nros-node/src/executor/arena.rs`): **subscription, timer,
  service-server, action-server**. Verified FFI: `node.create_subscription(sub,
  topic, cb)`, `node.create_timer(t, ms, cb, ctx)`,
  `nros_cpp_service_server_register(node, …, cb, …)`,
  `nros_cpp_action_server_register(…)`, plus the **raw, zero-copy**
  `nros_cpp_subscription_register(node, topic, type, hash, qos, cb, ctx, …)` with
  callback `void(const uint8_t* data, size_t len, void* ctx)` — borrowing the
  wire bytes, no copy, no typed header
  (`subscription.hpp:29`; [RFC-0010](0010-zero-copy-raw-api.md),
  [RFC-0038](0038-zero-copy-data-transport.md)).
- **Poll-driven** (the component owns a `try_recv_*` loop inside the spin tick —
  still real logic, still on the real executor, no synthesis): **service-client,
  action-client** (`polling_action_client.hpp` — "caller drives `send_goal_raw` +
  `try_recv_*` from a spin loop"). [RFC-0041](0041-unified-callback-receive-model.md)
  converges these to callbacks later; until it lands, the component polls. This
  RFC is **independent** of RFC-0041 — poll today, callback once 0041 ships, both
  identity-bound and synthesis-free.

The subscription receive flavours (none names a callback):

| flavour | callback signature | copy | status |
|---|---|---|---|
| untyped / zero-copy | `(const uint8_t* data, size_t len, void* ctx)` | none | exists; spiked on NuttX |
| typed | `(const M&)` — trampoline deserializes (opt-in) | one | exists (`create_subscription(sub, topic, cb)`) |
| borrowed-typed | `(const M<'a>&)` via `DeserializeBorrowed` over the slot | none | Rust-executor reality; **C++ surface TBD** (the spike used raw bytes, not `M<'a>`) |

### Spike (2026-06-12) — pub/sub executor-callback path on embedded

The executor's callback path runs on **native**, but on embedded the Entry path
always ran the interpreter, never the executor — that was the architectural
risk. A throwaway imperative NuttX entry (init → `create_node` →
`create_timer(cb)` + `nros_cpp_subscription_register(raw_cb)` → `spin_once` loop,
~10 lines of C++ glue, built by a direct `nros-nuttx-ffi` cargo invocation) was
booted in QEMU against the talker:

```
SPIKE init -> 0 / create_node -> 0 / create_timer -> 0 / subscription_register -> 0
SPIKE tick 0..88        ← executor TIMER callback fires on NuttX
SPIKE Received 0..38     ← executor RAW zero-copy SUB callback fires (correct Int32)
```

So the Rust executor's real-callback dispatch (timer + raw zero-copy message)
runs under the NuttX board lifecycle via the C++ FFI, with the C++ side a thin
wrapper.

**Scope of the spike: pub/sub + timer only.** Service-server / action-server
callback dispatch and the poll-driven service/action **clients** under the
embedded board lifecycle are **not** spiked — they are expected (same FFI, same
executor, same `spin_once` pump) but unverified. A non-counter service/action
E2E on the executor (236.D.5) is the proof obligation before claiming the Phase
238 service/action examples migrate cleanly.

## Design

Three pieces replace the interpreter; none introduces a callback name.

### 1. Component = stateful object

A component is no longer a static `register_node(NodeContext&)` emitting string
descriptors. It is an **object** that owns its entity handles + state as members
and binds real callbacks by identity. The sketch below is **illustrative —
pending Open Q1 (ctor vs `configure`) + Q2 (instance ownership); `NROS_NODE` and
`create_subscription_raw` are PROPOSED, not current API** (the raw register
exists only as the FFI `nros_cpp_subscription_register`):

```cpp
class Talker {                       // arena-owned instance (Q2)
    nros::Publisher<Int32> pub_;
    nros::Timer timer_;
    int count_ = 0;
    void on_tick() { Int32 m; m.data = count_++; pub_.publish(m); }  // real body, unnamed
  public:
    explicit Talker(nros::Node& node) {            // ctor-binds — vs configure(Node&) (Q1)
        node.create_publisher(pub_, "/chatter");
        node.create_timer(timer_, 1000, [this]{ on_tick(); });      // bound by identity
    }
};
NROS_NODE(Talker);   // PROPOSED: emits factory + sizeof + the per-pkg symbol
```

Zero-copy / untyped subscribers bind the raw form (a proposed C++ wrapper over
`nros_cpp_subscription_register`, e.g.
`node.create_subscription_raw(sub_, topic, [this](const uint8_t* d, size_t n){…})`)
— same no-naming, no deserialize. This mirrors Rust's `nros::node!()` (instantiate
a real struct, executor owns it) and ASI's `common/node` shim (RFC-0032 §8a's
reference implementation).

### 2. Typed codegen Entry routes to the executor

The launch-driven Entry codegen (`packages/cli/nros-cli-core/src/codegen/entry/`)
shifts from emitting a type-erased `__nros_component_<pkg>_register(NodeContext*)`
call to a **typed** entry: per launch node, `#include` the component header,
construct the component into an entry-owned arena slot (`sizeof` known via the
include), and run the **real executor** (`nros::init → spin_once loop →
shutdown`). The recording dispatch + the synthesizing spin loop are gone;
`spin_once` drives the executor.

Two non-trivial mechanics this opens (Open Q5, Q6 — not yet in the 238 entry
codegen, which is type-erased by design):

- **Launch-node → concrete type + header.** The launch XML names a node by
  `pkg`/`exec` strings; today the entry mangles `pkg` into a register symbol. A
  *typed* entry needs the component's C++ class name + header path. The mapping
  source (pkg metadata → class → `#include`) must be defined.
- **Carrier / cmake migration.** The Phase 238 `nano_ros_node_register` carrier +
  the emitter produce/consume the `__nros_component_<pkg>_register` symbol;
  retiring it means rewriting both to emit a `#include` + construct instead of a
  register-symbol call. Real work, not a flag flip.

### 3. C path parity

A C component is a `struct` (state) + a configure fn that registers **C**
callbacks (`fn ptr + void* ctx`) on the same executor via the C FFI
(`nros_node_create_subscription(node, &sub, topic, type, c_fn, ctx)`). No naming,
same zero-copy. The Phase 238.C mixed build already links C nodes into the C++
entry, so the C path inherits the executor route unchanged.

### Retirement

- Delete the C++ `EntryNodeRuntime` interpreter + the `detail::entry_*` synthesis
  helpers (`main.hpp`).
- Retire `DeclaredNode` / `DeclaredCallback` / `record_callback_effect` and the
  `NodeEntityDescriptor` string-descriptor `NodeContextOps` seam — no consumer
  once callbacks are real. (RFC-0032 §8a named this seam the binding point; this
  RFC supersedes that sub-decision: the binding point is the executor callback
  registration, not the recording op set.)
- The Phase 238 NuttX C/C++ E2E migrates onto the executor + real bodies. **Free
  for pub/sub** (spiked); **expected-but-unspiked for service/action** (servers =
  callback, clients = poll until RFC-0041) — gated on the 236.D.5 E2E. The
  synthesized counter / `a+b` / fixed result were stand-ins for exactly the
  logic this RFC binds.

## Alternatives considered

- **B — Real callbacks across the type-erased ABI.** Keep the declarative
  register + generic runtime, but carry a C `callback_fn + void* ctx` in the
  `NodeEntityDescriptor` instead of a name string; the runtime invokes it. This
  satisfies no-naming and is smaller, **but** it keeps a type-erased middle layer
  the no-naming principle is trying to delete, re-implements executor dispatch in
  the C++ runtime (violates RFC-0019), and leaves two runtimes (interpreter +
  executor) to maintain. Rejected: it bridges to the executor instead of using
  it.
- **Convention-named callbacks** (a fixed interface like `on_message()` /
  `on_timer()` the framework calls by a known name). Avoids user-invented
  strings but caps each node at one callback per kind; real nodes have several
  subs/timers. Rejected: doesn't scale, and "well-known name" is still a name.
- **Keep RFC-0032 §8a's recording-op binding** (map each recorded entity to an
  `nros-cpp` construction at register time). This is what 236.A/B built; it
  cannot carry a body across the string seam, which is the whole problem.
  Superseded here.

## Open questions

1. **Macro / ctor ergonomics.** Ctor-binds-`Node&` (above) vs an explicit
   `configure(Node&)` (two-phase, allows fallible setup to return `Result`).
   Ctor is closest to Rust's `node!()`; `configure` gives an error path. Pick one
   for `NROS_NODE`.
2. **Instance arena ownership + sizing.** Entry-owned arena slot per launch node
   (typed entry → `sizeof` known) vs executor-owned. Confirm the `no_std`
   `alloc`-free arena story (RFC-0032 §8a's "entity handle storage" open item —
   ASI used `shared_ptr`; the Entry runtime needs an arena equivalent).
3. **Typed-entry header coupling.** The typed entry `#include`s each component
   header. Acceptable for the embedded codegen (Rust + ASI both do it); confirm
   it composes with the Phase 238 mixed C/C++ carrier + the launch-libs sidecar.
4. **Parameter sequences.** Orthogonal but inherited (RFC-0032 §8a): the
   `ParameterServer` is scalar-only; a real `Controller` needs `double[]` weights
   — a separate phase, not gated here.
5. **Launch-node → C++ type/header resolution.** How the codegen maps a launch
   `<node pkg=… exec=…>` to the component's concrete class name + header path to
   `#include` + construct (today only the mangled register symbol is derived).
   Needs a metadata source (pkg → class).
6. **Carrier / emitter migration.** Rewriting `nano_ros_node_register` + the entry
   emitter from "emit a register-symbol call" to "emit `#include` + construct +
   executor spin", and deleting the register symbol they currently
   produce/consume. Scope + staging (can the 238 examples flip per-transport?).
7. **C component in a typed C++ entry.** A C node is a `struct` + C fns, not a
   C++-constructible class. Define the seam the typed entry uses to instantiate +
   configure a C component (a C factory + `configure(node)` the entry calls?),
   esp. under the 238.C mixed build.
8. **Service/action on the executor under the embedded lifecycle.** Unspiked
   (§Spike). Prove service-server/action-server callback dispatch + the poll
   clients boot + exchange on NuttX before retiring the 238 synthesis for those
   transports (236.D.5).
9. **C++ borrowed-typed surface.** The zero-copy *typed* flavor (`const M<'a>&`
   via `DeserializeBorrowed`) is Rust-executor reality but may lack a C++
   wrapper; the spike only exercised raw bytes. Confirm or scope it out of v1.

## Changelog

- 2026-06 — created. Resolves RFC-0032 §8a "callback bodies" open item; informed
  by the no-naming + thin-wrapper (RFC-0019) principles and the 2026-06-12 NuttX
  executor-callback spike. Tracked by phase-236 (236.D).
- 2026-06 — review pass. Split callback-bound (sub/timer/service-server/
  action-server) from poll-driven clients (service/action, until RFC-0041);
  scoped the spike to pub/sub + timer (service/action embedded path unspiked);
  marked `NROS_NODE` / `create_subscription_raw` as proposed + the component
  sketch illustrative-pending-Q1/Q2; added Q5–Q9 (launch-node→type resolution,
  carrier/emitter migration, C-component construction, service/action executor
  proof, C++ borrowed-typed surface).
