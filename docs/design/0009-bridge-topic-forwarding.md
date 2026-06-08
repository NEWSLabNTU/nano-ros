---
rfc: 0009
title: "Bridge topic-forwarding (Phase 172)"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Bridge topic-forwarding (Phase 172)

Design for in-binary **topic-forwarding bridges**: a `[[bridge]]` in the root
`nros.toml` connects ≥2 RMW sessions (different rmw / domain / locator) and
relays declared topics between them — one process, multiple sessions, raw CDR
forwarding. The public API stays in the **rclcpp / rclrs / rclc shape** (the
`domain_bridge` pattern expressed in our mirrored client API), with nano-ros
add-ons layered on. **Status: DONE** — the config→plan layer (`PlanBridge` +
the codegen step `apply_bridges`, codegen `64effd0`) and the generator runtime half
(`register_bridges` + the node-centric relay on Phase 189.M1, the `[[bridge]]`
check warning dropped) have landed. Runtime e2e against two live RMW agents
stays gated. This doc records the design.

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
binary* (no topology separation), so we need **per-message** echo suppression.

## API shape: mirror rclcpp / rclrs / rclc, with add-ons

nano-ros mirrors the upstream client libraries (rclrs 0.7.0 / rclcpp / rclc);
the bridge **must stay in that shape**, not invent a bespoke gateway API. The
upstream vocabulary maps cleanly — `domain_bridge` is literally built from it:

| upstream (rclcpp) | nano-ros today | notes |
|---|---|---|
| `Node::create_generic_publisher(topic, type)` → `GenericPublisher::publish(SerializedMessage)` | `node.create_publisher_raw(topic, type, hash)` → `publish_raw(&[u8])` | already the type-erased / serialized form |
| `Node::create_generic_subscription(topic, type, cb)` | `node.create_subscription_raw(...)` + the executor-registered callback variant | the relay's source |
| subscription callback `void(SerializedMessage, const MessageInfo&)` ([rclcpp](https://github.com/ros2/rclcpp/blob/rolling/rclcpp/src/rclcpp/generic_subscription.cpp)) | **add-on:** raw-sub callback carrying `MessageInfo` | needed for echo metadata |
| one `Node` per domain (domain_bridge) | one node per bridged session via `create_node_on` / `NodeBuilder::session_idx` | **add-on:** multi-session in one binary |

So a nano-ros bridge is the **`domain_bridge` pattern expressed in our
rcl*-mirrored API**: a generic subscription on session A whose callback
re-publishes on a generic publisher bound to session B (and the reverse). The
relay is a *plain subscription callback that publishes* — the same node-centric
shape an application writes — not a special runtime object. Two **add-ons**
sit on top, both idiomatic extensions (each has an upstream analogue):

1. **`MessageInfo` on the raw-sub callback** — mirrors rclcpp's
   `(SerializedMessage, const MessageInfo&)`. nano-ros's `MessageInfo` carries
   the message **attachment**; the bridge stamps a `bridge_origin` on egress and
   the callback drops samples whose origin is its own (echo). The
   attachment + `bridge_origin` codec + FNV dedup already exist in
   `nros-bridge` (`encode_bridge_origin` / `parse_bridge_origin` / `payload_hash`)
   and `nros-node` (`publish_raw_with_attachment` / `try_recv_raw_with_attachment`)
   — the add-on is only to surface them on the *callback* path.
2. **One node per bridged session** — `domain_bridge` makes one node per domain;
   nano-ros runs the sessions in one binary, so each bridge node binds to its
   session via `create_node_on(name, rmw)` / `NodeBuilder::session_idx` (the
   K.5 selector).

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
- **The relay is a generic subscription whose callback publishes** — the
  `domain_bridge` shape in our rcl*-mirrored API. Per (topic, ordered session
  pair): a raw subscription on session A's bridge node whose callback
  re-publishes on a raw publisher bound to session B's bridge node;
  bidirectional = the same the other way. It rides the executor's existing
  callback dispatch in `spin_once` — no separate runtime object, no pump loop.

## The relay + echo (the rcl* shape)

The relay is the standard "subscription callback that publishes" pattern an
application already writes — kept node-centric so it reads like rclcpp/rclrs.
The **only** new executor surface is the echo metadata on the callback:

- **`MessageInfo` on the raw-sub callback** (the add-on, mirrors rclcpp's
  `(SerializedMessage, const MessageInfo&)`). Today
  `register_subscription_buffered_raw_on` hands the callback `FnMut(&[u8])`;
  add a sibling whose callback is `FnMut(&[u8], &nros::MessageInfo)` where
  `MessageInfo` exposes the message **attachment**. The generated relay
  callback then:
  1. `parse_bridge_origin(info.attachment())` — drop the sample if the origin is
     this bridge's own (echo), reusing `nros-bridge`'s codec;
  2. else `dest_pub.publish_raw_with_attachment(payload, encode_bridge_origin(own))`.

  This puts echo handling in the *idiomatic callback*, not a bespoke gateway
  object — `MessageInfo` is exactly how rclcpp surfaces per-message metadata.
  (`nros-bridge::PubSubBridge`'s poll+`pump()` form stays the **standalone /
  C-FFI** path — `nros_pubsub_bridge_*`; the orchestration-generated path uses
  this node-centric callback relay so generated code matches application code.)

*Rejected:* a bespoke executor "bridge registry pumped by `spin_once`" — it
reuses `PubSubBridge`'s dedup but introduces a non-rcl* runtime object and a
second dispatch path. Keeping the relay as a generic-subscription callback (with
`MessageInfo`) stays in the upstream shape, which is the constraint here.

## Generator emission (sketch)

```
SESSION_SPECS  ← bridge.connect endpoints (rmw, locator, domain)   // + existing sources
build_executor_bridge() → open_multi(SESSION_SPECS)                // exists

register_bridges(executor):
  for bridge in PLAN.bridges:
    for topic in bridge.topics:
      (type_name, type_hash, qos) = resolve from plan.interfaces   // err if undeclared
      for (src, dst) in ordered session pairs of bridge.connect:
        // node-centric builders (0022-entity-api-tiers.md), node-ctx used one at a
        // time; the dest publisher is owned and outlives its node-ctx:
        let dst_pub = exec.node_on(dst).publisher(topic)        // NodeCtx dropped here
                          .generic(type_name, type_hash).qos(qos).build()?;
        exec.node_on(src).subscription(topic)                   // re-borrow exec
                .generic(type_name, type_hash).qos(qos)
                .message_info()          // bridge_origin echo check
                .build(move |payload, info| {
                    if parse_bridge_origin(info.attachment()) == Some(ORIGIN) { return; }
                    let _ = dst_pub.publish_raw_with_attachment(payload, &ORIGIN_ATT);
                })?;
register_all(...) { ...; register_bridges(executor)?; }
```

## Work breakdown

1. **nros-node — DONE** (Phase 189.M1). The `MessageInfo` (`&[u8]` payload +
   `attachment()`) axis is a builder knob — generic
   `.message_info()` yields `FnMut(&[u8], &RawMessageInfo)` (the relay's
   callback), not a new `register_*_with_info_on`. `NodeCtx::publisher`/
   `subscription` + `RawMessageInfo` (wire attachment) landed in 189.M1.
2. **generator — DONE.** `register_bridges` (`generate.rs`) emits, per bridge:
   one bridge node per `connect` endpoint (bound via `node_builder().session_idx(idx)`,
   idx matched to the endpoint's `SESSION_SPECS` slot), then per forwarded topic
   per ordered endpoint pair `(i→j)` a generic publisher on `j` + a generic +
   `.message_info()` subscription on `i` whose callback re-publishes through it,
   with `nros-bridge` `bridge_origin` echo suppression. Called from
   `register_all` when bridges exist; `validate_bridges` resolves each topic's
   type from `interfaces` (errors on undeclared / unopened-session / wildcard) +
   the build enables `nros/bridge`.
3. **plan** — DONE (`PlanBridge`).
4. **deploy** — DONE (`apply_bridges`).
5. **check — DONE.** The `[[bridge]]` "routing not yet emitted" warning is gone
   (routing is emitted).
6. **tests — DONE** (compile/shape).
   `generate::register_bridges_emits_relay` (bridge plan → bridge nodes +
   the generic-sub-callback relay + origin codec) +
   `validate_bridges_rejects_undeclared_topic`; the emitted relay was
   compile-verified against `nros` (builder + `nros::bridge` codec +
   `RawMessageInfo` + `publish_raw_with_attachment` + the `bridge` feature).
   Runtime e2e is agent/HW-bound (2 RMW agents) — still gated.

## Deferred

- **Wildcard `"*"`** topic forwarding — needs runtime topic discovery + type
  resolution; out of scope (resolve-from-interfaces requires named, declared topics).
- **QoS reconciliation across mismatched publishers** (domain_bridge's
  majority-match) — nano-ros bakes the declared interface QoS; revisit if needed.
