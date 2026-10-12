---
rfc: 0056
title: "ROS edition axis: per-distro interop profile (type hash + wire encoding + interface set)"
status: Draft
since: 2026-07
last-reviewed: 2026-10
implements-tracked-by: [phase-304, phase-41, phase-303]
supersedes: []
superseded-by: null
---

# RFC-0056 — ROS edition axis

## Summary

**ROS edition** (the target ROS 2 distribution) is one of nano-ros's three
orthogonal build axes (RMW × platform × **ROS edition**, ARCHITECTURE §2), and
— unlike RMW/platform, which are lowering targets confined to board/backend
crates — it is a **functional** feature that ordinary crates (`nros-serdes`,
`nros-node`, the interface packages, codegen) may branch on. This RFC
formalizes the axis: the set of supported editions, the **per-edition interop
profile** (what actually varies on the wire between distros), how an edition is
selected + lowered, and how the two implementation phases (type hash →
phase-41, wire encoding → phase-303) feed one coherent profile instead of
scattered `#[cfg]`s.

Today the axis exists but is **enumerated only `humble`/`iron`** and carries
only the type-hash difference. This RFC extends it to `jazzy` and
folds in the wire-encoding profile (RFC-0055), so "match the peer's ROS distro"
is a single first-class selection rather than a per-call guess (the phase-303
W1 finding: extensibility is distro-matched, never blanket-emitted).

## Motivation / problem

nano-ros interoperates with a real ROS 2 graph over DDS/zenoh. What a peer
expects on the wire **changes across ROS distributions**, independently of
which RMW or platform nano-ros runs:

- **Type hash / discovery.** Humble uses `TypeHashNotSupported` in the zenoh
  data keyexpr and a zeroed placeholder (`RIHS01_<64×0>`) in liveliness tokens.
  **Iron+** computes real **RIHS01** SHA-256 hashes (REP-2011) and a peer may
  reject a mismatched/placeholder hash at discovery. (Already partly handled:
  `nros-rmw-zenoh::keyexpr` branches on `ros-iron`; the RIHS01 computation is
  phase-41, not-started.)
- **Wire encoding / extensibility.** **NOTE (2026-07-26): this is NOT actually an
  edition-profile field — it is per-type, so it is being removed from this axis.**
  Live verification (RFC-0055 CORRECTION, #0267) showed a **default Jazzy peer is
  FINAL/XCDR1** on the wire (both fastrtps + cyclonedds), byte-identical to
  Humble. Extensibility/XCDR2 depends on a specific type's `@appendable`
  annotation, not the distro, and a per-edition blanket BREAKS interop
  (DDS-XTypes rejects an appendable writer against a FINAL reader). So the edition
  axis carries only **type hash** and the **interface set**; encoding/extensibility
  is a per-type property handled where a type declares `@appendable`
  (RFC-0055, parked). The bullet below is the original (refuted) framing.
  ~~Humble is effectively XCDR1/FINAL; later distros moved toward XCDR2 +
  `@appendable`; match the peer's edition.~~
- **Interface set.** Message definitions and available packages differ across
  distros; nano-ros already generates interfaces into per-distro dirs
  (`packages/interfaces/*/generated/{humble,iron}/`).

These are all facets of **one variable — the target ROS edition** — yet they
are currently either hard-coded to Humble, branched ad-hoc (`ros-iron`), or
unbuilt (RIHS01, XCDR2). The axis needs a single profile so a new distro is one
table row, not a scavenger hunt.

## Design

### Supported editions

| Edition | Feature | Status | Ubuntu / EOL | Notes |
| --- | --- | --- | --- | --- |
| `humble` | `ros-humble` (default) | supported | 22.04 LTS | XCDR1/placeholder-hash; the current baseline |
| `iron` | `ros-iron` | keyexpr done; RIHS01 pending (phase-41) | 23.05 (EOL) | RIHS01 type hashes introduced |
| `jazzy` | `ros-jazzy` | RIHS01 done; harness green (phase-309/310) | 24.04 LTS | RIHS01; **FINAL/XCDR1 on the wire** (per-type `@appendable` only; NOT XCDR2-by-edition — see the encoding note) |

Mutually exclusive within the axis (compile-time), like the other two axes.
`humble` is the default when none is selected.

### The per-edition interop profile

One profile per edition — the single source of truth for cross-distro behavior:

| Profile field | humble | iron | jazzy |
| --- | --- | --- | --- |
| **Type hash** | `TypeHashNotSupported` + zeroed liveliness placeholder | RIHS01 SHA-256 (REP-2011) | RIHS01 SHA-256 |
| **Data-keyexpr tail** | `…/<type>/TypeHashNotSupported` | `…/<type>/<RIHS01>` | `…/<type>/<RIHS01>` |
| **Wire encoding default** | XCDR1 (`0x0001`) | XCDR1 | **XCDR1** (a default jazzy peer is FINAL/XCDR1 — verified live; NOT XCDR2-by-edition) |
| **Type extensibility** | as ROS emits (no explicit annotation; matches `rosidl_adapter`) | same | **FINAL by default**; `@appendable` is per-type, not per-edition (RFC-0055 parked) — *not an edition-profile field* |
| **Interface set** | `generated/humble/` | `generated/iron/` | `generated/<edition>/` |

Implementation notes:

- The profile is **selected by the edition feature**, the same mechanism the
  keyexpr already uses (`#[cfg(feature = "ros-iron")]`). New editions add a
  feature + a row; consumers read the profile, not scattered per-site `cfg`s
  where avoidable.
- **Type hash (phase-41)** — compute RIHS01 per REP-2011 (research:
  `docs/research/rep-2011-type-hash.md`), gate the real hash on `iron`+, keep
  the Humble placeholder on `humble`. Feeds the keyexpr + liveliness + the
  RFC-0055 §type-hash/RIHS interop check.
- **Wire encoding (phase-303 / RFC-0055) — REMOVED from the profile (2026-07-26).**
  Live verification showed a default jazzy peer is FINAL/XCDR1, so encoding/
  extensibility is NOT edition-driven; ALL editions emit XCDR1/FINAL. The XCDR2
  writer/reader + DHEADER machinery is built but **parked**, to be re-activated
  per-type where a type declares `@appendable` — never by edition. (See the
  RFC-0055 CORRECTION and #0267.)
- **Selection + lowering** — the edition is DECLARED in `system.toml`, per
  image (see "Declared per image" below), and lowered to the `ros-<edition>`
  feature on the `nros` umbrella (→ `nros-node/ros-*`), on the board crate
  (→ the backend's own `ros-*`, which the zenoh keyexpr gates its RIHS01 tail
  on), and to codegen's `--ros-edition` (the baked `TYPE_HASH`). A cargo feature
  is the lowering target, never the user's knob — RFC-0031's rule for the RMW.

### Declared per image (amended 2026-10-12, issue 1305)

The edition is a property of the BINARY, like the RMW (RFC-0031 § "Scope"), so
it is declared where the binary is: `[image.<id>] ros_edition`, with the same
precedence as `rmw`:

1. `[image.<id>] ros_edition`
2. `[image_defaults] ros_edition`
3. `[system] ros_edition`
4. `humble`

An unknown name at any rung is a hard error naming the table it is in (the
typo guard `[system] ros_edition` always had). One resolver,
`SystemToml::image_ros_edition`, answers for both halves that must agree: the
selection writer (`facade::derive`, shared by a workspace entry's facade and a
single-package leaf's `<pkg>_nros_selection`) and the leaf's message codegen
(`nros sync --image <id>`). So one leaf can build a humble image and a jazzy
image — `examples/native/rust/*` declare `native`, `native_jazzy`,
`native_xrce_jazzy`, … for `just ros_editions build-e2e-fixtures`, which used
to pass `--features "rmw-<x> ros-<edition>"`.

A leaf's `generated/` holds ONE edition's message crates at a time, so `nros
sync` records the edition it generated for (`generated/.nros-ros-edition`) and
`nros build <id>` refuses an image whose edition differs, naming `nros sync
--image <id>`. A workspace still generates its shared message crates for
`[system] ros_edition`; per-image editions inside one workspace would need one
`generated/` per edition and are not supported.

### Non-goals

- Runtime multi-edition in one binary (the axis is compile-time exclusive, like
  RMW/platform).
- Full source compatibility with every distro's message package set — nano-ros
  generates the interfaces a workspace declares, per edition.

## Alternatives considered

- **Stay Humble-only.** Rejected — blocks interop with the current LTS (Jazzy)
  and any Iron+ node that enforces RIHS01 at discovery.
- **Scatter `#[cfg(feature = "ros-iron")]` per behavior (status quo).** Works for
  one difference (keyexpr) but does not scale to type-hash + encoding +
  interface deltas across four distros; a profile table is the maintainable form.
- **Detect the peer's distro at runtime and adapt.** Rejected for the wire
  encoding (the encapsulation id already self-describes per message, so the
  reader adapts) but the OFFERED representation + type hash are build-time
  identity; a compile-time edition is the honest selection.

## Open questions

1. ~~**`system.toml` lowering** — should the ROS edition become a declared
   `[system].ros_edition` (like `rmw`) that codegen lowers to the `ros-<edition>`
   feature + the interface-generation dir, instead of a hand-set cargo feature?~~
   **ANSWERED (2026-10-12, issue 1305): yes, and per IMAGE.** `[system]
   ros_edition` landed in [phase-304](../roadmap/archived/phase-304-complete-ros-edition-axis.md)
   W2; `[image.<id>]` / `[image_defaults] ros_edition` rank above it with the
   RMW's precedence (§ "Declared per image"), and no example names an edition
   feature.
2. **RIHS01 source** — compute in `rosidl-codegen` (add `sha2`, REP-2011
   canonical form) vs extract from an installed ament index vs hybrid
   (phase-41 §Implementation Options).
3. **Jazzy encoding default** — offer `[XCDR2, XCDR1]` (negotiate, safest) vs
   XCDR2-only; interacts with RFC-0055 open-Q4 (embedded default) and open-Q5
   (capture the live peer's negotiated representation before committing).
4. **Edition granularity for the embedded/constrained images** — do zenoh-pico
   / XRCE targets carry the full per-edition profile, or a reduced one (they
   rarely face a strict-RIHS Iron+ peer)?

## Changelog

- 2026-07 — created. Formalizes the ROS-edition axis (was `humble`/`iron`,
  keyexpr-only) into a per-edition interop profile spanning type hash
  (phase-41), wire encoding/extensibility (RFC-0055 / phase-303), and interface
  set; extends the enum to `jazzy`. (rolling is unsupported — a nightly release.)
- 2026-10-12 (issue 1305) — the edition is declared per image
  (`[image.<id>]` > `[image_defaults]` > `[system]` > humble, RFC-0031's
  precedence) and lowered by the selection writer onto the umbrella and the
  board crate; open question 1 answered.
