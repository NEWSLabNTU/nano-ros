---
id: 362
title: "`generate-px4-msgs` emits a Rust crate only, so an in-firmware C++ PX4 module has no CDR types — the uORB→RMW bridge cannot be written"
status: open
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
