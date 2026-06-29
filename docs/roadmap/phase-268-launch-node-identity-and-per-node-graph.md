# Phase 268 — Launch-authoritative node identity + per-node graph nodes

Implements **[RFC-0046](../design/0046-launch-authoritative-node-identity.md)** and
**[#105](../issues/0105-multi-node-per-node-graph-naming.md)**. Builds on #104 (node liveliness
token) + phase-266 (boot-config / single-node naming).

## Why

A multi-node launch shows ONE node in `ros2 node list` (the primary session's `/node`), not one
per component — every same-rmw component reuses the primary session (`node_record.rs:200`). And
Rust node names are **hardcoded** in node code (`create_node(NodeOptions::new("talker"))`), so the
launch `<node name=>` has no authority over a Rust node. Goal: each component is its own graph
node, **named from the launch**, uniform across Rust/C/C++.

Two findings ground the design:
- One zenoh session CAN host N graph nodes — the NN keyexpr
  `@ros2_lv/<domain>/<zid>/0/0/NN/%/<ns>/<node>` (`shim/mod.rs:410`) identifies by node NAME, not
  session id (`0/0` = protocol version). So per-node tokens on the shared session suffice; no
  session-per-node.
- Both languages create nodes through the one shared site `Executor::node_builder(name).build()`
  (`node_record.rs:270`) — Rust via `NodeContext::create_node`, C/C++ via `nros_cpp_node_create_ex`
  (`nros-cpp/src/lib.rs:1007`). So both the name resolution and the token declaration live there,
  once.

## Design decisions (RFC-0046)

- **Precedence (per field, all languages):** launch `name=` > code default (`create_node` arg) >
  `exec=` > `"node"`; namespace: launch `namespace=` > code default > `"/"`. Launch is
  authoritative (rclcpp-style override); code default is the fallback for direct-API/standalone use.
- **One resolution + declaration site:** `Executor::node_builder` / the create path in `nros-node`.
- **Rust feeds it the launch name** via per-component injection (mirror the W4a `runtime.params`
  rail). **C/C++ already feed it the launch name** (codegen bakes `n.name.unwrap_or(exec)`), so
  they need no new injection — they already conform once the token declaration lands at
  `node_builder`.
- Gate the #104 primary `/node` token OFF when ≥1 named component node exists (else a 2-node entry
  shows `/node` + `/talker` + `/listener`).

## Waves

### W1 — Rust: launch node-identity injection + `create_node` override
**Files:** `packages/core/nros-macros/src/main_macro.rs` (mirror `node_param_bakes` →
`node_identity_bakes`: collect each component's `(name, namespace)` from the parsed launch node;
emit `runtime.node_identity = Some(("talker", "/ns"))` before each `<pkg>::register` call, reset
to `None` for the self-bringup arm); the runtime carrier (`nros-platform` `RuntimeCtx` / the
`NodeRuntime`/`ExecutorNodeRuntime` in `packages/core/nros/src/node_runtime.rs`) gains a
`node_identity: Option<(&str,&str)>` slot; `NodeContext::create_node` /
`ExecutorNodeRuntime::create_node` resolves **injected identity > `NodeOptions` arg** before
calling `node_builder`.

- Precedence: if `runtime.node_identity` is `Some`, its name/namespace override the
  `NodeOptions::new(default)` values; else the `NodeOptions` values stand.
- `node_instances` (already parsed, `main_macro.rs:~599`) carries each component's name; thread the
  launch namespace too (parse `<node namespace=>`, default `"/"`).

**Acceptance:** unit test — a node whose code calls `create_node(NodeOptions::new("default"))`,
run with an injected identity `("launched", "/ns")`, registers as `launched` in `/ns` (override);
with no injection, `default` stands. `cargo test -p nros / -p nros-node` green. `nros::main!`
expands with the new emit (a hosted entry builds).

### W2 — per-node liveliness token at `node_builder` (graph half) + #104 gate
**Files:** `packages/core/nros-rmw/src/traits.rs` (add a `Session` trait method
`declare_node_liveliness(&mut self, domain_id, namespace, node_name) -> Option<LivelinessToken>` or
the crate's token type — default no-op for backends without it);
`packages/zpico/nros-rmw-zenoh/src/shim/session.rs` (impl it — the concrete method exists at
`:287`); `packages/core/nros-node/src/executor/node_record.rs` (`NodeRecord` += `node_liveliness`
field; `NodeBuilder::build()` declares the token for the node's resolved name + namespace via the
session and stores it — held for the node's lifetime; works even when the session slot is the
reused primary); the #104 primary-token gate in `ZenohSession::new` (or wherever the primary token
is declared) — suppress it when the executor will declare ≥1 named component node.

- Each `create_node` (Rust + C/C++, both via `node_builder`) now declares a per-node NN token with
  its resolved (launch) name → distinct graph nodes on the shared session.
- Token storage: `NodeRecord` holds its `Option<LivelinessToken>`; dropping the record undeclares.
- The #104 gate: a single-node entry / entity-less primary keeps the primary token (#98 behavior);
  a multi-node entry shows only its components. Decide the gate signal (e.g. the primary token is
  declared lazily / suppressed once a named `create_node` lands, or the macro tells the runtime
  whether named components exist).

**Acceptance:** unit/integration where feasible; the load-bearing check is the W3 e2e. `just check`
green (incl. the zenoh shim build). Backends without node liveliness (xrce/cyclone) compile via the
default no-op.

### W3 — e2e verification (Rust + C++ + mixed, multi-node)
**Acceptance (the proof):**
- Multi-node **Rust** workspace entry (`examples/workspaces/rust`, launch talker+listener) →
  `ros2 node list` shows `/talker` + `/listener` (was one `/node`).
- Multi-node **C++** entry (`examples/workspaces/cpp`) → `/talker` + `/listener` (was `/node`).
- **Mixed** entry likewise.
- **Single-node** entry unchanged: `ws-params-rust` still `/param_talker`; no extra `/node`.
- A launch that sets a name DIFFERENT from the node-code default makes the LAUNCH name win
  (override verified).
Harness: zenohd on a unique tcp port + `NROS_LOCATOR` + the `build/rmw_zenoh_ws` overlay +
`ZENOH_SESSION_CONFIG_URI` (same as the #104 / phase-266 e2e). Rebuild the fixtures so they link the
changed runtime.

## Sequencing
W1 (Rust naming resolution — feeds `node_builder` the launch name) → W2 (token declaration at
`node_builder`, both languages + the #104 gate) → W3 (e2e). W2 depends on W1 only for the Rust name;
C/C++ already feed `node_builder` their launch name, so W2's tokens cover them immediately.

## Acceptance (phase)
- `ros2 node list` shows one graph node per launch component, named from the launch, for Rust, C++,
  and mixed multi-node entries; single-node unaffected; launch `name=` overrides the code default.
- One precedence rule, resolved at the single `node_builder` site; Rust no longer hardcodes the
  graph name (the `NodeOptions` literal is a fallback default).
- #105 resolved; RFC-0046 realized.

## Outcome (2026-06-29) — DONE

All acceptance met: a multi-node launch entry (C++ **and** Rust) shows one graph node per launch
component in `ros2 node list`, named from the launch, with the #104 primary `/node` gated off;
single-node unchanged; C++ pubsub e2e shows no routing regression.

The implementation needed **two fixes beyond W1/W2** that the original plan did not anticipate,
because W2's contained-shim approach was structurally insufficient for the hosted/CFFI path:

- **W2b (`4bf7c1820`) — the load-bearing fix.** Root cause of the multi-node collapse: the RMW CFFI
  `create_publisher/subscriber/service_*` trampolines read each entity's `node_name` from the
  **per-call `NrosRmwSession` view**, but `CffiSession` built that view with the **session's**
  open-time name (`make_view()`). One session hosts N graph nodes, so every entity collapsed onto the
  single session name (`/node`). #104 never exposed it (single-node: session name == node name). Fix:
  `CffiSession::entity_view()` threads each entity's `TopicInfo`/`ServiceInfo` `node_name`+`namespace`
  into the per-call view (fallback to session buffers when absent). **No vtable ABI/signature change,
  no `abi_version` bump, no backend edits** — every backend already reads `session->node_name`
  (Cyclone is GUID-based and ignores it). This is why no RFC-0035 amendment was needed (the
  "extend the vtable ABI" option was investigated and ruled out).
- **W2c (`42398b61e`) — C++ listener.** `nros_cpp_node_create` (the simple FFI the typed C++ entry's
  `nros::create_node` uses) left `node_id = 0` (unregistered), so the C++ raw-arena-subscription path
  (`add_arena_subscription_c_callback`, keyed off `node_id`) fell back to the session name — the
  listener collapsed while the talker (typed publisher reading `node_ref.name`) showed. Fix: register
  via `Executor::node_builder().build()` + store the `NodeId`, exactly like `_ex` and Rust (RFC-0046:
  every node funnels through `node_builder`). Routing is unchanged (`node_session_mut` → primary slot
  0 for no-rmw override); only the per-component identity is now correct.

Net data path (hosted/CFFI): launch name → `node_builder` (NodeRecord.name) → entity creation tags
`TopicInfo.node_name` → **W2b** carries it across the CFFI view → **W2** shim declares the per-node NN
token. On the direct/embedded path (no CFFI) W2 alone suffices (TopicInfo.node_name is intact).

## Risks / decisions
- **Token lifetime:** a dropped `LivelinessToken` undeclares — each per-node token MUST be held by
  its `NodeRecord` for the node's life (mirror the #104 session-held token).
- **#104 gate signal:** how the runtime knows "named components exist" to suppress the primary
  `/node` — settle in W2 (simplest: declare the primary token only if no `create_node` ran, or let
  the macro pass a flag).
- **Namespace mangling:** the NN keyexpr expects a mangled namespace; reuse the existing entity-
  liveliness namespace handling (empty → `/`).
- **C/C++ unchanged observable behavior** beyond now appearing per-component; the codegen already
  bakes the launch name.
