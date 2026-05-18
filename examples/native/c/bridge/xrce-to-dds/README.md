# XRCE → DDS bridge (Phase 104.D.1)

C audience companion to
`examples/bridges/native-rust-zenoh-to-dds/` (Phase 104.C.10).
Forwards raw CDR bytes from an XRCE-DDS subscription to a
dust-DDS publisher on the same topic name, demonstrating the
rclcpp-aligned multi-Node + multi-RMW shape (Phase 104.C.3 +
104.C.8) at the nros-c surface:

* Two RMW backends (XRCE + dust-DDS) linked into one binary,
  each self-registering under its canonical name via
  `.init_array` ctor (Phase 104.A.1+2).
* One `nros_executor_t` holds two `nros_node_t`s. The
  ingress node binds to XRCE via
  `nros_node_options_t.rmw_name = "xrce"`; the egress node
  binds to DDS via `rmw_name = "dds"`.
* The Executor opens a second session under the hood
  (Phase 104.C.3) and drives both via `spin_once`.

## Build

```bash
cmake -B build -S .
cmake --build build
```

The example's CMakeLists pulls **two** RMW staticlibs:

* `NANO_ROS_RMW=xrce` (the cache var set at the top) drives the
  root `add_subdirectory()` dispatch and pulls
  `nros-rmw-xrce-cffi` into `NanoRos::NanoRos` via
  whole-archive wrap.
* A separate `corrosion_import_crate(... nros-rmw-dds-staticlib ...)`
  + matching whole-archive `target_link_libraries` pulls
  dust-DDS into the same binary. The root CMake's
  `NANO_ROS_RMW` dispatch handles single-backend builds only
  today (Phase 137.3 inline branch); multi-backend bridges
  extend it per-target. Generalising this into a
  `nano_ros_link_extra_rmw(target NAME <rmw>)` helper is
  tracked under Phase 104.B follow-up.

## Run

```bash
# Terminal 1 — XRCE agent (matches the locator the bridge uses).
MicroXRCEAgent udp4 -p 8888

# Terminal 2 — your DDS publisher / subscriber. dust-DDS uses
# the standard DDS discovery domain.

# Terminal 3 — the bridge itself.
./build/xrce_to_dds_bridge
```

Environment overrides:

| Var | Default | Purpose |
|---|---|---|
| `NROS_XRCE_LOCATOR` | `udp/127.0.0.1:8888` | XRCE agent socket. |
| `NROS_DDS_LOCATOR`  | `` (backend default) | DDS discovery transport override. Empty = `dust-dds` picks its own UDPv4 multicast. |
| `ROS_DOMAIN_ID`     | `0` | ROS domain ID for both sides. |

Topic flow:

```
XRCE "/chatter" ── ingress sub (raw) ──┐
                                        ├─ bridge ─ publish_raw ──> DDS "/chatter"
```

Type: `std_msgs/String` (inline `nros_message_type_t` — the
bridge forwards verbatim CDR bytes either way, so no codegen
dependency is needed).

## Notes

* `nros_subscription_init` + `nros_publisher_init` both flow
  through the per-Node session (Phase 104.C.8 dispatch) when
  `node_in.node_id` / `node_out.node_id` is non-zero. The
  `nros_executor_node_init` path sets that field
  automatically from `nros_node_options_t.rmw_name`.
* The egress publisher's `nros_publish_raw` accepts the same
  CDR byte buffer that arrived on the ingress subscription
  callback. No deserialise / reserialise round-trip — the
  bridge is byte-for-byte verbatim.
* Per Phase 104.D.5 (decoupling CI guard), the bridge binary
  contains zero direct deps on either RMW backend's Rust
  crate — both are pulled at link time by the CMake
  staticlib imports.
