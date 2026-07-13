---
id: 183
title: "declarative ws-bridge lanes deliver 0 samples (zenoh→cyclonedds nano listener + nested-header, zenoh→xrce)"
status: open
type: bug
area: testing
related: [phase-287, issue-0164, issue-0193]
---

## Summary

Deterministic (serialized rerun, fresh fixtures 2026-07-12):

- `declarative_bridge_zenoh_to_cyclonedds::declarative_zenoh_to_cyclonedds_bridge_to_nano_listener`:
  `expected ≥ 2 bridged samples to reach the nano cyclone listener (zenoh →
  declarative ws-bridge-rust entry → cyclonedds), got 0. Full listener
  output:` (EMPTY — the listener printed nothing at all).
- `…nested_header_to_ros2` — same lane, same shape.
- `declarative_bridge_zenoh_to_xrce::declarative_zenoh_to_xrce_bridge_to_nros_listener`.

The imperative `bridge_zenoh_to_cyclonedds::test_zenoh_to_cyclonedds_bridge_ros2`
and `demo_nodes_cpp_interop` failures from the parallel sweep PASSED
serialized (storm flakes) — only the declarative ws-bridge entries stay red.

## Notes

Empty listener output = the bridged-side listener process produced no stdout
at all → likely the ws-bridge-rust entry (or the listener fixture) never came
up rather than a forwarding bug. The ws-bridge workspace fixtures went
through the same fresh-sweep rebuild; check their entry build + the
`nros plan` wiring before suspecting the bridge runtime. Untriaged beyond
this; needs its own session.

## ROOT CAUSE — message-type mismatch (NOT a bridge/forwarding bug) — 2026-07-13

The "never came up" reading was wrong. The bridge RUNTIME works: a manual repro
with the prebuilt binaries (zenohd → `native_entry` bridge → cyclonedds → nano C
listener) delivered **11 `Received:` samples** end-to-end. The failure is a
**message-type mismatch in the test's fixture pairing:**

- The ws-bridge demo is intrinsically **`std_msgs/Int32`**: its own
  `talker_pkg` publishes Int32 on `/chatter`, and the generated
  `demo_bringup/nros-bridge.toml` forwards it typed as
  `std_msgs::msg::dds_::Int32_` (`fields = [{name=data, type=int32}]`). So the
  bridge stages an **Int32** Cyclone descriptor and registers `/chatter` as an
  Int32-typed topic on the cyclone egress.
- But the test drives it with the **SHARED** `talker_binary` (native rust
  talker) and observes with the **SHARED** nano `c/listener` — both of which
  commit `8f9433782` (277-W4.b, "native chatter examples match official ROS 2
  demos") migrated to **`std_msgs/String`** ("Hello World: N" / `I heard: [%s]`).
- Result: the bridge's Int32-typed cyclone topic never matches the String
  subscriber (DDS type mismatch) → 0 delivery. The `wait_for_output_count(
  LISTENER_LOG_PREFIX, 2, 12 s)` then times out and returns `Err`, and the
  test's `.unwrap_or_default()` turns that into the **empty** `listener_output`
  the report saw — not a crashed listener.

The imperative sibling's `..._bridge_to_nano_listener` shares the same shared
String talker + String C listener + `LISTENER_LOG_PREFIX` grep, so it has the
identical latent mismatch (its `..._bridge_ros2` variant passes because it uses a
`ros2 topic echo` receiver + greps `data:`, not the nano listener).

## Fix plan (needs `just cyclonedds setup` to rebuild + verify)

Re-align the test to the Int32 bridge (keep the demo's deliberate Int32 cross-RMW
showcase). Two workable shapes:

1. **Type-select the shared fixtures by env** (mirrors the #164 native-rust
   listener `NROS_SUB_TYPE` fix): add `NROS_PUB_TYPE=int32` to the native rust
   talker and `NROS_SUB_TYPE=int32` (Int32 deserialize + `Received:` print) to the
   nano `c/listener`; the bridge tests set both and grep `INT32_LISTENER_LOG_PREFIX`
   (`Received:`). Rebuild the cyclone C listener (needs cyclonedds).
2. **Migrate the ws-bridge demo to String** to match 277-W4.b's direction
   (`talker_pkg`/`listener_pkg`/`system.toml`/`nros-bridge.toml` → String), so the
   shared String talker/listener pair cleanly. Simpler test, but drops the Int32
   showcase and needs the `native_entry` rebuilt (needs cyclonedds).

Blocked here only on the cyclonedds submodule being absent in this tree (the
prebuilt binaries can't be regenerated for the String→Int32 alignment). Zero
runtime/bridge code change is needed — it is purely fixture type-alignment.

## FIX IMPLEMENTED (Option 1 — type-match to Int32) — 2026-07-13

Provisioned cyclonedds (`just cyclonedds setup`) and landed the type-alignment:

- **native rust talker** (`examples/native/rust/talker`): `NROS_PUB_TYPE=int32`
  now publishes `std_msgs/Int32` (default stays `String`). Rebuilt + verified —
  it logs `Publishing: 9/10/11` (numeric Int32).
- **nano C listener** (`examples/native/c/listener`): `NROS_SUB_TYPE=int32`
  subscribes Int32 (Int32 deserialize + `Received:` print; default stays String).
- **declarative bridge test**: sets `NROS_PUB_TYPE=int32` on the talker +
  `NROS_SUB_TYPE=int32` on the listener and greps `INT32_LISTENER_LOG_PREFIX`.

**The Int32 pipeline itself is proven end-to-end**: a manual repro with the
pre-migration (Int32) prebuilt binaries — zenohd → `native_entry` bridge →
cyclonedds → nano listener — delivered **11 `Received:` samples**. The talker side
of the fix is verified (publishes Int32).

**Remaining verification blocker (separate, not #183):** a FRESH cyclone C
listener build (via a targeted `fixture-make-driver.sh native-cyclonedds-cmake`)
fails `nros_executor_register_subscription -> -1` at startup — for the **String
default too**, so it is NOT the Int32 change; it is a fresh-cyclone-build /
descriptor-registration regression (or a gap in the targeted build invocation vs
the full `just native build-fixture-extras`). The old prebuilt listener registered
fine. Final e2e-on-current-fixtures verification is owed once a clean full
`build-fixture-extras` produces a listener that registers; the type-alignment fix
itself is correct by construction. **This register-1 blocker is now tracked
separately as #193** (it hits the String default too, so it is not the #183 change).

**Imperative sibling needs NO change (corrected):** `bridge_zenoh_to_cyclonedds`'s
hand-written `bridge-zenoh-to-cyclonedds-fwd` bin was already migrated to
`std_msgs/String` by phase-277 (`TYPE_NAME = std_msgs::msg::dds_::String_`), so its
String talker + String listener + String fwd bin + `LISTENER_LOG_PREFIX` are
internally consistent — no mismatch. Only the DECLARATIVE ws-bridge demo stayed
Int32 (its `talker_pkg` + generated `nros-bridge.toml`), which is why only its
lane broke. This fix is declarative-only.
