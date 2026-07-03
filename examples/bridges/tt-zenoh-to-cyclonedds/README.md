# Time-Triggered Bridge: Zenoh → Cyclone DDS (issue #53)

A single Rust process hosting **two RMW backends**: a Zenoh raw subscription
(ingress) and a Cyclone DDS raw publisher (egress), forwarding
`std_msgs/msg/String` on `/chatter` under an ARINC-653-style cyclic executive.

## What it shows

- 10 ms major frame; ingress window [0, 3) ms, egress window [5, 8) ms
  (`TimeTriggeredSchedule::<2>` / `apply_time_triggered_schedule`).
- The one structural difference from the XRCE sibling
  ([`tt-zenoh-to-xrce`](../tt-zenoh-to-xrce/)): Cyclone needs a registered
  `dds_topic_descriptor_t`, so the type schema (`{ data: string }`) is staged
  via `nros_rmw::register_type_descriptor(...)` **before** the raw publisher
  is created.
- The bridge auto-stops after 60 s.

## Build

```sh
cargo build -p native-rs-bridge-tt-zenoh-to-cyclonedds
```

(Own Cargo workspace — build from this directory or with `--manifest-path`.)

## Run

```sh
zenohd --listen tcp/127.0.0.1:7447 &          # ingress side (ZENOH_LOCATOR overrides)
cargo run -p native-rs-bridge-tt-zenoh-to-cyclonedds   # ROS_DOMAIN_ID = egress domain
# feed it: a zenoh /chatter talker (e.g. examples/native/rust/talker)
# observe: any Cyclone DDS /chatter subscriber, e.g.
ros2 topic echo /chatter std_msgs/msg/String   # rmw_cyclonedds_cpp, same domain
```

## Expected output (log)

```
=== Issue #53: zenoh → Cyclone DDS under TT schedule ===
TT schedule applied: major_frame=10000us, ingress=[0, 3000)us, egress=[5000, 8000)us
[ingress] captured 24 bytes
[egress] forwarded 24 bytes
Bridge stopped after 60 s.
```

## Files

- `src/main.rs` — schedule, ingress/egress tasks, descriptor staging.
- `Cargo.toml` — nros + nros-rmw-zenoh + nros-rmw-cyclonedds-sys.
