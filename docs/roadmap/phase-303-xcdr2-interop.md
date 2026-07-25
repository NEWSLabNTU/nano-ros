# Phase 303 — XCDR2 + extensibility: modern ROS 2 wire interop

Implements **RFC-0055** (wire encoding); the encoding default + extensibility it
turns on are **fields of the ROS-edition profile** (**RFC-0056**) — `humble`
keeps XCDR1, `jazzy`+ turns on XCDR2. Sibling phase **phase-41** owns the other
edition-profile field (RIHS01 type hash for Iron+); **phase-304** coordinates the
whole axis + the multi-distro test method (this phase's W1b = phase-304 W5's
encoding field). Roots: issue **#0267** (Control
mis-walked after a `domain_bridge` republish — nano-ros CDR proven canonical,
the gap is the missing DHEADER + implicit type extensibility) and the adjacent
serialization gaps (big-endian read, type-hash/RIHS).

Goal: nano-ros is byte-compatible with a **default ROS 2 humble+ peer**
regardless of the data representation the peer negotiates — specifically,
`@appendable` nested structs survive an XCDR2 decode (their DHEADER is present).

## Status (2026-07-25)

W1 STARTED — and produced a **blocking finding** (below): the naive
"emit `@appendable`" is wrong for ROS 2 Humble and was reverted. The remaining
waves (W2–W6) are gated on capturing the demo's downstream distro + negotiated
representation first (RFC-0055 open-Q5). The #0267 investigation's byte-exact
`nros-serdes` regression test proves the serializer canonical, so this phase
builds ON a known-good XCDR1 baseline — every wave keeps the XCDR1 tests green.

## W1 finding (2026-07-25) — blanket `@appendable` diverges from ROS 2 Humble

First W1 attempt added `@appendable` to every generated struct in
`nros-msg-to-idl`. **Reverted.** The emitter has a byte-for-byte **parity
contract** with ROS 2's own `rosidl_adapter` (`nros-msg-to-idl/tests/parity.rs`,
references captured from **rosidl_adapter Humble**), and those `.idl`s carry
**no extensibility annotation** — so a blanket `@appendable` breaks parity and
diverges from what Humble actually emits. nano-ros already produces the SAME
`.idl` as Humble → same idlc → same descriptor → same wire extensibility as a
native Humble cyclone node. Blanket `@appendable` is only correct against an
Iron/Jazzy+ (appendable/XCDR2) peer, and must be **distro-gated**, not applied
unconditionally. See RFC-0055 §"Finding (phase-303 W1)".

The parity guard doing its job here is the point: the change was proven wrong
before it shipped, no incorrect `.idl` landed.

## Background — two paths, one gap

- **`nros-serdes`** (Rust, `no_std`): XCDR1-only, LE-only, no encoding-version
  parameter. Consumed by the zenoh-pico / XRCE / native-Rust RMW paths.
- **CycloneDDS C++ path**: `dds_stream_write_sample` + idlc descriptor. Cyclone
  does XCDR2/DHEADER natively — but `nros-msg-to-idl` emits NO extensibility
  annotation, so the descriptor is implicit.

The cheap half (W1) fixes the Cyclone path by declaring extensibility in the
IDL; the substantial half (W2–W4) teaches `nros-serdes` XCDR2.

## Work items

### W1 — capture the target-distro extensibility (the gate; see finding above)

The blanket-`@appendable` approach is DISPROVEN (finding above). W1 is now the
diagnostic gate that unblocks the rest:

1. From the live #0267 setup, capture: the **downstream ROS distro**, the
   `data_representation` QoS it negotiated with the bridge's GenericPublisher,
   and the actual **extensibility** of both descriptors — nano-ros's
   vendored-idlc output vs the peer's rmw_cyclonedds descriptor for the same
   `autoware_control_msgs/Control`.
2. Confirm nano-ros's vendored cyclone **idlc default extensibility** (does it
   implicit-FINAL, or warn/APPENDABLE?) and whether it matches the target's.

- *Delivers:* the concrete answer to RFC-0055 open-Q5 — whether the fix is
  "emit XCDR2/DHEADER" (W2/W3), "align nano-ros's idlc default", or "gate a
  `@appendable` annotation on an Iron/Jazzy+ target". Only THEN does an
  extensibility change land — and any `.idl` annotation must be distro-gated so
  the Humble parity contract (`nros-msg-to-idl/tests/parity.rs`) stays green.
- *Accept:* the demo's downstream distro + negotiated representation +
  both-descriptor extensibility are documented in #0267; the fix direction
  (W2/W3 vs idlc-default vs distro-gated annotation) is chosen with evidence,
  not inference.

### W1b — extend the ROS-edition axis (RFC-0056) for `jazzy`

Prerequisite for gating the encoding profile: add the `ros-jazzy` feature +
`generated/jazzy/` interface dir, and make the wire-encoding default a
**profile field selected by the edition** (like `nros-rmw-zenoh::keyexpr`
already selects the type-hash tail by `ros-iron`). `humble` → XCDR1 (unchanged,
parity intact); `jazzy` → the XCDR2 path (W2–W4).

- *Accept:* a `ros-jazzy` build selects the XCDR2 encoding profile while a
  `ros-humble` build is byte-identical to today (the `rosidl_adapter` parity
  suite + the XCDR1 compat suite stay green); the profile is read from RFC-0056's
  table, not scattered `#[cfg]`s.

### W2 — `nros-serdes` XCDR2 writer

`CdrWriter` gains an encoding version; a nested `@appendable` type serialized
under XCDR2 (encapsulation `0x0008`/`0x0009`) is wrapped in a DHEADER; 8-byte
primitives align to 4 under XCDR2.

- *Accept:* new byte-exact compat tests — the Control layout under XCDR2 emits a
  DHEADER before each appendable nested struct at the canonical offset (compare
  against `rmw_cyclonedds_cpp` XCDR2 output for the same type); the XCDR1 tests
  stay green (version-gated). DHEADER length strategy per RFC-0055 open-Q1.

### W3 — `nros-serdes` XCDR2 reader

`CdrReader` dispatches on the encapsulation id; an XCDR2 appendable type reads +
validates its DHEADER and bounds the member walk by it (skips unknown trailing
bytes — forward compat).

- *Accept:* round-trip an XCDR2 appendable message (write W2 → read W3); decode
  a `rmw_cyclonedds_cpp`-produced XCDR2 buffer with a trailing added member and
  skip it cleanly; XCDR1 buffers still decode via the existing path.

### W4 — representation negotiation

Advertise `data_representation` QoS `[XCDR2, XCDR1]`; the writer emits the
selected representation; per-endpoint override for the constrained embedded
default (RFC-0055 open-Q4).

- *Accept:* nano-ros ↔ a default humble peer negotiates XCDR2 and interoperates;
  nano-ros ↔ a legacy XCDR1-only peer falls back to XCDR1; the **#0267 demo
  clears** (Control survives the `domain_bridge` republish) — the phase's
  done-when.

### W5 — big-endian read

The `nros-serdes` reader honors the encapsulation endianness bit instead of
rejecting `0x0000`/`0x0006`/`0x0008`.

- *Accept:* a hand-built big-endian CDR buffer round-trips; LE unchanged.

### W6 — type-hash / RIHS (optional; interop hardening)

Compute + advertise the RIHS type hash; check it at subscription match so a
type mismatch fails loud instead of decoding garbage. Interacts with the
Cyclone descriptor W1 already carries.

- *Accept:* a same-named-but-different type is REJECTED at match with a loud
  diagnostic rather than delivering garbage.

## Non-goals

- **`@mutable` / optional members** (PL_CDR2 / EMHEADER) — standard ROS 2
  messages don't use them; deferred (RFC-0055 §Alternatives).
- Changing the constrained embedded default away from XCDR1 unilaterally — the
  per-endpoint override is the mechanism; the default is an open question.

## Done when

A nano-ros cyclone publisher's `@appendable` message crosses a `ros2
domain_bridge` generic republish and the downstream typed subscriber decodes it
correctly (the #0267 acceptance), AND nano-ros negotiates XCDR2 with a default
humble peer while still falling back to XCDR1 for a legacy peer — with the
XCDR1 byte-exact compat suite unbroken throughout.
