---
id: 413
title: "Rust native cyclone/xrce LISTENER receives nothing in a same-language example pair"
status: open
type: bug
area: rmw
related: [phase-329, issue-0233, issue-0234]
---

## Symptom

A native Rust `examples/native/rust/listener` built for **cyclone** or **xrce**
receives ZERO samples from a native Rust `talker` of the same RMW, while the
**C and C++** same-language pairs deliver normally over the same backends
(run-proven 2026-08-04, phase-329 W4).

Surfaced by the native-example pubsub matrix consumer
(`tests/native_example_pubsub_e2e.rs`): 7 of 9 cells pass; the two failing are
exactly `(Native, Rust, Cyclonedds, Pubsub)` and `(Native, Rust, Xrce, Pubsub)`,
so both are CARVED out of that consumer's filter with a pointer here.

## Evidence

- With the correct runtime env (cyclone: `ROS_DOMAIN_ID` +
  `LD_LIBRARY_PATH=build/install/lib`; xrce: ephemeral Agent + `XRCE_MSG_COUNT` +
  readiness/settle), the C/C++ cyclone and C/C++ xrce pairs deliver ≥2 / ≥1
  `I heard:` lines. The Rust pair on the SAME env delivers 0.
- The Rust talker over cyclone is known-good: `native_api.rs::
  test_native_cyclonedds_rust_talker_to_listener` pairs a **Rust cyclone TALKER**
  with **C/C++ listeners** and they receive. So the Rust cyclone PUBLISH path
  works — the gap is on the Rust cyclone/xrce SUBSCRIBE (listener) side.
- No existing test exercises a Rust cyclone/xrce LISTENER (the matrix declares
  the cells `Runtime`, but the only lanes that ran them were the C/C++-listener
  pairings above), so this path was never actually verified — the cells are
  aspirational-Runtime.

## Likely area

The Rust example listener's subscription setup under the cyclone / xrce backends
(type/keyexpr registration, or the reader never matches the writer). Compare the
Rust listener's `create_subscription` path against the C/C++ listener demos that
DO receive on these backends, and against the Rust cyclone TALKER that DOES
publish. `examples/native/rust/listener/src/main.rs` (String `I heard:`) vs the
C listener `examples/native/c/listener/src/main.c`.

## Fix / direction (not prescribed)

Root-cause the Rust listener subscribe path for cyclone + xrce; once a Rust
same-language pair delivers, drop the carve in
`native_example_pubsub_e2e.rs` (the `!(Rust && (Cyclonedds|Xrce))` filter clause)
so those two cells run in the matrix consumer like the other seven.

## Not doing

Not widening the carve to other coordinates — only these two Rust cells are
affected; the C/C++ cyclone/xrce and all zenoh cells deliver.
