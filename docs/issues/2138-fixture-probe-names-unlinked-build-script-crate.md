---
id: 2138
title: "The fixture staleness probe reads a C image as STALE after an edit to `nros-zpico-build`, a build-script crate whose change relinks nothing"
status: open
type: bug
area: [testing, build]
severity: low
found: 2026-10-29
related: [0445, 1710, 1954]
---

## Measured

On 2026-10-29, after a fresh `just build-test-fixtures lane=linux` (exit 0), two
interop cells read STALE and did not run:

- `rosout_interop::a_c_log_call_reaches_ros2_topic_echo_rosout` (fixture
  `rosout-talker-c`).
- `qos_event_interop::c_matched_and_liveliness_callbacks_follow_stock_ros2_peers`
  (fixture `matched-events-probe-c`).

Both verdicts name the same input:

```
binary: .../bins/rosout-talker-c/build-zenoh/rosout_talker_c
newer:  .../packages/rmw/zenoh/nros-zpico-build/src/lib.rs
probe:  examined 46 input(s); ... 1552 cargo input(s) answered by a byte-identical relink ...
NOT RUN: 9th consecutive stale verdict for this fixture, first 2d ago.
```

- The binary's mtime is 02:30 local. `lib.rs` is 04:53, from the checkout of
  `0afa78b7bc` (#2094), which changes only NuttX's socket timeout.
- `ninja -C .../build-zenoh` then ran and exited 0. It did not relink the
  binary, whose mtime stayed 02:30.
- A re-run read STALE again.
- Run with `NROS_SKIP_FIXTURE_CHECK=1`, both cells PASSED: against Humble's
  0.1.9 router, and against a 0.1.11 overlay.

So no build can clear this verdict, and the cell has reported no result for two
days.

## Reading

`nros-zpico-build` is a `[build-dependencies]` crate of `zpico-sys`. An edit to
it re-runs `zpico-sys`'s build script. On Linux that edit changes nothing the
script emits, so the archive comes out byte-identical and nothing downstream
relinks. That is correct build behaviour.

Issue 1710's link cover (`binaries::cargo_unit_link_cover`) answers cargo
inputs that a byte-identical relink covered, and it answered 1552 of them here.
It did not answer this one. Our guess is that the cover attributes inputs
through the archive's own dep-info, which does not list a build-dependency's
sources. The exact path was NOT traced.

## What it is NOT

- Not a stale binary. Building it again changes nothing, and the cells pass.
- Not issue 1954 (archived), which was a probe reading a sibling image's crate.

## What would close it

- The probe answers a build-dependency's source the same way it answers a
  linked crate's, when cargo re-ran the build script after the edit and the
  archive came out byte-identical.
- Or the probe drops build-dependency sources from a C image's input set, and
  its `probe:` line says it did.
- **Acceptance:** after a `nros-zpico-build` edit that changes nothing on Linux,
  and a `build-test-fixtures lane=linux`, both cells run.
