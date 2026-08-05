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

## Status (2026-08-06)

**Builds, links and registers; the inward half works; the outward half needs the
bridge API this module does not yet use.** See
[issue 0436](../../../../docs/issues/0436-px4-bridge-init-transport-error.md) for
the full diagnosis. In short:

* `nros::init()` + `NodeBuilder::rmw()` — what this module does, following the
  phase-325 W3 note — is the SINGLE-session shape. It opens one session, so the
  outward node has no zenoh session to bind to.
* The supported multi-RMW shape is `nros::MultiExecutor` + `SessionSpec`
  (`nros/bridge.hpp`), which opens both sessions up front. It requires linking
  `libnros_bridge.a`, and `nros_px4_add_module` has no support for that yet —
  the remaining gap.

Two real defects were found and FIXED along the way (both verified): uORB
registered under the deprecated unnamed shim (name `"default"`, so `rmw("uorb")`
and `$NROS_RMW=uorb` could never select it), and `Executor::open_in` reporting
backend-SELECTION outcomes as `Transport(ConnectionFailed)`.

Verified so far, on a real PX4 SITL build:

| | |
| --- | --- |
| PX4 SITL links with the module | yes (1123/1123, `bin/px4`) |
| module registered in the image | yes (`nros_uorb_bridge`) |
| generated CDR symbols linked | yes (`nros_cpp_publish_px4_msgs_msg_debug_key_value`) |
| both backends' register symbols | yes (uORB + zenoh — the W3 gate) |
| module starts | **no** — issue 0436 |

So the codegen half (issue 0362) and the link half (phase-325 W3) are done; what
remains is a runtime session-selection question, not plumbing.

## Build + run

```sh
just px4 build-bridge-example                      # debug_key_value, jazzy
just px4 build-bridge-example topics=vehicle_status  # a different topic
```

Then in the PX4 shell:

```
nros_uorb_bridge start
nros_uorb_bridge status
```

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
