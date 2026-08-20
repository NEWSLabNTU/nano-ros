---
id: 738
title: "`just px4 build-bridge-example` is invoked by no lane, so the uORB->RMW bridge and the C++ px4_msgs emitter are built by nothing"
status: resolved
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

## Resolved (2026-08-20)

The cheap half is laned, exactly as scoped above — no PX4 build system involved.

**Build stage:** `compile-check-fixtures.sh` gains the unit `px4_bridge_ffi`,
inside the existing PX4-submodule guard so an absent submodule is a recorded
lane skip rather than a silent one. It runs stages [1/4] and [2/4] of
`build-bridge-example`, plus one thing the recipe never does:

1. `generate-px4-msgs --lang cpp --ros-edition jazzy --topics debug_key_value`
   — the emitter runs, under the SAME advisory lock as the Rust px4 codegen
   (issue 0520's staging clobber applies equally here);
2. each generated `.hpp` compiled STANDALONE, one TU, `-fsyntax-only` — the
   header parses without the bridge's own translation unit around it;
3. `cargo check` on the FFI crate with `NROS_PX4_BRIDGE_GEN` pointed at that
   output, into a DERIVED `--target-dir` (phase-340 P2) rather than the leaf's
   `target/`. The recipe uses the leaf default deliberately — PX4's make is
   handed that archive path — but a compile-check produces nothing anyone reads.

**Test side:** `px4_bridge_compile.rs` asserts the `.compile-ok` stamp via
`require_compile_check`, because the build script exits 0 on a unit failure —
the stamp, not the exit code, is the contract. It states its coordinate
(issue 0700) since these fixtures have no `[[fixture]]` row.

### Verified in both directions

| condition | result |
| --- | --- |
| clean tree | `px4=4/4`, stamp written, test PASSES |
| FFI crate broken (type error added to `lib.rs`) | `cargo-check FAILED for px4_bridge_ffi`, no stamp, `px4=3/4` |
| generated header does not parse | `generated header does not compile: <path>`, no stamp, `px4=3/4` |
| stamp absent | test FAILS naming `just build-test-fixtures` — not a skip |

The third row was not a contrived test. The first real run reported it, because
the header needs `nros/fixed_string.hpp` and `nros/platform.h` and my TU had
neither on its include path. **The header was fine; the check was wrong** — a
useful reminder that a lane's first red is as likely to be the lane. Fixed by
using the include set a PX4 module actually gets, read off `_NROS_PX4_INCLUDES`
in `integrations/px4/NanoRosPx4Module.cmake` — the file whose own comment records
being born with the wrong paths, which is the argument against a second copy.

### Still on demand

Stage [4/4], the PX4 SITL `make` with `EXTERNAL_MODULES_LOCATION`, remains
`just px4 build-bridge-example`. It is the only part that needs PX4's build
system, and the codegen risk this issue is about is not in the link.
