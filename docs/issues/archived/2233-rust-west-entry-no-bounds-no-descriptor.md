---
id: 2233
title: "A generated Rust Zephyr west entry gets no message-bound knobs and no sizing descriptor, and its descriptor default costs a second configure"
status: resolved
type: bug
area: [zephyr, build, sizing]
found: 2026-11-03
related: [issue-1252, issue-1407, issue-1729, issue-1796, phase-439, phase-530, rfc-0120]
---

## Symptom

Measured on `examples/workspaces/rust`, `nros build demo_bringup:zephyr`
(native_sim/native/64), from a clean build dir:

- **one extra configure.** The host configure's first pass says
  `nros: the sizing descriptor this image names to cargo (none was written)
  changed after the readers of this pass had run, so cmake will run once more`,
  and the build prints `Re-running CMake` once.
- **every message-bound knob at its crate default.** `NROS_SUBSCRIBED_TYPE_BOUNDS`,
  `NROS_SUBSCRIBER_BUFFER_SIZE`, `NROS_ZENOH_TL_RETAIN_BYTES`,
  `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS`, `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE` and
  `NROS_SUBSCRIPTION_BUFFER_SIZE` each read `left to its crate default -- no
  value stated and none derivable`, on both passes.
- **no sizing descriptor named to cargo** once the configure converges.

## Cause

A Rust west application (`rust_cargo_application()`) calls neither
`nros_find_interfaces()` nor `nano_ros_entry()`. Those are the two producers
a C/C++ west image's configure has for these answers:

- The message-bound fragment's only producer is `nros_find_interfaces()`.
  Issue 1796 gave every derived knob a road to the Rust lane's cargo env
  (`nros_derived_knob_env`), but for a Rust entry there was nothing derived to
  carry: the module's reader found a placeholder. `nros build` wrote the
  message-bound seed (`message_bound_knobs::write_west_seed`, phase-439 W6)
  only for a C/C++ west image.
- The sizing-descriptor decision (one or none) is made by `nano_ros_entry()`.
  Phase-439 W6 made the module default to "named" for any configure `nros
  build` started, expecting the entry to write the file; a Rust entry never
  does, so the deferred backstop took the decision back and armed a
  re-configure.

## Fix (phase-530 W3, RFC-0120 D6 case 2)

Stage 3.5 of `nros build` / `nros configure` now answers both for a Rust west
image (`resolve_rust_west_image` in `cmd/build.rs`):

- it writes the message-bound seed from the workspace's `generated/` bound
  tables (the ones the image's cargo build compiles), so the module seeds the
  fragment and the derived knobs reach the Rust lane;
- it writes the image's sizing descriptor from its resolve, by the cargo
  road's writer, and records the decision in
  `<resolve>/resolved-sizing.cmake`. The module takes that decision
  (`nros_sizing_descriptor_west_decide`): it copies the descriptor to the fixed
  path cargo is named, and nothing is left for the backstop to take back.

Measured after the fix: 0 `Re-running CMake`; see phase-530 W3 for the knob
comparison.
