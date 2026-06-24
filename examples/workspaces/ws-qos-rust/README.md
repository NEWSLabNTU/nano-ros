# ws-qos-rust — QoS-override showcase (phase-263 B4)

A minimal product-shaped nano-ros workspace demonstrating **explicit QoS
selection** end-to-end, the one differentiator the starter workspace does not
show.

## Topology

| Node | Role | QoS |
| --- | --- | --- |
| `reliable_talker` | publishes `std_msgs/Int32` on `/qos_chatter` @1 Hz | RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10) |
| `qos_listener` | subscribes `/qos_chatter`, republishes the receive count on `/qos_ok` | same profile (must match to connect) |

QoS is declared **per entity in code** via the declarative `*_with_qos` API:

```rust
let qos = QosSettings::default().reliable().transient_local().depth(10);
node.create_publisher_for_topic_with_qos::<Int32>("/qos_chatter", qos)?;
node.create_subscription_for_topic_with_qos::<Int32>("/qos_chatter", qos)?;
```

The shared contract lives in `reliable_talker_pkg::qos_profile()` so both
endpoints declare an identical profile. TRANSIENT_LOCAL durability is the
visible behaviour: a late-joining subscriber with the matching profile still
receives the publisher's buffered history.

## Build & run

```sh
nros ws sync                       # generate the std_msgs crate (gitignored)
cargo build -p native_entry        # links both Node pkgs
nros plan demo_bringup             # inspect the resolved plan
```

## Notes

- QoS here is a **code-level contract** — there is no `system.toml` QoS section;
  the planner's baked `qos_overrides` table (config-driven QoS, `apply_overrides`)
  is a separate, more advanced surface.
- Status events (deadline-missed / liveliness) are a further QoS surface not yet
  exposed on the declarative `CallbackCtx` — a remaining item.
- Same-process delivery does not occur (issue 0096); a cross-process subscriber
  observes `/qos_chatter` / `/qos_ok` (the Track-D runtime assertion).
