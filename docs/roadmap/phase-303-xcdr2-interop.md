# Phase 303 — XCDR2 + extensibility: modern ROS 2 wire interop

Implements **RFC-0055** (wire encoding). Roots: issue **#0267** (Control
mis-walked after a `domain_bridge` republish — nano-ros CDR proven canonical,
the gap is the missing DHEADER + implicit type extensibility) and the adjacent
serialization gaps (big-endian read, type-hash/RIHS).

Goal: nano-ros is byte-compatible with a **default ROS 2 humble+ peer**
regardless of the data representation the peer negotiates — specifically,
`@appendable` nested structs survive an XCDR2 decode (their DHEADER is present).

## Status (2026-07-25)

Not started. RFC-0055 `Draft`. The #0267 investigation landed the byte-exact
`nros-serdes` regression test proving the serializer canonical, so this phase
builds ON a known-good XCDR1 baseline — every wave keeps the XCDR1 tests green.

## Background — two paths, one gap

- **`nros-serdes`** (Rust, `no_std`): XCDR1-only, LE-only, no encoding-version
  parameter. Consumed by the zenoh-pico / XRCE / native-Rust RMW paths.
- **CycloneDDS C++ path**: `dds_stream_write_sample` + idlc descriptor. Cyclone
  does XCDR2/DHEADER natively — but `nros-msg-to-idl` emits NO extensibility
  annotation, so the descriptor is implicit.

The cheap half (W1) fixes the Cyclone path by declaring extensibility in the
IDL; the substantial half (W2–W4) teaches `nros-serdes` XCDR2.

## Work items

### W1 — explicit extensibility in generated code (the honesty half)

`nros-msg-to-idl` emits `@appendable` (default, matching rosidl) / `@final`
(only where the `.msg`/IDL says so) on every generated struct;
`rosidl-codegen` emits the matching `EXTENSIBILITY` const on the Rust type.

- *Delivers:* the **Cyclone path** becomes XCDR2/DHEADER-correct immediately
  (the idlc descriptor now carries the real extensibility; `dds_stream`
  honors it). The Rust type learns its extensibility for W2.
- *Accept:* generated IDL for a nested-struct type (e.g. a `Header`-bearing
  message) carries `@appendable`; a Cyclone round-trip of that type through a
  representation boundary decodes clean; the implicit-extensibility idlc
  warning is gone. Verify against a **live ROS 2 peer** (or the #0267 demo) —
  do NOT flip extensibility blind (RIHS hash changes; RFC-0055 alternatives).

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
