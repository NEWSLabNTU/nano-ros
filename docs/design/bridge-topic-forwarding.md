# Bridge topic-forwarding (Phase 172)

Design for in-binary **topic-forwarding bridges**: a `[[bridge]]` in the root
`nros.toml` connects ≥2 RMW sessions (different rmw / domain / locator) and
relays declared topics between them — one process, multiple sessions, raw CDR
forwarding. Status: the config→plan layer landed (`PlanBridge` + `nros deploy`
`apply_bridges`, codegen `64effd0`); this doc designs the remaining
generator + executor (runtime) half.

## Related work (what it teaches)

- **ROS 2 `domain_bridge`** ([design](https://github.com/ros2/domain_bridge/blob/main/doc/design.md)) —
  one node per domain in a single process; forwards via **generic / serialized**
  pub→sub (no deserialization, no C++ type support needed); the relay is
  **event-driven** (the user's executor spins generic subscription callbacks);
  QoS is auto-matched from discovered publishers. No per-message loop guard —
  bridges are directional.
- **`zenoh-bridge-dds`** ([blog](https://zenoh.io/blog/2021-04-28-ros2-integration/)) —
  discovers DDS readers/writers, maps topic ↔ zenoh key; **loop prevention is
  topological** (ensure no direct DDS path between the two bridged sides, e.g.
  distinct domains / multicast off).
- **RTI Routing Service** ([core concepts](https://community.rti.com/static/documentation/connext-dds/current/doc/manuals/connext_dds_professional/services/routing_service/core_concepts.html)) —
  a *Route* = N inputs → M outputs via StreamReader/StreamWriter; events are
  processed **serially in a Session thread** by a pluggable *Processor*; the
  builtin processor forwards every live input sample to each output.

**Takeaways for nano-ros:** (1) forwarding is **raw/serialized** sub→pub — we
have `create_subscription_raw` / `create_publisher_raw` / `publish_raw`;
(2) the relay is **event-driven**, riding the executor's spin (not a separate
thread); (3) for **bidirectional** relay we need echo suppression — and unlike
the topology-based approaches above, nano-ros runs both sessions *in one
binary* (no topology separation), so we need **per-message** echo suppression,
which `nros-bridge::PubSubBridge` already implements (`bridge_origin`
attachment + FNV-1a dedup ring).

## nano-ros model

- **Sessions** come from the bridge's `connect` endpoints (a 3rd `SESSION_SPECS`
  source alongside transports/domains). `Executor::open_multi` opens one session
  per endpoint (already supports same-rmw/different-domain, no rmw-dedup).
- **A bridge node per endpoint session** hosts that side's raw sub + pub,
  created via `NodeBuilder::session_idx(slot)` — the K.5 selector — so each
  forwarding endpoint binds to the right session.
- **Type + QoS** for each forwarded topic are resolved from the plan's
  `interfaces` (the topic must be declared by some component — the chosen
  "resolve from plan interfaces" model; wildcard `"*"` is deferred, it needs
  runtime discovery). `interface_type_name` / `interface_type_hash` already
  exist in the generator; QoS rides the declared interface's profile.
- **Forwarding primitive:** `nros-bridge::PubSubBridge { sub, pubr, origin,
  dedup }` — `pump()` drains the sub, drops echoes (payload hash seen, or
  `bridge_origin` == own origin), and republishes with the origin attachment.
  One `PubSubBridge` per (topic, ordered session pair); bidirectional = two.

## The crux: driving the relay inside the spin

`PubSubBridge::pump()` must run each spin cycle. The buffered-raw **callback**
(`register_subscription_buffered_raw_on`, `FnMut(&[u8])`) can't carry the
`bridge_origin` attachment, so a pure callback relay can't do echo-safe
bidirectional forwarding. Two options:

- **Option A (recommended) — executor bridge registry + pump in `spin_once`.**
  `nros-node` gains a small registry (`Vec` of type-erased pumpables, or a
  fixed `heapless::Vec<PubSubBridge>` sized by `MAX_BRIDGES`) and `spin_once`
  calls `pump()` on each after the callback/timer pass. Reuses the **tested**
  `PubSubBridge` dedup wholesale; the only new executor surface is
  `register_bridge(PubSubBridge) + pump-in-spin`. Generated `register_bridges`
  builds each `PubSubBridge` (raw sub on session A node + raw pub on session B
  node) and registers it.
- **Option B — attachment-carrying raw-sub callback.** Add a
  `register_subscription_buffered_raw_on` variant whose callback gets
  `(&[u8] payload, &[u8] attachment)`; generated code re-implements the
  origin-tag skip + `publish_raw_with_attachment` per direction. More generated
  logic, re-derives dedup that `PubSubBridge` already has. Not recommended.

Option A keeps the dedup in one tested place and matches the RTI "serial pump in
the session thread" model (here: pumped in `spin_once`). It needs an executor
change in `nros-node` (the registry + spin hook), but a small, contained one —
analogous to how K.5's `NodeBuilder::session_idx` was the one small executor
primitive.

## Generator emission (sketch)

```
SESSION_SPECS  ← bridge.connect endpoints (rmw, locator, domain)   // + existing sources
build_executor_bridge() → open_multi(SESSION_SPECS)                // exists

register_bridges(executor):
  for bridge in PLAN.bridges:
    for topic in bridge.topics:
      (type_name, type_hash, qos) = resolve from plan.interfaces   // err if undeclared
      for (a, b) in ordered session pairs of bridge.connect:
        node_a = executor.node_builder("<bridge>_<a>").session_idx(a).build()
        node_b = executor.node_builder("<bridge>_<b>").session_idx(b).build()
        sub  = node_a.create_subscription_raw(topic, type_name, type_hash)   // on session a
        pubr = node_b.create_publisher_raw(topic, type_name, type_hash)      // on session b
        executor.register_bridge(PubSubBridge::new(sub, pubr, origin="<bridge>:<a>"))
register_all(...) { ...; register_bridges(executor)?; }
```

## Work breakdown

1. **nros-node** — bridge registry + `spin_once` pump (Option A). Small, contained.
2. **generator** — `SESSION_SPECS` from `connect`; `register_bridges` (raw sub/pub
   per (topic, session pair) via the K.5 selector + `PubSubBridge`); call it in
   `register_all`. Resolve type/QoS from `interfaces`; error on undeclared topic.
3. **plan** — DONE (`PlanBridge`).
4. **deploy** — DONE (`apply_bridges`).
5. **check** — drop the `[[bridge]]` half of `pending_routing_warning` once (1)+(2) land.
6. **tests** — generate-shape (bridge plan → `SESSION_SPECS` from endpoints +
   `register_bridges` + per-topic raw sub/pub); a nros-node pump unit test;
   runtime e2e is agent/HW-bound (2 RMW agents) — gate it.

## Deferred

- **Wildcard `"*"`** topic forwarding — needs runtime topic discovery + type
  resolution; out of scope (resolve-from-interfaces requires named, declared topics).
- **QoS reconciliation across mismatched publishers** (domain_bridge's
  majority-match) — nano-ros bakes the declared interface QoS; revisit if needed.
