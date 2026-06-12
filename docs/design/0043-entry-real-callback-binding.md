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
bodies"** item. The codegen Entry path runs **real user callbacks** by routing
to the existing Rust executor (the same one the imperative native examples use),
**not** by extending the type-erased declarative register seam. The user's
component becomes a **stateful object** that binds its real callbacks through the
thin typed/raw callback API — bound **by identity, never by a string name**. The
synthesizing C++ `EntryNodeRuntime` interpreter and the `DeclaredNode` /
`record_callback_effect` string-descriptor layer are **retired** — they were a
v1 scaffold that existed only because the declarative register had no callback to
hand the executor.

This honours two standing principles: **(1) no user-named callbacks** — a
callback is bound by being written, like the imperative API and the Rust
`nros::node!()` macro; **(2) C/C++ = thin Rust wrapper** ([RFC-0019](0019-nros-c-thin-wrapper-discipline.md))
— the C++ Entry surface holds no runtime logic; the Rust executor
([RFC-0041](0041-unified-callback-receive-model.md)) does all dispatch.

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
no new seam is required:

- The executor dispatches an **identity-bound callback** fed by a QoS-depth
  `BufferStrategy` ([RFC-0041](0041-unified-callback-receive-model.md);
  `packages/core/nros-node/src/executor/arena.rs`).
- The C++/C FFI already exposes the registration: typed
  `node.create_subscription(sub, topic, cb)` /
  `node.create_timer(t, ms, cb, ctx)` /
  `node.create_service(srv, name, handler)`, and the **raw, zero-copy**
  `nros_cpp_subscription_register(node, topic, type, hash, qos, cb, ctx, …)`
  whose callback is `void(const uint8_t* data, size_t len, void* ctx)` —
  borrowing the wire bytes, no copy, no typed header
  (`packages/core/nros-cpp/include/nros/subscription.hpp:29`;
  [RFC-0010](0010-zero-copy-raw-api.md), [RFC-0038](0038-zero-copy-data-transport.md)).

The three receive flavours all sit on this one primitive — none names a callback:

| flavour | callback signature | copy |
|---|---|---|
| untyped / zero-copy | `(const uint8_t* data, size_t len, void* ctx)` | none |
| borrowed-typed | `(const M<'a>&)` via `DeserializeBorrowed` over the slot | none |
| typed | `(const M&)` — trampoline deserializes (opt-in) | one |

### Spike (2026-06-12) — the one unproven edge, now validated

The executor's callback path was proven on **native** (imperative examples) but
never exercised on embedded — the embedded Entry path always ran the
interpreter. A throwaway imperative NuttX entry (init → `create_node` →
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
wrapper. The architectural risk is retired.

## Design

Three pieces replace the interpreter; none introduces a callback name.

### 1. Component = stateful object

A component is no longer a static `register_node(NodeContext&)` emitting string
descriptors. It is an **object** that owns its entity handles + state as members
and binds real callbacks by identity:

```cpp
class Talker {                       // arena-owned instance
    nros::Publisher<Int32> pub_;
    nros::Timer timer_;
    int count_ = 0;
    void on_tick() { Int32 m; m.data = count_++; pub_.publish(m); }  // real body, unnamed
  public:
    explicit Talker(nros::Node& node) {            // ctor binds — no configure() ceremony
        node.create_publisher(pub_, "/chatter");
        node.create_timer(timer_, 1000, [this]{ on_tick(); });      // bound by identity
    }
};
NROS_NODE(Talker);   // emits factory + sizeof + the per-pkg register symbol
```

Zero-copy / untyped subscribers bind the raw form
(`node.create_subscription_raw(sub_, topic, [this](const uint8_t* d, size_t n){…})`)
— same no-naming, no deserialize. This mirrors Rust's `nros::node!()` (instantiate
a real struct, executor owns it) and ASI's `common/node` shim (RFC-0032 §8a's
reference implementation).

### 2. Typed codegen Entry routes to the executor

The launch-driven Entry codegen (`packages/cli/nros-cli-core/src/codegen/entry/`)
shifts from emitting a type-erased `__nros_component_<pkg>_register(NodeContext*)`
call to a **typed** entry: per launch node, `#include` the component header,
construct the component into an entry-owned arena slot (`sizeof` known via the
include), and run the **real executor** (`nros::init → spin_once loop →
shutdown`). The `NodeContextOps` recording dispatch + the synthesizing spin loop
are gone; `spin_once` drives the executor's RFC-0041 callback dispatch. This is
the necessary codegen boilerplate (launch → construct), kept minimal.

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
- The Phase 238 NuttX C/C++ E2E migrates onto the executor and runs **real**
  logic for free (the synthesized counter / `a+b` / fixed result were stand-ins
  for exactly the callbacks this RFC binds).

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

## Changelog

- 2026-06 — created. Resolves RFC-0032 §8a "callback bodies" open item; informed
  by the no-naming + thin-wrapper (RFC-0019) principles and the 2026-06-12 NuttX
  executor-callback spike. Tracked by phase-236 (236.D).
