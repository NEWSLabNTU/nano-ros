---
id: 1228
title: "phase-439 W2 landed the resolve phase; the three-pass fixed point is still authoritative and RFC-0094 A3 is not met"
status: open
area: build, cmake, cli
severity: medium
found: 2026-09-08
related: [0940, 0965, 0991, 1002, 1119, 1061, 1125, phase-439, phase-403, phase-412]
---

# What landed, what did not, and the one measurement that decides the rest

phase-439 W2 landed stage 3.5 of `nros build` — the resolve phase — as
RFC-0094 D1/D2 specifies it: it reads declarations, runs
`EntityInventory::derive` once per image, and writes
`build/<image>/resolved.toml` with `[provenance]` and a digest, before any
configure. `cmake/NanoRosResolved.cmake` is the configure-side reader, wired at
the two sites that READ the entity-inventory fragment.

It did **not** meet RFC-0094 **A3**, and this issue is the remainder.

    A3 — the fixed point is gone. No lane re-derives a knob during configure;
    `nros_reconfigure_settle` and the future-mtime arm are deleted, and Zephyr
    converges in one pass.

`nros_reconfigure_settle` and the future-mtime arm in
`cmake/NanoRosReconfigure.cmake` are **still present and still authoritative**.
Nothing about the pass count on a real Zephyr image changed.

## The measurement that says why, and it is one comment line

`nros_reconfigure_snapshot` hashes **content**. So the seed removes a configure
pass only when the mid-configure producer that follows it writes byte-for-byte
what the seed already wrote. Case B of `tests/cmake-resolved-seed-tests.sh`
measures exactly that on a five-line project — with an agreeing seed the chain
goes from 1 re-configure to 0.

The two composers in the real tree do **not** agree byte-for-byte, and the whole
difference is one line:

| | stage 3.5 | the mid-configure producer |
| --- | --- | --- |
| component set | the resolved SystemModel | `nros-metadata.json` merged with the model |
| derived numbers | — | **identical**, measured |
| `NROS_ENTITY_INVENTORY_SOURCE` | `<model path>` | `<metadata path> + <model path>` |

`resolve::tests::stage_3_5_and_the_mid_configure_producer_agree_on_every_number`
asserts both halves: every value equal, and the byte difference confined to the
source line. It is written to red if the numbers ever diverge — that direction
would make the seed unsafe, not merely inert.

So today the seed makes pass 1 read a **real number instead of a placeholder**,
which is a real improvement in what a first-pass reader sees, and the producer
still arms one re-configure over a provenance comment.

## Why the source line is not simply deleted

It is provenance, and RFC-0094 D2 makes provenance normative. The honest fixes
are one of:

1. **Make the producer read the resolve rather than recompose.** Then there is
   one composer and the question does not arise. This is what RFC-0094 actually
   asks for and it is blocked on the next item.
2. **Give the resolve phase the refusal guard the producer has.** The producer's
   `nros-metadata.json` component set is what makes `derive()` REFUSE when a
   registered component is absent from the launch declaration; stage 3.5 reads
   the model alone (the metadata is written DURING a configure, which is the lag
   the phase exists to remove), so it cannot see that case and would derive a
   number for an image the producer correctly refuses. **Do not skip this**:
   `tests/cmake-resolved-seed-tests.sh` case C exists because a seed that can
   override the producer is how stage 3.5 would ship a count nothing stood
   behind.
3. **Move the source string out of the hashed fragment**, into `resolved.toml`
   where the rest of `[provenance]` already lives. Cheapest, and it makes 1
   possible without 2 — but only if a reader that wants the source is left able
   to find it.

## The other half of the chain, untouched

Even with the entity link closed, Zephyr's chain has a second link:
`message_bound_knobs.cmake` is derived by `nros_find_interfaces()` mid-configure
from codegen-produced bound FRAGMENTS, and is read by `nros_resolve_knobs()`
earlier still. Stage 3.5 does not compute it — `nros_derive_message_bound_knobs`
is a pure-CMake composer over per-package fragments, and there is no Rust twin
to call before a configure. One pass more.

So A3 needs both links, and the honest ordering is: (3) then (1), measured with
`tests/cmake-resolved-seed-tests.sh`; then the bound half; then the deletion.

## Not measured here, deliberately

**No claim is made about a real Zephyr image.** This host has no
`zephyr-workspace` and no Zephyr SDK, so "converges in one pass" and "a named
image's knobs are byte-identical before and after" were not run and are not
claimed. phase-392 W5 had to withdraw a causal claim from a before/after that
accidentally built the same configuration twice; the acceptance for the deletion
must be a real `west build` on a real image with `check-knob-delivery
<build-dir>` green on both sides, and the unrelated knob values asserted
UNCHANGED as the control.

## What exists to build on

* `packages/cli/nros-cli-core/src/resolve.rs` — the phase, the artifact, the
  digest, `[provenance]`, and `HAND_SET_KNOBS` (the two knobs with no
  declarative derivation now SAY they are hand-set, which was W2's stated
  known-gap deliverable).
* `packages/cli/nros-cli-core/src/cmd/build.rs` — `resolve_image()`, stage 3.5,
  between the preflight bail and the driver match. It cannot fail a build.
* `cmake/NanoRosResolved.cmake` — `nros_resolved_dir`,
  `nros_resolved_entity_fragment`, `nros_resolved_seed_entity_inventory`.
* `-DNROS_RESOLVED_DIR=<dir>` on the cmake and west handoffs; unset elsewhere,
  so a bare `west build` or `just zephyr build-fixtures` is unchanged.
* `just check resolved-seed` — the pass-count measurement, with its negative
  controls.
