---
id: 477
title: "`nuttx-c-talker-zenoh` overflows ROM by 448776 bytes — a stale board artifact, not a code regression"
status: resolved
type: bug
area: nuttx
related: [phase-334, phase-340, phase-341, phase-337, issue-0466]
---

## Resolution — there was never a size regression

**Root cause: a rebuild edge that only watches the path that WON.**

`nros-board-common`'s `snapshot_root` / `snapshot_or_tree` prefer the per-arch
export snapshot (`$NUTTX_DIR/nros-nuttx-export-<arch>/`) and fall back to the
live `staging/` tree. Both emitted `cargo:rerun-if-changed` on the **resolved**
path only. So a build that ran before the ARM export existed resolved to
`staging/`, recorded an edge on `staging/`, and then — when the snapshot later
appeared — saw nothing change and **kept its artifact forever**.

A `lane=all` sweep builds RISC-V and ARM. The board crate's cached artifact was
staged against the wrong tree, and the resulting link overflowed ROM by 448776
bytes with nothing wrong in any source file.

### Proof

The bisect never needed a single step. Both ends were validated first, and once
the confounder was removed from the probe — provision the ARM export, then
`cargo clean -p nros-board-nuttx-qemu -p nros-nuttx-ffi` — **HEAD linked GOOD
with exactly the code that had been failing**:

| | image |
| --- | --- |
| last good artifact (Aug 6 00:35) | 687112 bytes |
| HEAD, after a clean rebuild | 691468 bytes |

**+0.6 % over 36 hours** — ordinary growth. The "doubling" was never real.
Confirmed through the real build path: `just nuttx build-fixtures-arm` now
completes the whole ARM lane with **zero** ROM overflows and zero failures.

This is CLAUDE.md's own warning arriving one step earlier than usual — "when a
bisect's first-bad is implausible for the symptom, the test tracked a
confounder". Here the confounder surfaced while *validating the endpoints*,
which is the cheapest place to catch it and the reason both ends must always be
validated before the first step.

### Fix

Both resolvers now emit `cargo:rerun-if-changed` on the preferred path **even on
the losing branch**. A `rerun-if-changed` naming a path cargo cannot find still
registers — the artifact is invalidated when that path later appears, which is
exactly the transition that was invisible. One edge in the shared helper, not a
second spelling per caller.

### What this says about the class

The `.fixtures-built` stamp and the staleness probes track fixture *sources*.
Nothing tracked the *kernel export* a board artifact was staged against, so a
lane switch left a silently mis-staged artifact that no gate could see. Worth
checking whether the other RTOS board crates resolve any input by
prefer-then-fallback with the edge on the winner — same shape, same failure.

### Still open, separately

The `freertos` `Error 101` seen in the same sweep is a different failure and is
NOT diagnosed here.

---

_Original investigation below, kept because the ruled-out table is what made the
stale-artifact explanation the only one left._

## Symptom

`just build-test-fixtures lane=all` dies in the `nuttx` stage:

```
ld: nros_nuttx_ffi-… section `.text' will not fit in region `ROM'
ld: region `ROM' overflowed by 448776 bytes
error: could not compile `nros-nuttx-ffi` (bin "nros-nuttx-ffi")
make[1]: *** [.../nuttx-c-zenoh-all-….mk:10: fixture-0000] Error 101
```

The failing row is `examples/qemu-arm-nuttx/c/talker` (zenoh). `native`, `qemu`
and `threadx_linux` pass in the same run; `freertos` fails separately (rc=101,
not diagnosed here).

## Why it matters beyond one fixture

It is the gate on a tier-2 sweep, and **three phases are waiting on that sweep**:

* **phase-341** — status says archive only once a tier-2 sweep confirms the 39
  migrated leaf configs across the matrix.
* **phase-340 W2.a** — its next step MOVES 85+ rows' artifacts and needs a
  known-good lane baseline first.
* **phase-334 W2.c** — follows 340's path move.

So a single embedded row's size regression is currently holding the whole
build-cache/identity program.

## What it is NOT

**Not phase-340 W5.b/c.** That commit (`aa8be9199`) removed the `nros`
build-dependency from `nros-c` and `nros-cpp`, and it is the most recent change
to those crates, so it was the obvious suspect. Tested directly: restored both
manifests from `aa8be9199~1`, rebuilt the same fixture, and the link fails with
**the identical byte count**:

```
sweep run        overflowed by 448776 bytes
edge restored    overflowed by 448776 bytes
```

Identical to the byte in both runs — deterministic, and unaffected by that
change. A build-dependency changes which crates the BUILD graph compiles, not
what the product links, so this matches expectation; it is recorded because the
next person will suspect the same commit.

**Not a leaf this work touched.** `examples/qemu-arm-nuttx/c/talker` has no
`.cargo/config.toml` at all and was not among phase-341 W3's 39 migrated leaves.
The NuttX board descriptor carries no `--gc-sections` (phase-341 W1 hoisted that
into the mps2/threadx/esp32 descriptors only).

**Probably not issue 0460.** That fix (`3f72baa8f`) made Zephyr Kconfig knobs
reach the Rust lane's crate build, and by its own account images previously
"happened to be identical for the wrong reason" — a knob resolving larger would
grow an image. But its fallback reads `DOTCONFIG`, a Zephyr variable, which a
NuttX build does not set. Worth re-checking if the lead below goes nowhere.

## Provenance: a 36-hour window, established from a museum binary

**This IS a regression, and the window is narrow.** A successfully linked
artifact sits at the exact output path:

```
687112 bytes  Aug 6 00:35  examples/qemu-arm-nuttx/c/talker/build-zenoh/
                           cargo-target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi
```

So this row linked fine, and the image has since grown past a ~1 MB ROM by
448776 bytes — it roughly DOUBLED. Window: **2026-08-06 00:35 → now**, about 24
commits (`git log --since="2026-08-06 00:00"` over `packages/api/nros-{c,cpp}`,
`packages/core`, `packages/rmw/zenoh`, the board crate, and the two profile
files).

Note this binary is exactly the "museum binary" CLAUDE.md warns about: the row
was passing sweeps on a stale artifact. Its mtime is what made the window
findable, so do not delete it before bisecting.

### Ruled out, each by direct test

| Suspect | Verdict | Evidence |
| --- | --- | --- |
| phase-340 W5.b/c (`aa8be9199`) | **No** | Restored both manifests from `aa8be9199~1`; identical 448776-byte count |
| phase-341 W3 leaf-config migration | **No** | The leaf has no `.cargo/config.toml`; the FFI crate's is unchanged since 2026-08-04 and has no `include` |
| The ambient profile / phase-336 split | **No** | The C lane hardcodes `-DCMAKE_BUILD_TYPE=Release` → `cargo build --release`, and `nros-nuttx-ffi` is its OWN workspace root with its own `[profile.release]` (`opt-level = "s"`, `lto = true`). `NROS_CARGO_PROFILE` is a no-op here — an override attempt reproduced the identical failure |
| `[profile.dev] incremental = false` (`78375a2d5`) | **No** | Same reason: this builds at `release` |
| `libnros_cpp` in a C link (the old lead) | **No** | Not new — `extern crate nros_cpp` predates the phase-337 W3 move; it was in the pre-move crate too |
| The `#0436` bridge ABI in `libnros_cpp.a` (`687848b4d`, 00:57 — the best-fitting suspect on timing) | **No** | `bridge` is off by default, the FFI crate does not request it, and NO `nros-bridge` artifact exists in the build's `deps/`. Its presence in the leaf lock is just an optional-dep record |

### Next step

Bisect the ~24-commit window; each step is ~5 min and deterministic, so ~5 steps
settle it. `687848b4d` was the strongest a-priori fit and is refuted, so do not
skip ahead — measure.

```sh
source ./activate.sh
NROS_FIXTURE_ID=nuttx-c-talker-zenoh just nuttx build-fixtures-arm 2>&1 | grep -c overflowed
```

Caveat learned here: that invocation exits **0** even when the row fails
(`FAILED: [code=101]` appears in the log but does not propagate), and it ignores
`NROS_FIXTURE_ID` for single-node rows — "does not narrow single-node fixtures".
Grep the log for `overflowed`; do not trust the exit status.

## Does this actually block `ci-matrix`?

`ci-matrix` is tier 2 and gates on `lane=tier2`, not `lane=all` — the lane is
COMPUTED (`scripts/build/fixture-lane.sh`), 1-wise, so `nuttx`/`c`/`zenoh` each
appear once but not necessarily as this row. If tier 2 does not select it, the
tier-2 sweep can run while this stays open. Verify by building `lane=tier2`
rather than by reading the cover.

## Reproduce

```sh
source ./activate.sh
NROS_FIXTURE_ID=nuttx-c-talker-zenoh just nuttx build-fixtures-arm
```

~5 minutes; fails deterministically.
