---
id: 304
title: "A declarative node's per-entity QoS was DROPPED by the component runtime — `create_publisher_for_topic_with_qos` had no effect, and `ws-qos-rust` demonstrates a feature that did not work"
status: resolved
type: bug
severity: high
area: runtime, executor
related: [issue-0303, issue-0096]
---

## Finding (2026-07-28, while designing the QoS-override e2e)

`EntityMetadata` carries a `qos: QosSettings` field, populated by the
declarative API when a node declares
`create_publisher_for_topic_with_qos(...)` /
`create_subscription_for_topic_with_qos(...)`. `ExecutorSink::create_entity`
(`packages/core/nros/src/node_runtime.rs`) — the runtime every `nros::main!`
component goes through — **never read it**:

```rust
EntityKind::Publisher => {
    self.executor.node_mut(node)
        .create_generic_publisher(entity_name, metadata.type_name, metadata.type_hash)
    //  ^ no qos argument; the callee passes QosSettings::default()
```

`grep 'metadata.qos' node_runtime.rs` returned nothing. So every publisher and
subscription created by a declarative node ran with
`QosSettings::default()` — reliable, volatile, keep-last — no matter what the
node declared.

## Why it survived a dedicated example and its e2e

`examples/workspaces/ws-qos-rust` exists to demonstrate exactly this feature.
Its `reliable_talker_pkg` doc comment says:

> TRANSIENT_LOCAL durability is the visible behaviour: a late-joining
> subscriber with matching QoS still receives the last 10 samples published
> before it joined.

That behaviour was not happening. The workspace's e2e coverage
(`workspace_features_e2e.rs`, `Proof::QosMatchedCount`) asserts only that the
listener receives a **count** of messages — and default-QoS publisher to
default-QoS subscriber delivers perfectly well, so the test passed while the
feature it named did nothing. No test in the tree observes QoS *semantics*
(history replay, depth-driven drops); that absence is what let this sit.

The C and C++ projections of the same demo go through `nros-cpp` /
`nros-c`, which pass their QoS struct by value — they were unaffected. So the
Rust path alone was silently degraded, from workspaces whose whole point was
the QoS profile.

## Impact

- Any declarative Rust node's per-entity QoS was ignored: reliability,
  durability, history and depth all fell back to defaults.
- Cross-vendor interop QoS agreements (a nano-ros node matching an
  `rmw_zenoh_cpp` peer's non-default profile) could not be expressed from a
  declarative node at all.
- Plan-level `qos_overrides` (issue #52 / 0303) DID apply, because they fold
  inside the executor's create path — so the model could set QoS the code
  could not. That inversion is the tell that these two layers were never
  tested together.

## Fix (2026-07-28)

`NodeCtx` gains `create_generic_publisher_with_qos` /
`create_generic_subscription_with_qos`; the default-QoS constructors are now
thin wrappers over them. `ExecutorSink::create_entity` passes `metadata.qos`
for both entity kinds.

Ordering is unchanged and correct: the node's declared profile is the INPUT to
the executor's create path, and plan `qos_overrides` fold on top of it there —
so a plan override still wins over a code-declared profile, matching rclcpp
(plan = authority) and issue #52's stated contract.

## Follow-up

Covered by the QoS e2e added with this fix. Note the broader gap it exposed:
`Proof::QosMatchedCount` proves *delivery*, not *profile*. A test that asserts
a count cannot distinguish "the QoS I asked for" from "any compatible QoS" —
which is how a whole feature stayed dark behind a green suite.
