---
id: 147
title: "Fixture staleness is only enforced under `just test-all`, not at the resolver — a bare `cargo nextest` silently runs stale plain-example binaries"
status: open
type: tech-debt
area: testing
related: [132, 146, 129, 140, phase-278]
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

## Fix direction — resolver-level detect-only dep-info probe (phased)

Move staleness to the RESOLVER (so it works under any launcher, incl. bare
`cargo nextest`) with a DETECT-ONLY probe that reads the toolchain's own
recorded dependency graph and compares mtimes — it never invokes the compiler,
so it structurally cannot rebuild a final binary (the maintainer's constraint:
a build PROBE is fine, building a final binary at test time is not).

Preferred over both (a) a content signature over the example dir (blind to
shared-crate deps) and (b) invoking `cargo build`/`cmake --build` as the probe
(a no-op when fresh, but rebuilds the final binary when STALE — the forbidden
case, and precisely when the resolver cares; there is no stable
`cargo build --dry-run`).

Mechanism:
- **Rust** — cargo writes `<target>/<profile>/<binary>.d`, a make-style file
  listing EVERY source input incl. shared crates (`nros-core`, `nros-macros`,
  the generated msg crates — ~186 files for the listener). The resolver parses
  it and flags stale if any listed source's mtime is newer than the binary.
  Pure `stat()`, no process spawn, covers deps the content signature cannot.
- **C/C++** — same on the per-object `.d` files gcc/clang emit under `-MD`
  (which ninja already generates); read + stat, do NOT invoke ninja (sidesteps
  the Corrosion always-run step that broke `ninja -n` in the existing
  preflight).

Phasing:
- **P1 — native rust single-node (the #146 family): highest value, smallest
  surface.** Add `require_prebuilt_binary_fresh(&path)` that parses the sibling
  `<binary>.d` + mtime-checks; route `build_example` / `build_example_rmw`
  through it (talker/listener/service/action + the interop set).
- **P2 — native C/C++ (`build_example_cmake_rmw`) + `bins/`** via the object
  `.d` files.
- **P3 — zephyr-workspace entries.** Independent quick win: the 9
  `build_zephyr_workspace_*` use bare `require_prebuilt_binary` where the
  native/cmake workspace family is guarded — either switch them to the existing
  `require_prebuilt_workspace_binary` (+ have `zephyr-fixture-leaves.sh` write
  the inputsig) or give them the `.d` probe (the west build emits `.d` files
  too). Closes the clearest oversight.
- **Non-cargo/embedded (qemu/west/idf)**: lower priority; existence +
  `.compile-ok` is tolerable since those rebuild wholesale per lane.

Caveat (accepted): mtime comparison flags stale after a `git checkout` that
resets source mtimes even when content is unchanged — but that errs toward a
(correct) rebuild, never toward silently running a stale binary. This is
exactly how cargo's own fingerprint behaves.

Cheaper stopgap (not a substitute): wrap the common nextest entry in a `just`
recipe that always runs `_check-fixtures-stale` first, and document that bare
`cargo nextest` skips the guard.

## Detection today (until fixed)

A test whose subject clearly should deliver returns 0 / wrong type — check the
on-disk fixture's baked keyexpr/type (`strings <binary> | grep std_msgs`)
against current source before assuming a product bug.
