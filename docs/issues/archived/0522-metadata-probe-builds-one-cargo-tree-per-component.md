---
id: 522
title: "The metadata probe builds one full cargo tree per component — cargo harness FIXED, the cmake/corrosion probe (14 trees, 50.3 GiB) remains"
status: resolved
type: tech-debt
area: build
related: [phase-340, issue-0488, rfc-0070]
---

## Symptom

Nothing fails. This is disk and wall-clock, found while re-measuring phase-340
W7 on the post-change tree.

`nros metadata --build` renders a small harness crate per component and runs it
to record what that component registers. The harness builds into the component's
OWN directory:

```rust
// packages/cli/nros-cli-core/src/orchestration/metadata_build.rs:337
let target_dir = o.harness_dir.join("target");
```

`harness_dir` is `<leaf>/build/nros-metadata/metadata-probe/<component>/`, so
every component gets a private cargo target dir holding a full host build of
itself and its dependency graph.

Measured on this tree (2026-08-12):

| class of per-leaf `target*` dir under `examples/` | dirs | size |
| --- | --- | --- |
| **metadata probe** | **108** | **82.4 GiB** |
| example leaves (phase-340 residue) | 356 | 28.9 GiB |

The probe is now the LARGEST per-leaf duplication class in the tree — roughly
three times what phase-340 has left behind in the population it was created to
fix. The eight biggest single `target*` dirs under `examples/` are all probe
trees, 2.0–2.3 GiB each.

## It is the phase-340 thesis, on a build path phase-340 never touched

Counting `libnros_core-<hash>.rlib` inside the probe trees — that hash is
cargo's `-C metadata`, i.e. cargo's own judgement that two builds are
interchangeable:

```
162 probe trees hold nros_core
    312 rlibs, 16 distinct identities
     62  a0c38a7bcc90ab36
     56  7bf3fe978cb14ee5
     39  6203506aceacbcd6
     …
```

**296 of 312 are literal repeats of a compilation that already exists.** Sixteen
distinct compilations are doing the work of 312 — the same shape phase-340
measured on the fixture lane (45 of 106 `nros_core` rlibs identical) and fixed
there with one shared `--target-dir` per group.

## Why the earlier census missed it

Issue 0488 inventoried the second-build-path residue and lists two classes, both
real. Neither is this one, because 0488 swept `just/` and `scripts/` for
`cargo build` call sites — and this invocation is emitted by the CLI, in Rust,
from cmake's configure step. A sweep keyed on the caller's LANGUAGE will keep
missing it.

That is the issue-0196 rule again: the gate (`check-example-leaf-target-dirs.py`)
covers `examples/**/target/`, and these dirs are `examples/**/build/nros-metadata/
metadata-probe/*/target/`, which the glob does not reach.

## Direction, not a decided fix

The probe's requirements are narrow and stated in `metadata_build.rs`: it needs
the component's `.cargo/config.toml` in scope (hence `current_dir(&harness_dir)`,
phase-307 W1), a host `--target`, and `panic = "abort"` (issue 0288). None of
those requires a PRIVATE target dir.

Candidates, cheapest first:

1. **One probe target dir per (host-triple, workspace)** under
   `$NROS_BUILD_ROOT` — `nros_build_dir` already derives such paths (RFC-0070
   R1/R3). The 16 identities suggest the natural key is close to "the workspace's
   patch set", not the component.
2. **Reuse the fixture group dir** where the component belongs to a manifest row.
   Tempting and probably wrong: the probe builds for the HOST while the row may
   build for a board, and mixing them in one dir is what phase-340 W1 measured as
   an artifact-name collision.
3. Leave the location alone and let cargo dedupe via sccache — refuted for this
   class by phase-340 W1: `--target <host>` and the group's flags are different
   `-C metadata` identities and share nothing, measured 0 hits / 62 misses.

Acceptance for whichever lands: the probe-tree count and total bytes above,
re-measured, plus a `find`-based gate whose glob actually reaches
`build/nros-metadata/**/target` (verified against a replica, per issue 0196).

## Note on ownership

Not phase-340's to fix — that phase owns the fixture lane and is closing. Filed
so the number is recorded rather than re-swept: the disk story phase-340 wrote
(`examples/` 402 GiB, one talker leaf holding 7.4 GiB across five target dirs)
is no longer the dominant story on this tree, and this is what replaced it.

## Status (2026-08-12) — the cargo harness half is FIXED

`metadata_build.rs` now resolves a SHARED target dir:
`$NROS_BUILD_ROOT/metadata-probe`, else `<nano-ros workspace>/build/metadata-probe`
(the same `<repo>/build` rule the shell's `nros_build_root` uses, reached through
the checkout the harness already points its path deps at), else a
`.shared-target` beside the harness dirs when the nano-ros workspace is a
read-only installed SDK.

Measured on `examples/workspaces/rust` (6 components), wiped and re-probed:

| | before | after |
| --- | --- | --- |
| probe target dirs | 6 | **1 shared** |
| bytes | 3.2 GiB | **483 MiB** (6.8x) |
| `nros_core` rlibs / identities | 12 / 2 | **2 / 2** |

**It is also faster, which was not the goal.** Cold `build-test-fixtures
lane=native`, same method as phase-340 W7:

| | wall | native stage |
| --- | --- | --- |
| pre-fix steady state (W7) | 581 s | 342 s |
| first build after the fix | 722 s | 392 s |
| steady state after the fix | **461 s** | **295 s** |

The middle row is the one-off cost of populating the shared dir; steady state is
21 % faster on wall clock than before the change, because the probe work that
used to be repeated per component now happens once.

Two parts of the fix are worth keeping in mind before touching this code again:

* **Unique artifact names are part of it, not tidy-up.** Every harness was
  package `nros-metadata-probe` with bin `probe`. Fine with a private dir, fatal
  when shared: cargo does not hash the final artifact name, so two components
  write the same `<target>/<host>/<profile>/probe` and a `cargo run` can execute
  the other component's binary. Phase-340 W1 measured that exact failure on the
  fixture lane. Package and bin are slugged per component now.
* **The DEFAULT mattered more than the override.** The first version keyed only
  on `$NROS_BUILD_ROOT`, which is a function-local default in `build-root.sh`
  and not an exported variable — so a plain `nros metadata --build`, and every
  cmake configure that shells the CLI, sees it unset. That version would have
  left all 108 dirs exactly as they were.

## Remaining — a SECOND producer, and the residue

**Still open, and the reason this issue stays open:** the corrosion-driven
`metadata-probe-cmake` path is a different producer with its own trees —
**14 trees, 50.26 GiB** measured after the cargo fix landed. Its target dir is
chosen by cmake/corrosion rather than by `metadata_build.rs`, so nothing above
touches it. That is issue 0493's territory (two workspace roots, one corrosion
target dir) and wants to be solved with it rather than beside it.

The original 82.4 GiB census counted BOTH producers, because it classified by
path (`/nros-metadata/`). Split after the fix: the cargo harness class is gone,
the cmake class is the 50.3 GiB above, and **75.7 GiB across 94 dirs is pre-fix
residue** — output nothing writes any more. Deleting it belongs to issue 0488's
cleanup, not to a bug fix.


## The cmake half, re-checked 2026-08-13 — it is NOT the same defect

I called `metadata-probe-cmake` "a second producer" of this issue's defect. That
was too strong, and the distinction matters for what to do about it.

**Location is already right.** The cargo harness wrote one full target dir per
COMPONENT, inside the component's own leaf. The cmake probe is per WORKSPACE
(`probe_dir_for_workspace`, phase-313 — "was per component, which is what made
every probe rebuild the runtime") and lands in `<ws>/build/nros-metadata/`, i.e.
the workspace's own build dir. That is where build output belongs, and
`check-workspace-build-output` exempts it deliberately.

**The bytes are corrosion's, not cmake's.** Measured on
`examples/workspaces/safety`: 4.8 GiB total, of which `build/cargo` is **4.7
GiB**. Same ratio phase-344 measured for cmake dirs generally (83.2 %). So the
14 trees / 50.26 GiB are 14 copies of the same Rust dependency graph, one per
workspace — a DUPLICATION problem, not a misplacement one.

**Corrosion 0.6.x changes what is possible here, and the pin moved this session
(v0.5.1 -> v0.6.1).** The target dir is:

```cmake
# Corrosion.cmake ~781 — "so if you build multiple workspaces … they won't
# collide if they use a common dependency"
string(SHA1 cargo_path_hash ${workspace_manifest_path})
cmake_path(APPEND CMAKE_BINARY_DIR ${build_dir} cargo "${cargo_folder_name}_${cargo_path_hash}" …)
```

Rooted at `CMAKE_BINARY_DIR`, then separated per WORKSPACE MANIFEST. Two facts
follow:

* Corrosion offers no knob to move the cargo dir on its own — the only lever is
  the consuming project's `CMAKE_BINARY_DIR`.
* Projects sharing one `CMAKE_BINARY_DIR` no longer collide, because 0.6 hashes
  by manifest path. That is precisely the collision issue 0493 recorded against
  Corrosion `< 0.6.0`, which shared one `cargo/build` across workspace roots and
  produced duplicate `#[no_mangle]` symbols.

So the direction is: give the cmake probe projects ONE shared
`CMAKE_BINARY_DIR` (under `$NROS_BUILD_ROOT`, `<root>/<kind>/<coordinate>`), and
let corrosion's own per-manifest hashing keep them apart. The probes all import
the same nano-ros manifest, so they should collapse to one cargo tree rather
than 14.

**Not done here, and the reason is 0493.** That issue is open and is about
exactly this sharing; doing it under this issue would be fixing 0493 without
its evidence. What this entry adds is the measurement (4.7 of 4.8 GiB is
corrosion), the mechanism (Corrosion.cmake:781), and the fact that the 0.6.1 pin
may have removed the objection.

Alternative worth pricing if that stalls: the probe build dir is REGENERABLE
scratch — nothing reads it after the sidecars are written — so deleting it on
success reclaims the 50 GiB at the cost of a cold probe on the next sync. That
trade needs a measurement of cold-probe time, which nobody has taken.


## The measurement is BLOCKED, and probing for it found why (2026-08-13)

Attempting the cold-vs-warm probe timing this issue asked for: warm
`nros sync examples/workspaces/safety` takes **30.9 s** and ENDS IN FAILURE —
the probe cannot build, because it asks `nros-c` for `metadata-mode`, a feature
only `nros-cpp` has (issue 0542).

So the 14 trees / 50.26 GiB are not a working cache whose value is being
weighed. They are the residue of probe builds that get as far as compiling the
runtime and then fail at the `nros-c` cargo step. Three of the four C/C++
components in that workspace have no sidecar producer at all.

Both of this issue's open questions wait on 0542:

* "is the probe tree worth keeping?" cannot be timed cold against warm until a
  probe can finish.
* the shared-`CMAKE_BINARY_DIR` direction would currently be sharing a broken
  build.


## The keep-or-delete measurement, taken 2026-08-13 (once 0542 unblocked it)

`examples/workspaces/c`, sidecars deleted before each run:

| | time |
| --- | --- |
| WARM — probe tree kept (4.8 GiB) | **6.0 s** |
| COLD — probe tree deleted | **23.2 s** |

The cache buys ~17 s per re-probe for 4.8 GiB, i.e. roughly **3.5 GiB per second
saved**, and it is only consulted when a sidecar is missing — which no lane
causes. On that ratio, deleting the probe tree after a successful probe looks
like the better default for CI and the worse one for a developer iterating on a
component's metadata.

That makes the remaining question a policy one rather than an unknown: keep it
warm locally, drop it in a lane. Which is a smaller decision than the
shared-`CMAKE_BINARY_DIR` direction above, and independent of it.


## Resolved 2026-08-13 — the cargo half moved, the cmake half is a knob

Both halves are now answered, and they needed different answers.

**Cargo harness (fixed 2026-08-12):** one shared target dir instead of one per
component. 6 dirs / 3.2 GiB / 12 rlibs -> 1 dir / 483 MiB / 2 rlibs on
`examples/workspaces/rust`, and cold `lane=native` got FASTER (581 s -> 461 s).

**Cmake probe (this):** its location was already right — per workspace, in the
workspace's own `build/` — so there was nothing to move. What it had was a cache
nobody had priced. Priced (`examples/workspaces/c`, sidecars deleted each run):

| | time |
| --- | --- |
| WARM — build tree kept | **6.0 s** |
| COLD — build tree deleted | **23.2 s** |

~17 s bought for 4.8 GiB, consulted only when a sidecar is MISSING — which no
lane causes. Good deal for a developer iterating on component metadata, bad one
for a build lane paying the disk on all 14 workspaces (50.26 GiB) to save time it
never spends.

So it is a knob rather than a decision imposed on everyone:
`NROS_METADATA_PROBE_CACHE=0` discards the tree, defaulting to KEEP, and the two
lane builders (`fixtures-build.sh`, `workspace-fixtures-build.sh`) set it.
Verified on `examples/workspaces/c`: **921 MiB kept by default, 40 KiB with the
knob**, 6 sidecars produced either way.

**Discarded only on FULL SUCCESS.** A failed probe's tree is the evidence — this
entire issue, plus 0542 and 0543, were diagnosed by reading exactly those
`CMakeFiles` logs. Deleting it on failure would trade 4.8 GiB for the ability to
explain what went wrong.

The shared-`CMAKE_BINARY_DIR` direction recorded above stays with issue 0493,
where the corrosion evidence lives. It is now a smaller prize than it looked: on
a lane the tree does not survive the build at all.
