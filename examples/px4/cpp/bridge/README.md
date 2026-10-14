# nano-ros uORB → RMW bridge (PX4 in-firmware module)

Reads a uORB topic inside PX4 and re-publishes it as CDR `px4_msgs` on a
networked backend, so an ordinary ROS 2 subscriber sees it.

```
PX4 publisher ──uORB──▶ nros_uorb_bridge ──zenoh/CDR──▶ ROS 2 subscriber
                (raw struct,            (translate +
                 no decode)              type hash)
```

## What this is NOT

The sibling `examples/px4/cpp/firmware` (phase-325 W2) demonstrates the
**zero-copy** property: nano-ros publishes a PX4 struct into uORB with no
serialization, and a stock PX4 `listener` reads it.

This example demonstrates **reach**, and the serialization uORB avoids comes back
here, at the RMW boundary — necessarily. Zero-copy is not claimed for the outward
half. Conflating the two would overclaim (phase-325 W4.3).

## Status (2026-08-06): WORKING

Verified end to end on PX4 SITL — the module reads a uORB topic in-firmware and
re-publishes it as CDR `px4_msgs` on zenoh:

```
INFO  [nros_uorb_bridge] bridging /fmu/out/debug_key_value (uorb) -> /fmu/out/debug_key_value (zenoh)
INFO  [nros_uorb_bridge] forwarded 100 samples (key=velx value=99.0)
```

(`px4_mavlink_debug start` supplies the uORB samples; nothing publishes
`debug_key_value` by default.)

Getting here fixed five real defects — see
[issue 0436](../../../../docs/issues/archived/0436-px4-bridge-init-transport-error.md):

1. **Two copies of zenoh-pico** in one image (umbrella core + a complete second
   from the platform archive), each with its own statics — `z_open` failed having
   put nothing on the wire. zenoh-pico now has ONE source: the umbrella built with
   a platform feature.
2. **uORB registered under the deprecated unnamed shim** (`"default"`), so
   `rmw("uorb")` / `$NROS_RMW=uorb` could never select it.
3. **`open_multi`'s extra sessions were anonymous**, so the first Node naming a
   backend opened a SECOND session against it.
4. **Two incompatible executor handles behind one `void*`** — `MultiExecutor`'s
   `ExecutorBox` vs the C++ Node path's `CppContext`. Mixing them corrupted memory
   (PX4 dumped core). `nros_cpp_init_multi` now opens multi-RMW sessions into a real
   `CppContext`, so every existing C++ Node/publisher path works unchanged.
5. **Collapsed errors** at four seams reported selection/registration problems as
   transport failures; each now names its cause.

## Build + run

```sh
just px4 build-bridge-example                       # debug_key_value, jazzy, zenoh
just px4 build-bridge-example vehicle_status        # a different topic
just px4 build-bridge-example debug_key_value humble  # a different ROS edition

# phase-325 W3.3 — the OUTWARD backend. An env var, not an argument (below).
NROS_PX4_BRIDGE_RMW=xrce just px4 build-bridge-example
```

**The arguments are POSITIONAL.** `just` has no named-argument syntax, so the
`topics=vehicle_status` this block used to show bound the literal string
`"topics=vehicle_status"` to the first parameter and the build died with

```
Error: px4 message `topics=vehicle_status` not found under .../msg
```

naming the message rather than the mistake. Measured 2026-09-04. The outward
backend is an env var for the same reason — it has to reach the cargo feature,
the `BACKENDS` list and the session-spec name, and a fourth positional that
silently swallows a typo is worse than an exported value.

Then in the PX4 shell:

```
nros_uorb_bridge start
nros_uorb_bridge status
```

One thing to know, measured:

* **Build for your subscriber's edition.** The `edition` argument (default
  `jazzy`) decides the data key: a `humble` image is seen only by a Humble
  subscriber and a `jazzy` image only by an Iron+ one. Until 2026-10-15 every
  image published Humble keys whatever you asked for
  ([issue 1893](../../../../docs/issues/archived/1893-px4-bridge-edition-misses-zenoh-keyexpr.md)).

`NROS_PX4_BRIDGE_RMW=xrce` runs beside PX4's own `uxrce_dds_client` in one
`px4_sitl_default` image: nano-ros's copy of Micro-XRCE-DDS-Client compiles under
private `nros_` symbol names, so the two clients no longer bind to each other
([issue 1896](../../../../docs/issues/archived/1896-px4-bridge-xrce-collides-with-uxrce-dds-client.md)).
Point the bridge at the same agent PX4 dials, e.g.
`NROS_LOCATOR=udp/127.0.0.1:8888`; `just px4 test-bridge-xrce-ros2` runs both
into a ROS 2 subscriber.

## Test: a real ROS 2 subscriber (phase-325 W3.4)

```sh
just px4 test-bridge-ros2
```

builds the `humble` zenoh image, builds the `px4_msgs` ament overlay
(`just px4 build-px4-msgs-ros humble` — no distro ships px4_msgs, so it is
built from this PX4 tree's own `.msg` files into
`build/px4-msgs-ros/humble/install`), then runs
`packages/testing/nros-px4-sitl-test/tests/px4_bridge_ros2_e2e.rs`: PX4's stock
`px4_mavlink_debug` publishes `debug_key_value` on uORB, the bridge forwards it,
and `ros2 topic echo /fmu/out/debug_key_value px4_msgs/msg/DebugKeyValue` over
`rmw_zenoh_cpp` must decode `key: velx` with an advancing `value`. To watch it by
hand after the two builds:

```sh
source /opt/ros/humble/setup.bash
ros2 run rmw_zenoh_cpp rmw_zenohd &            # the router; the bridge dials tcp/127.0.0.1:7447
source build/px4-msgs-ros/humble/install/share/px4_msgs/local_setup.bash
RMW_IMPLEMENTATION=rmw_zenoh_cpp ros2 topic echo /fmu/out/debug_key_value px4_msgs/msg/DebugKeyValue
```

(`px4_mavlink_debug` runs its 100-sample loop in the foreground of the shell
command, so start it after the bridge and the subscriber.)

## Why there are four build steps

Publishing on a wire (rather than into uORB) pulls in three things the W2 demo
needs none of:

1. **Generated CDR types.** `nros generate-px4-msgs --lang cpp --topics …`
   (issue 0362). `rmw_zenoh` keys discovery on the **RIHS01 type hash**, so a
   hand-rolled payload with a guessed hash is either invisible to ROS 2 or —
   worse — visible and decoded as the wrong type. The hash therefore comes from
   the same generator that emits the struct, and is byte-identical to the one the
   Rust `px4_msgs` crate carries.
2. **An FFI staticlib.** A generated C++ message header declares its
   serialize/deserialize as `extern "C"`; the bodies are Rust. A normal CMake
   consumer gets that crate synthesized by `nros_generate_interfaces(LANGUAGE
   CPP)` — a PX4 module builds under PX4's own cmake and never runs it, so `ffi/`
   carries it. Its `build.rs` globs whatever the generator wrote, so the topic
   list is stated once (in the recipe), not twice.
3. **The outward backend baked into the archive.** Backend selection happens when
   `libnros_cpp.a` is built (`--features rmw-zenoh-cffi`), not in cmake; zenoh
   additionally needs zenoh-pico's platform layer from
   `nros-rmw-zenoh-staticlib`.

## Why the translation is field-by-field and not a `memcpy`

Both structs come from the same `.msg`, so their field *names* match — but the
layouts do not. PX4 reorders for packing and appends explicit padding:

| | layout |
| --- | --- |
| PX4 `debug_key_value_s` | `{ uint64 timestamp; float value; char key[10]; uint8 _padding0[2]; }` |
| ROS `px4_msgs::msg::DebugKeyValue` | `{ uint64 timestamp; char key[10]; float value; }` |

A `memcpy` would silently transpose `key` and `value`. `translate()` in
`NrosUorbBridge.cpp` writes the map out, and copies `char[N]` with its NUL
terminator intact because the CDR side reads it as a string.

## Two sessions, both named

The inward node binds `"uorb"`, the outward node binds the networked backend, via
`NodeBuilder::rmw(name)` on one executor. An empty name takes the
first-registered backend, which is `BACKENDS` argument order — an argument list,
not a contract — so both are named explicitly.
