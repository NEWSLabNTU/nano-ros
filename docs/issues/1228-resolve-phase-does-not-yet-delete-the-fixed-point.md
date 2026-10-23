---
id: 1228
title: "phase-439 W2 landed the resolve phase; the three-pass fixed point is still authoritative and RFC-0094 A3 is not met"
status: open
area: build, cmake, cli
severity: medium
found: 2026-09-08
related: [0940, 0965, 0991, 1002, 1119, 1061, 1125, 1252, phase-439, phase-403, phase-412]
---

**Home: [phase-439](../roadmap/phase-439-resolve-before-configure.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

# UPDATE 2026-10-23 (later) — items (1) and (2) landed for the C/C++ west road (phase-439 W7)

The producer now READS the resolve instead of recomposing, and stage 3.5 has
the refusal guard. The guard did not need the configure's `nros-metadata.json`
after all. `nros sync`'s metadata probe already has every workspace
component's row, and a C/C++ west app `add_subdirectory()`s exactly the
packages its launch plan names. So the probe rows of those packages ARE the
rows its configure registers, available before any configure runs
(`registered_from_probe`). A registered component the contract does not
describe is an absent row on both sides, and both refuse.

One composer. `nros ws entity-inventory`'s single-model path and stage 3.5 both
call `cmd::entity_inventory::compose_image`. The resolve records the inputs it
composed over (`resolved-entity-inputs.cmake`), and the producer copies
`resolved.cmake` and `resolved-entity-inventory.json` only when its own inputs
match. A mismatch (a component registered since the last sync, several models,
another workspace, another board receive cost, an unknown record schema)
recomposes as before and names the input. Case C's property holds one level
up: an answer composed over other inputs is never taken (case T,
mutation-tested).

Found by the comparison, which is a reason to compare rather than trust: on the
west road the producer's `--workspace` was EMPTY, because the generated app
never calls `nano_ros_workspace()` and the fallback (`_ws_root`) lands in
`build/<coord>/`. So issue 1694's probe-sidecar floor ran in stage 3.5 and not
in the configure. The bytes happened to agree on `demo_bringup`. The producer
now takes the workspace from the bringup's `<ws>/src/<pkg>` layout.

Measured, `demo_bringup:zephyr`, clean dir: 0 `Re-running CMake`, producer read
the resolve, fragment and JSON byte-identical, 13 delivered-knob lines identical
to W6, `check-knob-delivery` green, two incremental rebuilds 0.

**Still open here: item (3)'s question is moot for this road, and the
deletion is W8.** A bare `west build` of `examples/zephyr/cpp/talker` (no
resolve) still takes 1 re-configure with two arms (bounds, entity), measured
the same day. The fixed point is what makes that configure deliver the right
numbers, so it cannot be deleted until such configures are seeded too.

# UPDATE 2026-10-23 — the entity arm came back, and is gone again

phase-439 W5 re-measured `demo_bringup:zephyr` on 2026-10-22: 1 `Re-running
CMake`, and the ENTITY arm was firing again beside the bounds arm (and a third,
the sizing descriptor's, which the 09-10 table below did not list). `diff`
between `resolved.cmake` and `nros/entity_inventory.cmake` was five lines:
phase-461 W3's inbox join, which `cmake/NanoRosEntityInventory.cmake` APPENDED
to the fragment after the CLI wrote it. The guard test below compares two Rust
composers, so a cmake-side append was outside its reach.

The appendix had no reader left (issue 1649 moved both inbox sizes to the sizing
descriptor), so the join now publishes into scope only, and
`tests/cmake-entity-inventory-tests.sh` case I asserts the fragment stays
byte-identical to the CLI's text. Re-measured: still 1 pass (the bounds and
descriptor arms fire), entity arm gone, `diff` empty, delivered knobs identical,
`check-knob-delivery` green.

# UPDATE 2026-09-10 — item (3) is landed and MEASURED, on a real Zephyr image

The entity link is closed and the numbers below are from a real build, not from
a five-line probe. What changed in this issue's own analysis is recorded first,
because the correction matters more than the fix.

**The composers differed by TWO renderings, not one.** This issue said the whole
difference was `NROS_ENTITY_INVENTORY_SOURCE`, on the strength of a unit test in
which both composers happened to carry the same `pkg`. On
`demo_bringup:zephyr` (`examples/workspaces/cpp`, `native_sim/native/64`,
Zephyr 3.7) the seed and the producer differed in that line AND in the
per-component provenance line's PACKAGE —

    -#   /listener::listener = 1 entities, 1 slots      (stage 3.5, model alone)
    +#   listener_pkg::listener = 1 entities, 1 slots   (the merged producer)

because `EntityInventory::from_model` states the node FQN as the package (it
names nodes, not ament packages) while the merge keeps the declaration's. Every
NUMBER was identical, as this issue predicted. So the CLASS is
"composer-dependent content in a hashed file", not one field, and the fix and its
test are written against the class: `to_cmake` emits neither, the per-component
rows are sorted on the rendered line, and
`resolve::tests::stage_3_5_and_the_mid_configure_producer_agree_byte_for_byte`
asserts whole-string equality between the two production composers — with the
model side built the way `from_model` builds it, so the second half is actually
covered. Mutation-tested: restoring either rendering reds it.

**The pass count, measured three ways on a clean build dir each time:**

| run | resolve seed | `Re-running CMake` | arms in pass 1 |
| --- | --- | --- | --- |
| no resolve phase (control) | — | 2 | bounds, entity |
| before this fix | yes | 1 | bounds, entity |
| after this fix | yes | **1** | **bounds only** |

Two findings in that table. The seed ALREADY removed one pass on a real image
(2 → 1) before this wave — phase-439 W2 declined to claim it and was right to,
but it was true. And **this fix removes an ARM, not a PASS**: both arms fire in
the same pass 1, so one re-configure discharges both, and the count only moves
when the LAST arm goes. `diff` between `resolved.cmake` and the producer's
`nros/entity_inventory.cmake` is now empty where it had two hunks.

Control, as phase-392 W5 requires: `python3 scripts/check-knob-delivery.py
<build-dir>` green on BOTH sides; the eight knob values the build reports
resolving identical across all three runs; the image's `zephyr/.config`
byte-identical before and after.

**What is still open here:** items (1) and (2) — making the producer READ the
resolve, which needs stage 3.5 to gain the refusal guard the producer's
`nros-metadata.json` component set gives it. That is unchanged by this wave, and
it is still gated on not trading a correct refusal for a saved pass. The
message-bound half and the deletion moved to **issue 1252**, which carries the
acceptance and the reproduction.

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

## Not measured here, deliberately — SUPERSEDED, see the UPDATE at the top

*(The paragraph below was written on a host with no `zephyr-workspace`. A later
host had one, and the update at the top of this file carries the real numbers.)*

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
