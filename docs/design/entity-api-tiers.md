# Entity API tiers — convenient (`fork`) + customizable (`clone`)

**Problem.** The executor grew a combinatorial zoo of entity constructors —
`register_subscription`, `register_subscription_raw`,
`register_subscription_buffered_raw_on`,
`register_subscription_raw_with_qos_sized_on`, the proposed
`…_with_info_on`, plus the `create_publisher` / `create_publisher_raw`
pair, `create_subscription_raw_sized`, etc. Every new axis (raw vs typed, QoS,
`rx_buf` size, `_on`-a-node, MessageInfo, session binding) multiplies the
surface. New needs (the bridge wants raw + MessageInfo + session) tempt yet
another `_with_x_y_z` function.

**Goal.** Two tiers, like Linux `fork` vs `clone`:

- **`fork` — convenient, matches upstream ROS.** The 90% case reads exactly like
  rclcpp / rclrs. No knobs to learn.
- **`clone` — one fully-customizable primitive** that *every* convenient
  constructor delegates to, exposing every axis as an optional knob with a
  default. Adding an axis = a builder method, never a new top-level function.

This is also how upstream is shaped: rclcpp has `create_subscription<M>(topic,
qos, cb, options)` + `create_generic_subscription(topic, type, qos, cb,
options)` over one `SubscriptionOptions`; rclrs has `create_subscription` +
fluent QoS-on-topic (`IntoPrimitiveOptions`) + `SubscriptionOptions`. We mirror
that, plus nano-ros-only knobs (raw `rx_buf` size, session binding) on the same
options.

## Tier 1 — convenient (`fork`), mirrors rclcpp / rclrs

```rust
// typed (the common case) — identical shape to rclrs/rclcpp:
let publisher = node.create_publisher::<Int32>("/chatter")?;
let _sub      = node.create_subscription::<Int32>("/chatter", |m: &Int32| { … })?;

// QoS via the fluent topic option (rclrs IntoPrimitiveOptions):
let _sub = node.create_subscription::<Int32>("/chatter".keep_last(10).reliable(), cb)?;

// generic / type-erased (rclcpp create_generic_*), for relays & tools:
let gp = node.create_generic_publisher("/chatter", "std_msgs/msg/Int32", hash)?;
let gs = node.create_generic_subscription("/chatter", ty, hash,
            |bytes: &[u8], info: &MessageInfo| { … })?;
```

These are thin wrappers — each is the builder below with defaults.

## Tier 2 — the `clone`: one builder, every knob

```rust
let sub = node.subscription("/chatter")          // SubscriptionBuilder
    .generic(type_name, type_hash)               // XOR .typed::<Int32>()
    .qos(QosSettings::default().keep_last(10))
    .rx_buffer::<2048>()                          // const-generic staging size
    .message_info()                              // callback gets (&[u8], &MessageInfo)
    .session(slot)                               // bind to an open_multi session (172.K.5)
    .sched_context(sc)                           // scheduling tier
    .build(callback)?;

let publisher = node.publisher("/chatter")
    .typed::<Int32>()
    .qos(q)
    .session(slot)
    .build()?;
```

Mapping the existing zoo onto the builder (so nothing is lost, everything
collapses):

| today's flat fn | builder form |
|---|---|
| `register_subscription::<M>(t, cb)` | `subscription(t).typed::<M>().build(cb)` |
| `…_raw(t, ty, hash, cb)` | `.generic(ty, hash)` |
| `…_raw_sized::<N>` | `.rx_buffer::<N>()` |
| `…_with_qos…` | `.qos(q)` |
| `…_on(node, …)` | `node.subscription(...)` (node-scoped) + `.session(slot)` |
| `…_with_info_on` (proposed) | `.message_info()` |
| `create_publisher` / `…_raw` | `publisher(t).typed::<M>()` / `.generic(ty, hash)` |

## Why a builder (not a big options struct)

A `SubscriptionOptions { … }` struct works in C++/Python (named fields,
defaults). In Rust the **builder** is more ergonomic here because some knobs are
**const-generic** (`rx_buffer::<N>()` sizes the staging array at the type
level — can't be a runtime struct field) and the typed-vs-generic choice changes
the callback's argument type. A builder threads the const param + the
callback-type through `build`. `into`-style fluent topic QoS
(`"t".keep_last(10)`) stays available for the convenient tier.

The C / C++ surfaces (rclc / rclcpp mirrors) keep their named-options structs;
the builder is the Rust ergonomic front, all three lowering to the same core.

## Migration (additive, no break)

1. Land the builder (`node.subscription(t)` / `node.publisher(t)`) over the
   existing core — each `register_*` becomes a one-line delegate.
2. Keep the convenient `create_publisher`/`create_subscription` (+ generic)
   stable — they're the upstream-matching surface; re-point them at the builder.
3. Deprecate the `register_subscription_*_*_*` zoo (keep as thin shims one
   release, then remove) — nothing outside the generator should call them.
4. **The generator emits builder calls**, so generated code reads like
   hand-written application code (the orchestration ⇄ application symmetry the
   bridge design relies on).

## How the bridge uses this

No new flat function. The bridge relay (see
[`bridge-topic-forwarding.md`](bridge-topic-forwarding.md)) is just the
convenient generic subscription with two knobs set:

```rust
node_src.subscription(topic)
    .generic(type_name, type_hash).qos(qos)
    .message_info()                 // for the bridge_origin echo check
    .session(src_slot)              // 172.K.5 selector
    .build(move |payload, info| {
        if parse_bridge_origin(info.attachment()) == Some(ORIGIN) { return; }
        let _ = dst_pub.publish_raw_with_attachment(payload, &ORIGIN_ATT);
    })?;
```

`message_info` + `session` are pre-existing axes finally exposed as knobs rather
than as a new `register_subscription_raw_with_info_on`.
