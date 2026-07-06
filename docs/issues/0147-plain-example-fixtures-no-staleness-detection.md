---
id: 147
title: "Fixture staleness is only enforced under `just test-all`, not at the resolver — a bare `cargo nextest` silently runs stale plain-example binaries"
status: open
type: tech-debt
area: testing
related: [132, 146, 129, 140]
---

## Summary

Staleness detection for plain (single-node) example fixtures EXISTS, but it
lives in a `just test-all` PREFLIGHT, not in the fixture resolver — so any run
that doesn't go through that recipe (a bare `cargo nextest run`, the normal
dev/debug loop, and exactly how #146 surfaced) uses whatever binary is on disk
with zero staleness check. `require_prebuilt_binary`
(`nros-tests/src/fixtures/binaries/mod.rs:391`) is a pure existence check; the
~120 plain-example resolvers that funnel through `build_example` /
`build_example_rmw` / `build_example_cmake_rmw` all use it. The result: a green
LOCAL run (via nextest) proves nothing about current source unless fixtures
were rebuilt, and nothing at the resolver enforces that.

## Accurate current state (2026-07-06 audit)

Three tiers of fixtures, three different staleness stories:

1. **Workspace fixtures (~70 resolvers)** — REAL guard, resolver-enforced.
   `require_prebuilt_workspace_binary` recomputes a content signature
   (`workspace-fixture-signature.sh`, sha256 over the workspace dir's source +
   the manifest record, `target*`/`build*`/`generated` pruned) and HARD-FAILS
   "… is stale". Keyed by `fixture_id` so multi-variant dirs get distinct
   `.nros-workspace-fixture.<id>.inputsig` stamps. This is the template.

2. **Plain single-node fixtures (243 `[[fixture]]` rows)** — staleness ONLY at
   the `just test-all` preflight `_check-fixtures-stale`
   (`scripts/check-fixtures-stale.sh`), and only WARN + self-heal:
   - rust: `scripts/test/rust-fixture-stale.sh` runs
     `cargo build --message-format=json` and treats `"fresh":false` as stale →
     rebuilds, warns. Uses cargo's OWN fingerprint; nothing is stored next to
     the binary.
   - C/C++: `scripts/test/cmake-fixture-stale.sh` runs `cmake --build` and
     greps for real compile/link → rebuilds, warns.
   Neither is enforced at the resolver, so a direct `cargo nextest` skips both.
   (The `.nros-fixture.inputsig` named in the justfile:1029 comment no longer
   exists — the mechanism moved to cargo/cmake incremental self-heal; the
   comment is stale.)

3. **Zephyr-workspace entries (9 resolvers) + all non-cargo (west/qemu/idf/
   compile-check)** — NO staleness at all, not even a preflight. The 9
   `build_zephyr_workspace_*` fns are conceptually workspace fixtures but use
   the BARE `require_prebuilt_binary` (an oversight vs the native/cmake
   workspace family). west/qemu/idf resolvers assert only a `.compile-ok` /
   existence marker.

## Why it keeps biting

- **#146** — a stale `native/rust/listener` (pre-W4 `Int32_` vs current
  `String_`) gave 0-received "ros2→nano broken" under bare nextest, before the
  real QoS defect was reachable. The talker fixture was stale too.
- **#129 / #140** — "stale June prebuilts masked months of lane rot" in both
  root-cause writeups.
- Every debugging session that runs `cargo nextest` directly (to iterate faster
  than `just test-all`) is exposed.

## Fix direction — resolver-level content signatures (phased)

The robust fix is to move staleness to the RESOLVER (works under any launcher),
using content signatures (NOT a cargo/cmake build probe — that would compile at
test time, violating the "no compilation inside tests" rule). Generalize the
workspace template:

- **P1 — native rust single-node (the #146 family): highest value, smallest
  surface.** Give `[[fixture]]` records an emitted `id` (they already carry one
  in `fixtures.toml`; `fixtures-manifest.py` must include it in the plain
  record). `fixtures-build.sh` writes `.nros-fixture.<id>.inputsig` in the
  fixture's target dir after building; a new `require_prebuilt_binary_checked(
  id, path)` recomputes + hard-fails on mismatch. Migrate `build_example` /
  `build_example_rmw` first (talker/listener/service/action + the interop set).
- **P2 — native C/C++ + `bins/`.** Same stamp, C/C++ via
  `build_example_cmake_rmw`.
- **P3 — zephyr-workspace entries.** Trivial: switch the 9
  `build_zephyr_workspace_*` to `require_prebuilt_workspace_binary` and have the
  zephyr leaf builder (`zephyr-fixture-leaves.sh`) write the workspace inputsig
  — closes the clearest oversight.
- **Non-cargo/embedded (qemu/west/idf)**: lower priority; existence +
  `.compile-ok` is tolerable since those rebuild wholesale per lane. Revisit if
  they ever mask a bug.

Variant note: key stamps by `fixture_id`, NOT dir — a single example dir builds
N variant binaries in sibling `target-*` dirs (per-RMW, tls, safety,
zero-copy), so a dir-level signature would collide or thrash. The signature
must fold in the manifest record (features/env/target-dir), exactly as the
workspace signature does.

Scope note: like the workspace inputsig, this signs the example's OWN dir, not
its shared-crate deps — a pure `nros-core` edit won't invalidate it (accepted;
CI rebuilds fixtures wholesale). Target = the common "edited the example /
prebuilt is months old" drift.

Cheaper stopgap (not a substitute): wrap the common nextest entry in a `just`
recipe that always runs `_check-fixtures-stale` first, and document that bare
`cargo nextest` skips the guard.

## Detection today (until fixed)

A test whose subject clearly should deliver returns 0 / wrong type — check the
on-disk fixture's baked keyexpr/type (`strings <binary> | grep std_msgs`)
against current source before assuming a product bug.
