---
rfc: 0055
title: "Wire encoding: XCDR2 + explicit type extensibility for modern ROS 2 interop"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-303]
supersedes: []
superseded-by: null
---

# RFC-0055 — Wire encoding: XCDR2 + explicit type extensibility

## Summary

nano-ros serializes ROS 2 messages as **XCDR1** (PLAIN_CDR, encapsulation
`0x0001`), little-endian only, with **no declared type extensibility**. Modern
ROS 2 (Humble+) defaults message types to **`@appendable`** and offers
**XCDR2** as a data representation, under which an appendable struct carries a
4-byte **DHEADER** (a delimiter length). Across a representation boundary — the
concrete trigger was a `ros2 domain_bridge` generic re-publish (issue #0267) — a
downstream reader using the type's XCDR2/appendable typesupport expects a
DHEADER that nano-ros's XCDR1-FINAL byte stream does not contain, consumes 4
payload bytes as a phantom length, and mis-walks every following member.

This RFC extends the nano-ros serialization stack to speak **XCDR2 with
DHEADERs for appendable types** and to **declare extensibility explicitly** in
generated code, so nano-ros is byte-compatible with a default ROS 2 humble+
peer regardless of the representation the peer negotiates. It also records the
adjacent interop gaps (big-endian read, type-hash/RIHS, mutable/optional) and
where each lands.

> **⚠️ CORRECTION (2026-07-26) — the driving premise was refuted; this is now an
> OPTIONAL per-type feature, not a modern-edition requirement.** Two live findings
> from the #0267 investigation overturned the framing above:
> 1. **A default Jazzy peer serializes FINAL/XCDR1 on the wire** (verified live
>    for both fastrtps and cyclonedds; guard
>    `nros_serdes::cdr::tests::xcdr1_header_matches_live_jazzy_wire_bytes`). "Modern
>    ROS 2 defaults types to `@appendable`/XCDR2 so nano-ros must too" is wrong —
>    and DDS-XTypes REJECTS an appendable writer against a FINAL reader, so
>    emitting appendable-by-edition BREAKS default interop. Extensibility is a
>    **per-type** property (a specific type's `@appendable` annotation), never an
>    edition property.
> 2. **#0267 was NOT an XCDR2/DHEADER gap.** Its real cause was two bugs in the
>    Cyclone runtime descriptor builder (`m_size` under-size for a nested member
>    >16 B, and a preorder sibling-skip that read a struct's 2nd nested member as
>    a scalar), fixed independently (phase-309 line). nano-ros's XCDR1 bytes were
>    canonical all along.
>
> The XCDR2 + DHEADER machinery specified here stays **built + tested but parked**,
> to be re-activated per-type once an `@appendable` signal is wired for a type
> that genuinely declares it (e.g. a future autoware type). It is NOT on the
> default interop path. The `#0267 (high)` motivation below is retained for
> history but no longer holds.

## Motivation / problem

Two serialization paths exist in the stack; the gap is shared:

1. **`nros-serdes`** (pure-Rust, `no_std`) — the CDR writer/reader used by the
   zenoh-pico / XRCE / native Rust RMW paths and the embedded images. Today it
   is **XCDR1-only** (`CDR_LE_HEADER = [0x00,0x01,0x00,0x00]`), **little-endian
   only** (the reader rejects the big-endian `0x0000` header — "we only support
   little-endian for now", `cdr.rs`), and the `Serialize`/`Deserialize` traits
   take no encoding-version parameter (XCDR1 is hard-wired).
2. **The CycloneDDS C++ path** (`nros-rmw-cyclonedds`) — the data path uses
   Cyclone's `dds_stream_write_sample` / typed `dds_take` with an
   **idlc-compiled descriptor** (`m_ops`). Cyclone itself can emit XCDR1 or
   XCDR2 with DHEADERs, **driven by the descriptor's extensibility** — but
   `nros-msg-to-idl` emits struct definitions with **no `@final`/`@appendable`
   annotation**, so the descriptor's extensibility is implicit (a compiler
   default + warning), not the `@appendable` the canonical rosidl type declares.

Consequences, in interop-impact order:

- **#0267 (REFUTED — kept for history; see the CORRECTION above):** this phase
  hypothesized that `autoware_control_msgs/Control` → `Lateral`/`Longitudinal`
  was mis-walked because a peer decoded it as XCDR2-appendable and nano-ros's
  XCDR1-FINAL stream lacked the DHEADER. **That is not what happened.** nano-ros's
  XCDR1 bytes were canonical (correct), and the real fault was in the Cyclone
  runtime descriptor builder (`m_size` under-size + a preorder sibling-skip);
  fixed independently. Extensibility/DHEADER was never involved.
- **No representation negotiation:** nano-ros advertises only XCDR1, so it
  cannot honor a peer that requires XCDR2 (or agree on it when the peer prefers
  it).
- **Big-endian read gap:** the `nros-serdes` reader hard-rejects a big-endian
  CDR stream. Rare on LE hosts, but a spec-conformant BE publisher fails silently.
- **Type-hash / RIHS ignored:** the Cyclone subscriber discards the incoming
  `type_hash`, so a same-named-but-different type decodes as garbage instead of
  failing at match time.

## Design

### 1. Encoding model

CDR encapsulation ids nano-ros must understand and (per negotiation) emit:

| Id | Scheme | Extensibility it serves |
| --- | --- | --- |
| `0x0000/0x0001` | PLAIN_CDR (XCDR1) BE/LE | `@final`, `@appendable` (no DHEADER in v1) |
| `0x0002/0x0003` | PL_CDR (XCDR1) BE/LE | `@mutable` (member headers) |
| `0x0006/0x0007` | PLAIN_CDR2 (XCDR2) BE/LE | `@final` |
| `0x0008/0x0009` | DELIMITED_CDR2 (XCDR2) BE/LE | `@appendable` (**DHEADER** on the type) |
| `0x000a/0x000b` | PL_CDR2 (XCDR2) BE/LE | `@mutable` (**EMHEADER** per member) |

The **DHEADER** is a `uint32` (aligned to 4) prefixing an appendable
aggregated type, giving the serialized byte length of that type's members — a
reader that doesn't recognize a trailing added member skips to
`start + DHEADER`. The **EMHEADER** is a per-member header for `@mutable`.
XCDR2 also aligns 8-byte primitives to **4** (not 8).

**Scope of this RFC:** XCDR1 (`0x0001`, keep) + XCDR2 DELIMITED/PLAIN
(`0x0008/0x0009`, `0x0006/0x0007`) — i.e. `@final` and `@appendable`, the two
extensibilities every standard ROS 2 message uses. `@mutable` (PL_CDR2 /
EMHEADER) is **out of scope** (deferred; standard ROS 2 msgs do not use it).

### 2. Type extensibility is a generated property

Every message type gains a compile-time **extensibility** value
(`Final | Appendable`), derived from its `.msg`/`.idl` the same way rosidl
derives it (default `Appendable` unless the IDL/annotation says `@final`).
Codegen carries it two ways:

- **Rust (`rosidl-codegen`):** a `const EXTENSIBILITY: Extensibility` on the
  generated type (or an associated const on a new `CdrType` trait). The
  `nros-serdes` writer consults it to decide whether the type emits a DHEADER
  under XCDR2.
- **IDL (`nros-msg-to-idl`):** emit the explicit `@final` / `@appendable`
  annotation on every generated struct so the Cyclone descriptor is
  unambiguous. This alone makes the **Cyclone path** XCDR2/DHEADER-correct (the
  C++ `dds_stream` honors the descriptor) — the cheap half of the fix.

### 3. `nros-serdes` gains encoding-version awareness

- The `CdrWriter`/`CdrReader` carry the **encoding version** (XCDR1 vs XCDR2)
  and endianness, set from (writer) the negotiated representation and (reader)
  the received encapsulation id.
- A nested **`@appendable`** type serialized under **XCDR2** is wrapped in a
  DHEADER. Because the length is only known after serializing the members, the
  writer either (a) reserves 4 bytes, serializes the members, backpatches the
  length, or (b) size-precomputes via a `cdr_size(version)` the codegen emits.
  (Open question 1.)
- The reader dispatches on the encapsulation id: XCDR1 → today's path; XCDR2
  appendable → read + validate the DHEADER, then bound the member walk by it
  (skip unknown trailing bytes for forward-compat).
- 8-byte alignment becomes version-dependent (8 under XCDR1, 4 under XCDR2).

### 4. Representation negotiation

nano-ros advertises `data_representation` QoS `[XCDR2, XCDR1]` (XCDR2 preferred,
XCDR1 offered for legacy peers). The **writer** emits whichever the match
selected; the **reader** already dispatches on the on-wire encapsulation id, so
it accepts either from any peer. Default kept **XCDR1-compatible** on the
constrained embedded paths where the extra DHEADER bytes matter and the peer
set is known (a per-endpoint override).

### 5. Adjacent gaps (recorded; sequenced in phase-303)

- **Big-endian read:** honor the encapsulation endianness bit in the reader.
- **Type-hash / RIHS:** compute + advertise the RIHS type hash; check it at
  subscription match so a type mismatch fails loud instead of decoding garbage.

## Alternatives considered

- **Pin every generated type `@final`.** Makes nano-ros's XCDR1 bytes valid
  under any version (no DHEADER ever expected). REJECTED as the general answer:
  it changes the RIHS type hash and **diverges from the canonical ROS 2
  `@appendable` type**, so a peer holding the real `@appendable` type may not
  match, and cross-vendor evolvability is lost. `@final` is correct only for the
  rare type whose `.msg` truly declares it.
- **Stay XCDR1-only, document the `domain_bridge` caveat.** REJECTED: blocks
  interop with any peer that negotiates XCDR2 — the humble+ default — and leaves
  #0267 permanently worked-around instead of fixed.
- **Full XCDR2 incl. `@mutable`/optional (PL_CDR2/EMHEADER).** Deferred, not
  rejected: standard ROS 2 messages are `@final`/`@appendable` only, so
  `@mutable` support buys no interop today at large cost.

## Finding (phase-303 W1, 2026-07-25) — extensibility is NOT a blind blanket `@appendable`

A first W1 attempt emitted `@appendable` on every generated struct in
`nros-msg-to-idl`. It was **reverted**: the `nros-msg-to-idl` emitter is bound
by a **byte-for-byte parity contract** with ROS 2's own `rosidl_adapter`
(`tests/parity.rs`, reference `.idl`s captured from **rosidl_adapter Humble**),
and those references carry **NO extensibility annotation** — plain
`struct Time_ {`. So a blanket `@appendable` diverges from what ROS 2 Humble
actually produces and breaks parity.

Consequences for the design:

- nano-ros ALREADY produces the same `.idl` as ROS 2 Humble (byte-parity). Fed
  to the same idlc, it yields the same descriptor → the same wire extensibility
  as a native Humble cyclone node. So on a **pure-Humble** graph there is no
  extensibility mismatch to fix at the `.idl` layer.
- The real variables are (a) the **idlc default extensibility**, which is
  version-dependent (older idlc = implicit FINAL; newer idlc warns and can
  default APPENDABLE) — nano-ros's *vendored* idlc default must match the
  target's; and (b) the **downstream ROS distro** — Humble is effectively
  XCDR1/FINAL on the cyclone wire, while Iron/Jazzy+ moved toward
  APPENDABLE/XCDR2. Extensibility is therefore **distro/peer-matched or
  negotiated, never blanket-emitted**.
- The distro is a first-class build selection — the **ROS edition axis**
  (RFC-0056). The wire-encoding default + extensibility this RFC designs are
  **fields of that axis's per-edition profile**, not a standalone knob: `humble`
  keeps the XCDR1/`rosidl_adapter`-parity behavior, `jazzy`+ turns on the XCDR2
  path below. This RFC owns the ENCODING mechanism (DHEADER, negotiation);
  RFC-0056 owns WHICH profile applies per distro.
- This re-opens the #0267 diagnosis: if the nano-ros cyclone `.idl` matches
  Humble exactly, the corruption implies the downstream is NOT pure-Humble
  (a newer-distro reader decoding Humble XCDR1 data as XCDR2/appendable), OR
  nano-ros's vendored idlc default diverges from the target's. **The demo's
  downstream distro + its negotiated `data_representation` must be captured
  before any extensibility change lands** (see open Q5).

## Finding (LIVE verification, 2026-07-26) — extensibility/encoding is PER-TYPE, not per-edition

A live run (nano-ros cyclone talker ↔ a real ROS 2 Jazzy `ros2 topic echo`, DDS
domain 2, `std_msgs/Header`) overturned the "jazzy edition ⇒ appendable + XCDR2"
model this RFC assumed:

- nano-ros FINAL/XCDR1 → Jazzy decodes perfectly (byte-for-byte).
- nano-ros APPENDABLE → Jazzy REJECTS it ("incompatible QoS … INVALID").

**Jazzy's standard types are FINAL/XCDR1**; DDS-XTypes strict assignability
rejects an appendable writer against a final reader. So the wire-encoding +
extensibility **default is FINAL/XCDR1 for every edition** — a jazzy build is
byte-identical to humble and interoperates with a default Jazzy peer. XCDR2 +
DHEADER (§2–§4) is the correct mechanism for a type the peer treats as
`@appendable` (a PER-TYPE decision — the annotation on that specific type), and
must NOT be gated on the ROS edition. The machinery stays built; its ACTIVATION
moves to a per-type `@appendable` signal (future work). This supersedes the
edition-gated framing in the phase-303 W1b/W2b coordination — the edition axis
still owns the RIHS01 type-hash tail (phase-304), but NOT the encoding/extensibility.

## Open questions

1. **DHEADER length strategy** — backpatch (reserve-then-fill, one pass, needs a
   seekable writer position) vs a codegen-emitted `cdr_size(version)`
   precompute (two representations of the layout to keep in sync). The
   `no_std` embedded writer already has a random-access `buf` + `pos`, so
   backpatch is likely cheaper; confirm against the streaming XRCE path.
2. **How the Rust trait carries extensibility** — a `const` on the type, an
   associated const on a `CdrType` trait, or a parameter threaded through
   `serialize`. Must not regress the `no_std` / zero-alloc contract.
3. **RIHS scope** — compute the full RIHS01 hash (needs the complete type
   graph) vs a cheaper structural check. Interacts with the descriptor the
   Cyclone path already builds.
4. **Embedded default** — do the constrained zenoh-pico / XRCE embedded images
   default to XCDR1 (fewer bytes, known peer set) with XCDR2 opt-in, or XCDR2
   everywhere for uniform interop? Per-endpoint `data_representation` override
   is the mechanism either way.
5. **Target-distro extensibility (blocks the #0267 fix)** — ~~capture, from the
   live demo: the downstream ROS distro, the `data_representation` it
   negotiated, and the actual extensibility of BOTH descriptors.~~ **Largely
   RESOLVED offline (2026-07-26) — see #0267 "phase-303 W1 continued".** Key
   evidence: (a) Humble AND Jazzy ship IDENTICAL, UNANNOTATED `.idl` for
   nested-`Time` types → an `.idl` `@final`/`@appendable` annotation is the wrong
   layer and breaks parity with BOTH (the "align the `.idl`" option is
   RETIRED); (b) nano-ros's cyclone descriptor is FINAL by construction — the
   runtime builder `dynamic_type_builder.cpp` sets no appendable flag in
   `m_flagset` and emits no `DDS_OP_DLC` DHEADER op (NOT the idlc `.idl` route).
   **⇒ Decision:** the fix is at the DESCRIPTOR / SERIALIZER layer, edition-gated
   (RFC-0056): the cyclone path emits an appendable descriptor (flagset +
   `DDS_OP_DLC`) on iron/jazzy+, and `nros-serdes` gains XCDR2/DHEADER (W2/W3);
   Humble stays FINAL/XCDR1 byte-identical. The `.idl` layer is untouched, so the
   `rosidl_adapter` parity contract holds trivially. The ONE remaining live
   capture is a CONFIRMATION (the demo's downstream negotiated representation +
   its runtime sertype extensibility for `Control`), not an exploration — and no
   descriptor/serdes change lands before it.

## Changelog

- 2026-07 — created. Extends the wire encoding beyond XCDR1 to XCDR2 +
  explicit extensibility; roots the #0267 root-cause finding (nano-ros CDR
  proven canonical; the gap is DHEADER + implicit extensibility). Work
  breakdown in [phase-303](../roadmap/phase-303-xcdr2-interop.md).
