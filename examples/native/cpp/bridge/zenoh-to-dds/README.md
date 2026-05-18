# Zenoh → DDS bridge (Phase 104.D.2, C++)

C++ audience companion to
`examples/bridges/native-rust-zenoh-to-dds/` (104.C.10) and to
the C-side `examples/native/c/bridge/xrce-to-dds/` (104.D.1).
Forwards raw CDR bytes from a Zenoh subscription to a dust-DDS
publisher on the same topic name, demonstrating the
rclcpp-aligned multi-Node + multi-RMW shape (Phase 104.C.3 +
104.C.9) at the nros-cpp surface:

* Two RMW backends (zenoh-pico + dust-DDS) linked into one
  binary, each self-registering under its canonical name via
  `.init_array` ctor (Phase 104.A.1+2).
* One `nros::Executor` holds two `nros::Node`s built via the
  104.C.9 `NodeBuilder` chain
  (`executor.node_builder("ingress").rmw("zenoh").build(node_in)`).
* `nros::Subscription<M>` is poll-based — the bridge calls
  `try_recv_raw` each `spin_once` tick + re-publishes verbatim
  via `publish_raw`. No deserialise / reserialise round-trip.

## Build

```bash
cmake -B build -S .
cmake --build build
```

The example pulls **two** RMW staticlibs:

* `NANO_ROS_RMW=zenoh` (the cache var set at the top) drives
  the root `add_subdirectory()` dispatch and pulls
  `nros-rmw-zenoh-staticlib` into `NanoRos::NanoRosCpp` via
  whole-archive wrap.
* A separate `corrosion_import_crate(... nros-rmw-dds-staticlib ...)`
  + matching whole-archive `target_link_libraries` pulls
  dust-DDS into the same binary. The root CMake's
  `NANO_ROS_RMW` dispatch handles single-backend builds only
  today; multi-backend bridges extend it per-target. The
  generalised helper (`nano_ros_link_extra_rmw(target NAME <rmw>)`)
  is tracked under Phase 104.B follow-up — see the C bridge
  (104.D.1) README for the same workaround.

## Run

```bash
# Terminal 1 — Zenoh router (matches the locator the bridge uses).
zenohd --listen tcp/127.0.0.1:7447

# Terminal 2 — your DDS publisher / subscriber. dust-DDS uses
# the standard DDS discovery domain.

# Terminal 3 — the bridge itself.
./build/zenoh_to_dds_cpp_bridge
```

Environment overrides:

| Var | Default | Purpose |
|---|---|---|
| `NROS_ZENOH_LOCATOR` | `tcp/127.0.0.1:7447` | Zenoh router socket. |
| `ROS_DOMAIN_ID`      | `0` | ROS domain ID for both sides. |

Topic flow:

```
Zenoh "/chatter" ── ingress sub (raw poll) ──┐
                                              ├─ bridge ─ publish_raw ──> DDS "/chatter"
```

Type: `std_msgs/String` (hand-rolled `ChatterString` stub with
`TYPE_NAME` + `TYPE_HASH` + `SERIALIZED_SIZE_MAX` — the
templates require those static constants; bridge forwards
bytes verbatim either way, so no codegen dep is needed).

## Cyclone DDS variant (alternative)

The 104.D.2 spec originally suggested
`examples/native/cpp/bridge/zenoh-to-cyclonedds/`. Cyclone
DDS needs a one-time `just cyclonedds setup` to build the
upstream C++ library + headers, which isn't on the default
`just setup` path. Swap-in for Cyclone:

1. Bridge code: `.rmw("dds")` → `.rmw("cyclonedds")` on the
   egress node.
2. CMakeLists: replace the
   `corrosion_import_crate(... nros-rmw-dds-staticlib ...)`
   block with
   `add_subdirectory(${repo_root}/packages/dds/nros-rmw-cyclonedds)`
   and switch the second whole-archive entry from
   `nros_rmw_dds_staticlib-static` to `nros_rmw_cyclonedds`
   (matches root CMake's branch at `CMakeLists.txt:219-242`).
3. Run `just cyclonedds setup` first to populate the Cyclone
   DDS install prefix that `add_subdirectory` reads.

## Notes

* `nros::Executor::create(zenoh_locator, domain_id)` pins the
  primary session to Zenoh. The egress node's `.rmw("dds")`
  triggers the 104.C.3 extra-session open path; both sessions
  drive each `spin_once`.
* `nros::Subscription<M>` is poll-only by design — the cb
  dispatch the C side uses (`nros_subscription_init` +
  `nros_executor_register_subscription`) doesn't have a C++
  equivalent yet. Phase 104.C lists a "lambda subscription"
  goal in the spec; today the poll shape achieves the same
  end-to-end behaviour with one extra check per tick.
* Per Phase 104.D.5 (decoupling CI guard), the bridge binary
  contains zero direct deps on either RMW backend's Rust
  crate — both are pulled at link time by the CMake
  staticlib imports.
