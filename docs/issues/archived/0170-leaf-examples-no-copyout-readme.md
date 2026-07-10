---
id: 170
title: "Leaf examples ship no README — copied-out project carries zero instructions, contradicting the RFC-0026 copy-out contract"
status: resolved
type: tech-debt
area: docs
related: [rfc-0026, phase-277]
resolved_in: "c921fdc87"
---

## Problem

Examples are framed as **standalone copy-out projects** (RFC-0026, phase-277),
but the canonical leaves ship no `README.md`:

- `examples/native/rust/talker/`, `.../listener/` — the highest-traffic first
  examples — have none.
- Same for every `native/c/*`, `native/cpp/*` (except `custom-platform` and
  `custom-transport-loopback`), `qemu-*/…/{talker,listener}`, and `zephyr/…`
  leaves.
- Run instructions live only in the parent `examples/<platform>/README.md`
  coverage tables — a copied-out `talker/` directory carries nothing.
- `examples/README.md:137` claims "each workspace has its own README"; true for
  `workspaces/*` and `templates/*`, false for the canonical leaves.

Adjacent doc rot in the same file: `examples/README.md:218,260` still describe
the Phase-140 `add_subdirectory` consumption era + `build/zenohd/zenohd`, while
the current C copy-out contract (lines 31-37) is the `-DNANO_ROS_ROOT` guard —
two eras of instructions coexist in one file.

## Fix direction

Template-generate a minimal per-leaf README (3-5 lines: prerequisites /
`source activate.sh`, router start, build+run command, where the copy-out knobs
live). Start with `native/rust/{talker,listener}`. Fold generation into the
example-scaffolding path so new leaves get one automatically. Purge the
Phase-140-era paragraphs from `examples/README.md`.

## Resolution (2026-07-09)

**Every canonical leaf now ships a README** — 176 leaves
(`examples/<platform>/<language>/<case>` carrying a `package.xml`), 174
generated, 2 hand-written pages preserved untouched
(`native/c/{custom-platform,custom-transport-loopback}`; the zephyr/px4 nested
leaves already had their own).

- `scripts/docs/gen-example-readmes.py` renders each page from facts read off
  the leaf, so nothing is invented: Rust vs CMake build block; native vs
  cross-built run block; and the *actual* knob file — it distinguishes
  `[package.metadata.nros.deploy.<target>]`, the leaf's real `rmw-*` cargo
  features, `nano_ros_deploy(…)` (49 leaves) and `nano_ros_entry(…)` (27
  leaves). An early draft that assumed `nano_ros_deploy` everywhere would have
  lied about every native C/C++ leaf. It never overwrites an existing README.
- **Links are absolute GitHub URLs on purpose.** A copied-out directory has no
  repo above it, so relative links back into the checkout would 404 — the exact
  failure this issue is about.
- Gated by `example_shape::every_canonical_leaf_has_readme` (verified it fails
  when a README is removed, not just that it passes).
- `examples/README.md`: the Phase-140 `add_subdirectory(<repo>)` sentence
  (which contradicted the `-DNANO_ROS_ROOT` copy-out contract 190 lines above
  it) is replaced by the current guard; the leaf-README contract is documented.

**Verified end to end**, not just asserted: copying `native/rust/talker` and
`native/c/talker` out of the tree and running the exact README commands both
build clean (`nros sync` + `cargo build` → `talker`; `cmake
-DNANO_ROS_ROOT=… && cmake --build` → `c_talker`).

Not done here: `just native zenohd` was already repaired by #168 (shared
resolver), so the zenohd half of this issue's "adjacent rot" was stale on
arrival.
