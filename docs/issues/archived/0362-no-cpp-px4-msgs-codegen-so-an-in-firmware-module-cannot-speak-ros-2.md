---
id: 362
title: "`generate-px4-msgs` emits a Rust crate only, so an in-firmware C++ PX4 module has no CDR types — the uORB→RMW bridge cannot be written"
status: resolved
type: limitation
severity: medium
area: codegen, px4
related: [issue-0360, phase-325, rfc-0039]
---

## Finding (2026-07-31, phase-325 W3)

phase-325 W3's bridge — uORB inward, a build-time-selected RMW outward, a real
ROS 2 node on the far side — is blocked on message types, not on plumbing. The
plumbing is done and proven:

- one PX4 module links uORB **and** zenoh (`rc=0`, both `register` symbols
  resolved at runtime — phase-325 W3 gate);
- two sessions in one image is a solved shape (`NodeBuilder().rmw(name)`);
- backend selection is the ordinary cargo-feature knob one layer down
  (`--features std,rmw-zenoh-cffi`).

What is missing is that **the bridge has to translate**, and one side of the
translation has no types.

## Scope — CDR is needed for exactly one path (confirmed 2026-08-05)

CDR `px4_msgs` types are needed ONLY where nano-ros speaks the ROS 2 **wire**
protocol from inside PX4 — i.e. an in-firmware C++ uORB→RMW bridge module. Every
other PX4 path is CDR-free, so this is not a general PX4 gap:

| path | serialization | needs C++ CDR types |
| --- | --- | --- |
| nano-ros ↔ PX4 in-firmware (**direct uORB**) | NONE — `publisher_publish_raw` hands the caller's bytes straight to `orb_publish`; the payload IS the PX4 struct (`px4_uorb_interop_e2e.rs:4`) | no |
| nano-ros node → ROS 2 via the XRCE companion (**off-board**) | CDR, but the node is Rust and uses the generated `px4_msgs` **crate** | no (Rust) |
| **in-firmware C++ bridge** → ROS 2 | CDR + RIHS01 type hash | **yes** ← the whole gap |

So the fix is DEMAND-DRIVEN: it should land WITH the phase-325 W3 bridge, not
speculatively. Nothing today needs it (the direct demo is verified by a stock PX4
`listener`).

## The shape of the gap

| side | payload | identity | available in C++? |
| --- | --- | --- | --- |
| inward (uORB) | the PX4 C struct, verbatim | `ORB_ID(x)` | **yes** — `<uORB/topics/*.h>`, PX4's own headers |
| outward (zenoh / xrce / cyclonedds) | CDR | ROS type name **+ type hash** | **no** |

`nros generate-px4-msgs` already produces exactly the right message SET — CDR
`px4_msgs::msg::*` generated from the PX4 `.msg` tree, no ament dependency — and
it is already wired into `just px4 build-fixtures`. But its `--output` is a
**Rust crate**, for the XRCE companion examples (RFC-0039 Track B). An
in-firmware module is C++.

So the bridge can read a `vehicle_status_s` and has nothing to write.

## Why hand-rolling the CDR is not the shortcut it looks like

For a fixed-layout struct the encoding is nearly mechanical — a 4-byte
encapsulation header plus little-endian fields. The blocker is not the bytes, it
is the **type hash**: `rmw_zenoh` keys discovery on it, so a guessed hash gives
one of two outcomes, and the bad one is worse than failure:

- wrong hash → the ROS 2 subscriber never matches, and the bridge looks broken in
  a way that reads like a networking problem;
- wrong hash that happens to match something → a subscriber decodes our bytes as
  a different type.

The hash comes from the same generator that emits the struct. Deriving one
without the other is inventing a second source for a value that already has one.

## What makes this worth doing beyond the bridge

The outward types should be `px4_msgs::msg::*` — precisely what ROS 2 users
already subscribe when PX4 runs `uxrce_dds_client`. That makes the bridge's
ROS-2-facing contract **identical to PX4's own**: a ROS 2 node subscribing
`/fmu/out/vehicle_status` cannot tell whether the samples came from PX4's XRCE
client or from the nano-ros bridge. That indistinguishability is the interop
claim worth proving, and it falls out of reusing the existing message set rather
than inventing a nano-ros-flavoured one.

The translation itself should be cheap: `px4_msgs` is generated from the same
`.msg` files as the uORB structs, so the mapping is field-for-field with no
semantic work — mostly a per-field copy, plus care where PX4's `char[N]` meets
CDR strings.

## Ways to fix

**A. Teach `generate-px4-msgs` a C++ emitter.** `--lang cpp` (or a sibling verb)
writing the same headers the C++ examples get from `nros generate`. Reuses the
`.msg` parsing, the capacity config and the ROS-edition axis that already exist;
the new part is the emitter and the CMake glue to put the output on a PX4
module's include path. Largest piece, and the one that leaves the tree
consistent.

**B. Generate only the topics a bridge names.** Same emitter, but driven from the
bridge's topic list rather than the whole PX4 `.msg` tree — PX4 ships ~200
messages and a bridge carries a handful. Smaller output, faster builds, and it
keeps the module's include surface honest about what it actually speaks.
Probably the right first cut, with A as the general form.

**C. Do the bridge in Rust instead.** PX4 modules can be Rust
(`third-party/px4/px4-rs`), and the Rust `px4_msgs` crate already exists — so
this needs no codegen at all. But it makes the ONE example that must look like a
PX4 module to PX4 people the only Rust one, against the maintainer's direction
that the example follow PX4 convention. Recorded because it is genuinely the
cheapest path, not because it is recommended.

**Recommended: B**, then A when a second consumer appears.

## Blast radius

Nothing today is broken by this: the uORB **direct** demo (phase-325 W2) needs no
CDR at all — that is the entire point of the zero-serialization path, and it
works, verified by a stock `listener`. This blocks only the bridge (W3).

Note the interaction with **issue 0360**: a C++ emitter's output is another
per-feature-variant artifact that must stay paired with the archive it was built
against. Whatever fixes 0360's flat-path problem should cover this too rather
than adding a third thing that silently races for one filename.

## Resolved (2026-08-20) — implemented in `2974adb33`, never closed

Both recommended approaches shipped; this issue simply outlived them. Verified by
RUNNING the tool against the vendored PX4 tree, not by reading the help text.

**Approach A — the C++ emitter.** `nros generate-px4-msgs --lang cpp` emits the
struct plus FFI glue:

```
$ nros generate-px4-msgs --px4 third-party/px4/PX4-Autopilot \
      --lang cpp --ros-edition jazzy --topics vehicle_status -o <dir>
generated px4_msgs C++ (1 messages) at <dir>/px4_msgs
  px4_msgs/msg/vehicle_status.hpp                        # the struct
  px4_msgs/msg/px4_msgs_msg_vehicle_status.hpp           # + TYPE_HASH
  px4_msgs/msg/px4_msgs_msg_vehicle_status_types.rs      # FFI bodies
  px4_msgs/msg/px4_msgs_msg_vehicle_status_exports.rs
```

**Approach B — topic filtering.** `--topics` takes either spelling
(`VehicleStatus` or `vehicle_status`) and pulls nested types in automatically,
so a bridge carrying a handful of topics does not generate PX4's ~200.

**The type hash — the part this issue said was the actual blocker.** Solved the
way it argued it had to be, from the same generator that emits the struct:

```
static constexpr const char* TYPE_HASH =
    "RIHS01_828bddbb7d4c2aa6ad93757955f6893be1ec5d8f11885ec7715bcdd76b5226c9";
```

And on an edition without type hashes it refuses to invent one:

```
warning: --ros-edition humble predates REP-2011, so the emitted TYPE_HASH is a
placeholder. A peer that keys discovery on the type hash (rmw_zenoh) needs
--ros-edition iron|jazzy.
```

That is exactly the failure this issue called worse than failure — "wrong hash
that happens to match something" — declined rather than guessed at.

**The bridge exists.** `examples/px4/cpp/bridge/` (module CMakeLists + an FFI
crate whose `build.rs` globs whatever the generator wrote), driven by
`just px4 build-bridge-example topics=… edition=…`, where TOPICS is the single
source of truth for the message set.

## Scope correction from the maintainer (2026-08-20)

This issue frames the need as CDR-for-the-bridge. In practical use a node
talking to PX4 peers **skips serialization entirely** — raw encoding through
uORB — and what is actually wanted is the message STRUCT in Rust / C / C++.

Against that framing: Rust (`--lang rust`) and C++ (`--lang cpp`) are covered;
`--lang` rejects `c`. That is NOT recorded here as a gap, because for the
raw-uORB path PX4's own `<uORB/topics/*.h>` already provides the C struct
verbatim — this issue's own table says so. A C emitter would matter only to a
nano-ros C node wanting `px4_msgs` types without PX4 headers, and this issue's
own rule applies: demand-driven, land it with the consumer that needs it.

## Not closed by this, and filed separately

`just px4 build-bridge-example` is invoked by **no lane** — not a tier, not a CI
workflow, not a `[[fixture]]` row. The three px4 compile-check fixtures are all
`examples/px4/rust/companion/*`, the XRCE path. So the bridge this issue exists
to unblock is built by nothing, which is how it would rot without anyone
noticing. That is a coverage question rather than a codegen one, so it is its
own issue rather than a reason to keep this one open.
