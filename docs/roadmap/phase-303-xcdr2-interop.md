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

## W1 continued (2026-07-26) — the gate is substantially resolved OFFLINE

Deep offline investigation (recorded in #0267, "phase-303 W1 continued")
resolves the diagnostic gate without the live demo, and REFINES the fix:

- **The `.idl` annotation layer is retired.** Humble AND Jazzy ship identical,
  UNANNOTATED `.idl` for nested-`Time` types — so neither `@final` nor
  `@appendable` in `nros-msg-to-idl` is correct; it would break parity with both.
  The extensibility control is NOT in the `.idl` text.
- **nano-ros's cyclone descriptor is FINAL by construction — located.** The
  runtime builder `packages/dds/nros-rmw-cyclonedds/bridge/dynamic_type_builder.cpp`
  sets only `DDS_TOPIC_FIXED_SIZE` in `m_flagset` and emits no `DDS_OP_DLC`
  DHEADER op. That is the cyclone-path control point.
- **The blocker's premise is dissolved by RFC-0056.** The gate was "we don't know
  the downstream distro." The ROS-edition axis (phase-304, now landed) makes it a
  DECLARED value (`[system].ros_edition`), so the encoding/extensibility profile
  is chosen, not inferred.

**Refined fix split (both edition-gated):**
- **W1c — cyclone descriptor extensibility — LANDED (2026-07-26).**
- **W2/W3 (unchanged):** `nros-serdes` XCDR2 writer/reader + DHEADER for the
  zenoh-pico / XRCE / native-Rust paths.

### W1c — cyclone descriptor extensibility — LANDED (2026-07-26)

`nros_cyclonedds_build_descriptor_from_schema` gained an `extensibility`
argument. `0` (FINAL, humble) is byte-identical to pre-W1c; non-zero
(APPENDABLE, iron/jazzy+) prefixes EACH aggregate's op stream (top-level + every
nested struct body) with `DDS_OP_DLC`. Cyclone derives extensibility from the
leading op (`dds_stream_extensibility`) and writes a per-nested-struct DHEADER
under XCDR2 — matching a modern ROS 2 peer, so the domain_bridge republish no
longer mis-walks. `m_flagset` is unchanged; the DLC in the ops is the whole
mechanism. NO `.idl`/`nros-msg-to-idl` parity impact.

Edition gate: `dynamic_type::TYPE_EXTENSIBILITY` (`0` humble / `1` iron+jazzy+
rolling), keyed on `ros-<edition>` features on `nros-rmw-cyclonedds`, forwarded
by `nros-rmw-cyclonedds-sys` — mirroring `nros-rmw-zenoh[-staticlib]`'s keyexpr
edition wiring. A cyclone app/board enables `nros-rmw-cyclonedds-sys/ros-<edition>`
the same way it already enables `nros-rmw-zenoh/ros-<edition>` (per-crate, the
established pattern).

**Verified against the REAL Cyclone library** (`tests/appendable_extensibility.cpp`,
compiled + run vs `libddsc.a`): FINAL descriptor has 0 DLCs (byte-identical);
APPENDABLE has exactly one DLC per aggregate — a flat type → op0 = `DDS_OP_DLC`,
`nops`+1; a nested type → 2 DLCs (top + nested body). Rust `TYPE_EXTENSIBILITY`
const test covers the edition gate. This is NOT a blind land — the descriptor
shape is proven; only the end-to-end WIRE delivery (the domain_bridge demo
clearing) remains for the live-demo confirmation.

Still to confirm on the live demo (not blocking W1c): the downstream's negotiated
`data_representation` + its runtime sertype extensibility for
`autoware_control_msgs/Control` — the final wire-level closure.

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

**W2 + W3 CORE LANDED (2026-07-26) — the `nros-serdes` XCDR2 machinery.**
`CdrWriter`/`CdrReader` gained an `EncodingVersion` (Xcdr1 default / Xcdr2):
- `new_with_header_xcdr2` writes the `0x0009` DELIMITED_CDR2 header; the reader
  parses `0x0008`/`0x0009` → Xcdr2, `0x0000`/`0x0001` → Xcdr1.
- `begin_dheader`/`end_dheader` on BOTH sides: the writer reserves + backpatches
  a 4-byte DHEADER (RFC-0055 open-Q1 → backpatch chosen — the writer has random
  access); the reader reads + bounds by it and SKIPS unknown trailing members
  (forward compat). Under XCDR1 both are pure NO-OPs → generated code can wrap
  every struct unconditionally with ZERO Humble impact.
- `align` caps at 4 under XCDR2 (8-byte primitives align to 4).
- Byte-exact + round-trip tests (`cdr::tests::xcdr2_*`): nested-struct DHEADER
  layout, XCDR1-identity of the no-op wrap, and trailing-member skip. `DeserError`
  gained `DHeaderOverrun`.

**W2/W3 REMAINING (the integration — folded into W4 below, needs a live peer to
verify end-to-end):** (a) codegen wraps each generated `serialize`/`deserialize`
body in `begin/end_dheader` (safe — no-op under XCDR1; regenerates all message
crates); (b) the RMW constructs an XCDR2 writer when the edition/negotiation
selects it. These are inert until (b) flips, so they land WITH the negotiation so
the whole path is wire-verifiable at once (a real Jazzy peer), not as inert code.

### W4 — codegen DHEADER wrap + edition-gated tx writer — LANDED (2026-07-26)

Wired the W2/W3 machinery into generated code + the Rust RMW path:
- **codegen wrap:** the `message`/`service`/`action` `serialize`/`deserialize`
  templates wrap each struct body in `begin_dheader`/`end_dheader`
  (`writer`/`reader`) — including the empty-struct + `deserialize_borrowed`
  variants. No-op under XCDR1 (byte-identical); under XCDR2 delimits every struct
  with a DHEADER. Generated msg + service compile-checked
  (`heap_compile_check.rs`).
- **edition-gated tx writer:** `nros_node::tx_writer` picks
  `new_with_header_xcdr2` for iron/jazzy/rolling, `new_with_header` for humble;
  every production publish/action/lifecycle path routes through it (the reader
  auto-detects the version from the encapsulation header, so ONLY the writer is
  gated). nros-node green under BOTH `--features ros-humble` (default) and
  `--features ros-jazzy` (231 tests).
- **negotiation:** for the nros-serdes RMW paths (zenoh-pico / XRCE / native)
  there is no runtime `data_representation` handshake — the edition selects the
  format at compile time, matched endpoints agree, and the reader is
  self-describing (header id). The DDS/Cyclone path negotiates natively (W1c).

**W4 REMAINING (needs a live Jazzy peer to verify end-to-end):** the C/C++ tx
paths (`nros-c`/`nros-cpp` `cdr.rs` writers) still construct XCDR1 writers — they
need the same `tx_writer` gate for full C/C++ jazzy support; and the actual wire
round-trip against a real Jazzy `domain_bridge`/peer (the #0267 demo clearing) is
the final confirmation. The Rust pub/sub/action path is complete + edition-gated.

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
