---
id: 738
title: "`just px4 build-bridge-example` is invoked by no lane, so the uORB->RMW bridge and the C++ px4_msgs emitter are built by nothing"
status: open
type: tech-debt
severity: medium
area: testing, px4, codegen
related: [issue-0362, issue-0360, phase-325]
---

## Finding

Issue 0362's deliverable — a C++ `px4_msgs` emitter and the in-firmware
uORB->RMW bridge that consumes it — is implemented and works. Nothing builds it.

```
$ grep -rn "build-bridge-example" --include=justfile --include='*.just' \
      --include='*.yml' --include='*.sh' . | grep -v third-party
just/px4.just:162:build-bridge-example topics="debug_key_value" edition="jazzy":
```

One hit: the definition. No tier, no CI workflow, no `[[fixture]]` row, no test.

The px4 compile-check set covers only the XRCE companion path, all of it Rust:

```
px4_probe:examples/px4/rust/companion/px4-probe
px4_stub:examples/px4/rust/companion/px4-stub
px4_offboard_companion:examples/px4/rust/companion/offboard-companion
```

`examples/px4/cpp/bridge/` appears in none of them, and `examples/fixtures.toml`
has no px4 rows at all.

## Why it matters more than an unbuilt example

The bridge is the only consumer of `generate-px4-msgs --lang cpp`. So the
emitter's output — headers, the FFI `*_types.rs`/`*_exports.rs` bodies, and the
CMake glue that puts them on a PX4 module's include path — is exercised by
nothing either. A change to the emitter, to `rosidl-bindgen`, or to the C++
header shape can break the whole path and no lane will say so.

Issue 0362 notes the same fragility from the other direction: the emitter's
output is "another per-feature-variant artifact that must stay paired with the
archive it was built against" (issue 0360's flat-path problem). An artifact with
that constraint and no lane is the combination this tree keeps paying for.

## What makes this awkward

A full build is a PX4 firmware build — PX4's own CMake, an external-modules
tree, and the recipe's four stages (generate headers -> FFI staticlib ->
`libnros_cpp.a` carrying `rmw-zenoh-cffi` -> module link). That is heavy for a
per-change tier and is presumably why it was never laned.

But the cheap half is not heavy, and it is where the codegen risk lives:

* run the generator for the bridge's topic set and assert it emitted the four
  files per message;
* compile the generated `.hpp` standalone in one TU — catches a header that does
  not parse, which is most of what an emitter change breaks;
* `cargo check` the FFI crate with `NROS_PX4_BRIDGE_GEN` pointed at that output —
  its `build.rs` already fails loudly when the generated tree is missing.

None of that needs PX4's build system. The full module link can stay on demand,
or nightly, where its cost is affordable.

## Do not fix by making the existing recipe a tier step

`build-bridge-example` hard-errors when the PX4 tree is absent
(`ERROR: PX4-Autopilot tree not found`), which is correct for a recipe someone
typed and wrong for a tier on a host without PX4 provisioned — it would make the
tier unrunnable rather than covered. Whatever lands should either be gated on a
coordinate the lane already knows (the px4 compile-check set is), or FAIL with a
remedy the way `check-zephyr-kconfig-symbols` and `zephyr tier3-cell` do
(issue 0651) — never skip silently, which reports the same colour as coverage.
