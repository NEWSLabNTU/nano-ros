# Phase 340 — Build-artifact reuse: compile each identity once

**Status (2026-08-12). COMPLETE.** Every work item is closed. W2.d and W5.d, the
last two open, landed 2026-08-11/12 — W2.d via [issue
0517](../issues/archived/0517-resolver-spells-variant-identity-as-a-leaf-path.md),
which found the framing wrong before it found the fix: the `target_dir` column
was never a location (all 124 cargo rows build into `build/fixtures-cargo/<slug>`)
but a KEY encoded as a path, so making the lane check take the ROW deleted the
need for it and nothing moved on disk. W7 re-measured both axes the same week:
**wall 6794 s -> 581 s (11.7x)** at the same 72 fixtures, budgets verified AT the
truth rather than lowered
([data](data/phase-340-w7-remeasure.md)). P1 and P2 are DONE, P3 is
[phase-344](phase-344-cmake-cache-relocation.md) (measurement complete, no path
moved, deliberately), and **P4 is WITHDRAWN, not blocked** — the per-leaf
`.gitignore` files it would delete are the copy-out contract, which is why
RFC-0070 R1 was amended to be scoped by context.

**What this phase did NOT finish, and where it went instead:** the re-measure
found the dominant per-leaf duplication class is no longer the fixture lane's.
The cmake metadata probe holds **108 dirs / 82.4 GiB** — 162 trees, 312
`libnros_core` rlibs, 16 distinct identities — against 28.9 GiB of leaf residue.
That is this phase's thesis on a build path this phase never touched, filed as
[issue 0522](../issues/0522-metadata-probe-builds-one-cargo-tree-per-component.md).
Residue cleanup is [issue 0488](../issues/0488-second-build-path-residual-per-leaf-cargo-dirs.md).

**Original status (2026-08-10).** IN PROGRESS. P1 and P2 are DONE, P3 is split out to
[phase-344](phase-344-cmake-cache-relocation.md), P4 is blocked on P3, and two
items remain open (W2.d, W5.d). **[phase-334](archived/phase-334-build-cache-layout.md)
is COMPLETE and ARCHIVED** — its W2.c was landed and withdrawn the same day (the
391 per-leaf `.gitignore` files are the copy-out contract, not sprawl), and
RFC-0070 is what it actually produced. A second investigation,
**[issue 0493](../issues/0493-two-workspace-roots-share-one-corrosion-target-dir-duplicate-no-mangle-symbols.md),
reached this phase's multi-identity finding from the LINK side** — see
"Relationship to issue 0493" below before working the identity items, because
the two accounts disagree on one measurable fact.

**Original status (2026-08-07).** IN PROGRESS, and **CONSOLIDATED with**
phase-334: the two are one program on two axes
and their items had begun to overlap (334 W3.a was this phase's W2, W3.b was its
W3). This phase owns WHAT gets compiled and how often; 334 owns WHERE the result
lives. The ordered plan for both is "Work order (both phases)" below — the
authoritative copy. Measurement-first, and the measurements
are the deliverable so far: W1 answered (`incremental` for the shared profile,
single-leaf and lane-scale results recorded), W5.a measured (the biggest
duplicate is INSIDE one invocation — the build-dep graph). W4 landed 2026-08-07:
the measured numbers are now a gate (`check-artifact-identity-budget`), so they
cannot regrow quietly while the rest is in flight. **W3 landed 2026-08-08 for
the cmake lane**: explicit-always, decided by measurement (zero sccache sharing
across the two spellings; zero extra units from normalising), with the three
generators that could emit cargo's implicit host spelling collapsed onto one
resolver and gated. Its cargo-LEAF half is deferred into W2's path pass.

**W2's MECHANISM is decided as of 2026-08-08, by measurement, and it is not the
one W2.b proposed.** Three arms were timed against each other at the real group
size: separate dirs (status quo), one shared `--target-dir` with N invocations,
and the generated umbrella workspace. The shared dir gets **100 % of the disk
win** — 9.70 GiB → 455 MiB over 37 leaves, `deps/` dedup 27.9:1 → 1.0:1,
identical to the umbrella's — and is **never slower** than the status quo, which
refutes F3 and the phase-334 W1.a verdict that cites it. So W2 ships the
mechanism that already exists rather than a generated-manifest subsystem. **No
paths have moved yet**: `linux`'s first migration blocker (artifact-name
collisions) is fixed and gated; the second (the Rust resolver's group key) was
settled on 2026-08-08 and **the cheap answer lost**. The platform-grained key
W2 proposed is refuted by measurement — it makes 17 artifact-name collisions
across `linux` and `threadx-linux`, and it disarms the two gate arms that would
have said so. The remaining shape is the Rust-side variant slug, whose prize is
now measured (`linux` alone: 46.1 GiB → ~7.0 GiB, −84.9 %). Work-order items 7
and 8 stay blocked, re-confirmed against the tree rather than assumed.

**Closes:** issue 0446. **Touches:** phase-334 (build-cache layout — this is the
*identity* question that layout question implies), phase-336 (the profile that
made `incremental` the default everywhere), RFC-0065 (`nros build` owns the
workspace build root). **Related:** issue 0400 (host/box target-dir split).

## Goal

Compile each distinct artifact **identity** once per build, instead of once per
directory. Today the tree does the latter, and the ratio is ~21:1.

## The measurement

`nros-core`, counted across 60 leaf `target*/nros-relwithdebinfo/deps` dirs:

```
total nros-core rlibs: 106
     45  3636184e65c044e3
     23  92233bfad5e3d350
     21  25e8c1176cfcdf63
     16  f221b2c956c26591
      1  9d786b2b3017abde
```

That hex is cargo's `-C metadata` hash — cargo's own judgement that two builds
are interchangeable. So "are these the same compilation?" is answered by
construction, not by inspection. Forty-five of them are.

The symptom is a `just build-test-fixtures` run with ~8 cargo frontends live,
**0–2 `rustc` processes** at any instant, and load ~12 on 32 cores. The machine
is not compiling; it is repeating.

## The disk story

Wall-clock is the symptom that gets noticed; disk is the one that accumulates.
The same repetition, measured as bytes on a working tree (2026-08-06):

```
668G  .
   402G  examples/
    56G  target/
    50G  packages/
    39G  build/
```

One leaf, `examples/native/rust/talker`, holds **7.4 GB across five target
dirs** — `target/` 2.1G, `target-zenoh/` 1.5G, `target-tls/` 1.5G,
`target-cyclonedds/` 1.2G, `target-xrce/` 1.1G — for one binary whose lower
half is identical in all five. That is R1 stated in bytes, and it is why
`examples/` alone is 402 GB. 327 `libnros_core-*.rlib` files exist under
`nros-relwithdebinfo/deps` right now.

Incremental state is a large and separable share of it:

| | dirs | size |
| --- | --- | --- |
| `nros-relwithdebinfo/incremental` (live) | 245 | **41 G** |
| `nros-fast-release/incremental` (dead profile name) | 79 | 20 G |
| `packages/cli/target/debug/incremental` | 1 | 4.1 G |
| all `incremental/` dirs | 858 | **70 G** |

Two things follow. First, W1 has a disk answer that does not depend on the
timing question at all: `incremental = true` costs 41 GB of live state in a tree
whose builds are overwhelmingly build-once-per-leaf. Second, 20 GB sits under
`nros-fast-release`, a profile name phase-336 renamed away — nothing writes
there any more, so it is pure museum, and its presence means these trees are
never reclaimed, only added to. Any reader taking a `du` measurement here must
separate live from orphaned before quoting a number.

This is not a theoretical budget. The volume holding the checkout was at **96 %
(36 GB free)** when these numbers were taken, with 115 GiB of that in the native
lane's target dirs alone — so "run the lane A/B four times" is itself gated on
reclaiming space first. A phase about redundant compilation is also, on this
host, a phase about whether the next sweep can run at all.

**Disk is an acceptance criterion below, not a side effect.** A change that
holds wall-clock flat while removing tens of GB of redundant state is a win this
phase should be able to record; one that trades disk for time needs the trade
stated rather than discovered later.

*Host note.* These figures come from a 20-core machine; the "load ~12 on 32
cores" observation above came from a different one. Absolute wall-clocks in this
document are therefore not comparable across sections — only within a section's
own alternating reps.

## Why the repetition happens

### R1 — isolation is per-DIRECTORY, incompatibility is per-IDENTITY

The per-RMW target dirs exist so one variant's artifacts cannot overwrite
another's. But the same identity appears across all of them:

```
 6  3636184e65c044e3 target-fixtures
 5  3636184e65c044e3 target-zenoh
 5  3636184e65c044e3 target-xrce
 5  3636184e65c044e3 target-cyclonedds
```

Because `nros-core` does not depend on the RMW at all:

```console
$ grep -ciE "rmw|zenoh|xrce|cyclone" packages/core/nros-core/Cargo.toml
0
$ grep -A3 '^\[features\]' packages/core/nros-core/Cargo.toml
default = ["std"] ; std = [...] ; alloc = [...]
```

The RMW choice partitions the *upper* layers (`nros-node`, `nros-rmw-*`). The
lower layers are identical and get rebuilt once per variant for nothing. The
directory partition is coarser than the real one.

### R2 — every leaf is its own workspace

```console
root workspace:  f914a127d89299a3
leaf workspace:  317728cd7daaa57d     # examples/native/rust/talker
```

Same crate, same profile, same features — different identity, because the leaf
resolves through its own `Cargo.lock`. Leaves DO agree with each other (talker
and listener both carry `3636184e65c044e3`), which is what makes R1 fixable: the
duplicates are genuinely interchangeable among leaves.

### R3 — corrosion's explicit `--target` splits every crate again

```console
implicit host:      f914a127d89299a3
--target x86_64-unknown-linux-gnu:  888ced5467919627
```

Same triple, different identity. Corrosion always passes `--target=` (
`cargo rustc --lib --target=x86_64-unknown-linux-gnu …`); native leaves never
do. So the cmake-driven and cargo-driven builds of an identical crate can never
share, and nothing in the manifests makes that visible.

**Decided in W3 (2026-08-08): normalise toward EXPLICIT.** Corrosion hardcodes
the flag and is upstream, and the explicit spelling costs zero extra
compilations. The cmake lane is done and gated; the cargo-leaf half is a path
move that waits on W2. It was also not only "cmake vs cargo" — nano-ros' own
FFI-glue generator used the implicit spelling INSIDE a corrosion build.

### R4 — `incremental = true` destroys byte-reproducibility

```console
CARGO_INCREMENTAL=1 -> differ
CARGO_INCREMENTAL=0 -> BYTE-IDENTICAL
```

With incremental on, two builds of the same identity produce the same
`-C metadata` hash and a byte-identical `lib.rmeta`, but codegen-unit members
differing by a per-session token
(`…2h1hivz5wi6wzpcc1ckgl7n8q.03iazng.rcgu.o` vs `….0802496.rcgu.o`). Any
content-addressed reuse — a shared dir, sccache, a CI cache restore — needs
byte-stability, so this forecloses the fix in R1/R2 even where identities match.

It also costs, on a fresh target dir (alternating runs, both cache-warm, so the
ordering effect is controlled):

| | run A | run B |
| --- | --- | --- |
| `CARGO_INCREMENTAL=1` | 38 s | 27 s |
| `CARGO_INCREMENTAL=0` | **23 s** | **17 s** |

~37 % slower, consistently, in both reps — and 649 MB vs 482 MB of target dir.
Incremental pays off when the SAME target dir is rebuilt after an edit. The
fixture lanes build each leaf once into a per-leaf dir, which is the case where
it is pure cost.

**Caveat on how this was measured.** The first A/B ran 1-then-0 and showed 0
winning; the reverse order showed 1 winning. In both, the *second* run was
faster — warm caches dominated. Only alternating repetitions isolate the factor.
Any future timing claim here needs the same treatment.

**Superseded in part by the W1 result below.** A re-measurement of the same leaf
reproduced the DISK figures exactly (649 MiB vs 482 MiB) but not the timing: it
found ~10 %, not ~37 %, and no difference at all without sccache. Since the two
runs agree on disk to the byte and disagree on time by 3×, the discrepancy is in
the timing method, not in the leaf or the setting — most likely that the `run A`
/ `run B` pair above is cold-then-warm within one arm rather than two
independent reps. Take the disk column from here and the timing from W1.

## The complete incompatibility set

Re-measured 2026-08-06 on `nros-core`, **features held constant**
(`--no-default-features --features alloc,std`), one factor varied per arm, each
into its own target dir. The value is `libnros_core-<hash>.rlib`:

| Factor | Changes identity? | hash |
| --- | --- | --- |
| *(baseline)* | — | `ef26eca89af52b10` |
| Same build, different target dir | **no** | `ef26eca89af52b10` |
| Workspace root (leaf vs root) | yes | `18ad8b827eebc4b1` |
| Explicit `--target` vs implicit host | **yes, same triple** | `2b8d1fd2adda0290` |
| RUSTFLAGS | yes | `a2b0dc427d43178a` |
| Profile name — `release` | yes | `4b39f882b3c08c23` |
| Profile name — `dev` | yes | `ec004e0c9f10d286` |
| `opt-level` | yes | `742760f17a80b8cc` |
| `debug-assertions` | yes | `118191be48cc117d` |
| `panic` | yes | `866fba4594ca08e4` |
| `codegen-units` | yes | `9ffc8f9e7768b1ce` |
| `lto` | yes | `1f752f577b8e31d5` |
| `incremental` | **yes** | `b8ac6e54ff73351f` |

**Two corrections to the earlier version of this table.**

1. **`incremental` DOES change identity.** The old row said "no (identity), but
   breaks byte-equality". Measured twice over: `CARGO_INCREMENTAL=1` yields
   `b8ac6e54ff73351f`, and so does the `nros-iterate` profile — two independent
   routes to "relwithdebinfo plus incremental" landing on the same hash, distinct
   from the baseline. This is also why W1's change forced a rebuild of 37 units:
   it altered the identity, not merely the on-disk bytes.

2. **"Profile" is not one factor — it is at least six.** `opt-level`,
   `debug-assertions`, `panic`, `codegen-units`, `lto` and `incremental` each
   change identity ON THEIR OWN. Two builds can therefore name the SAME profile
   and still not share, if any single setting is overridden.

That second point is not hypothetical here: `nros_cargo_profile_env()` injects
`CARGO_PROFILE_<NAME>_*` variables for corrosion targets, so the cmake path
constructs its profile from environment rather than inheriting the manifest's.
Any divergence in one injected setting silently splits the identity. A build
tree bears this out — `examples/workspaces/mixed` carries **ten distinct profile
hashes at a single `--profile nros-relwithdebinfo`**, which is what produced the
"unattributed ×2" recorded in W4: those two `nros-core` fingerprints agree on
rustc, features, target, path, rustflags, config, compile_kind and all three
deps, and differ ONLY in `profile`.

**Practical consequence for W2/W3.** Feature-set equality is necessary and
nowhere near sufficient. Any grouping key that claims two builds are
interchangeable must cover the workspace root, the `--target` spelling, RUSTFLAGS
and every individual profile setting — not the profile NAME, which is a label
over six independent axes.

## Roadmap after the W2 decision (2026-08-08)

W2's measurement changed what this phase should optimise, so the remaining work
is organised as WAVES rather than as the original item list. The ordering is
(value x confidence) / cost, and the reason for it is one number:

**240.2 GiB — 91.1 % of `deps/` — is duplicate identity, and its largest mass is
the feature-INVARIANT host build-dep graph** (`libwinnow` x512, `libcc` x504,
`libsyn` x391, `libnros_macros` x391). This phase has been partitioning by RMW
and platform. That split does not touch these units at all. The biggest lever in
the tree was never the thing being worked on — which is worth stating plainly,
because it is the second time in this phase that a documented premise did not
survive measurement (the first was F3).

> **The byte total is confirmed; the sentence after it is not** (2026-08-08,
> [phase-343](phase-343-host-build-graph-duplication.md)). These units are NOT
> feature-invariant — 0 of 91 host-only crates carry one identity — and 32 % of
> the mass is in nested build-script target dirs rather than in leaves. Read the
> Wave 2 block below with its correction, and phase-343 for the decomposition.

### The unblock plan (2026-08-09) — follow this order

Waves 1-3 are resolved (1 and 2 by REFUTATION, 3 landed). What is left is one
serial chain plus two independent items. Numbers below are from the tree on
2026-08-09, not inherited.

**Everything still blocked traces to ONE fact: paths have not moved.**
`build/fixtures-cargo/` holds **1** entry against **110** live per-leaf target
dirs.

#### Critical path — strictly serial

**B1. Widen the collision gate beyond `[[bin]]` names.** It scans binary names
only. `libnros_c.a` is unhashed at **438 copies** across ~30 distinct sizes.
This must land BEFORE any path move, because cargo replaces the final artifact
SILENTLY across invocations — measured on a two-feature probe crate: `deps/`
kept both identities, `debug/probe` was overwritten with a different sha256 and
different behaviour, and no warning fired (the `output filename collision`
diagnostic only fires when ONE invocation builds both; a group is N). A path
move before B1 corrupts fixtures that still "build". Cheap; do it first.

**B2. Teach the resolver the variant slug. — LANDED (inert), 2026-08-10.**
W2.a called this "strictly more work"; Wave 1 proved it is the ONLY path (the
coarse platform-grained key gives 17 artifact-name collisions).

The shape is a **prefix rewrite keyed on the manifest row**:
`fixtures-manifest.py fixture-groups` pairs each cargo row's
`row_artifact_root()` with the slug the SHELL derives for it
(`nros_fixture_group_batch`, new, so the gate and the export share one loop),
and `nros_tests::fixtures::groups` inverts the root the way
`fixtures::lane` already inverts it for coordinate narrowing — leaf artifact
path → row → `build/fixtures-cargo/<slug>`, carrying every component below the
root verbatim.

Both recorded sub-blockers dissolve rather than get solved:

* the variant no longer comes from the call site at all, so `build_example` /
  `build_example_rmw` are untouched. The rewrite sits at
  `require_prebuilt_binary`, the chokepoint — which also covers the ~30 inline
  `target/`-spelling resolvers (`bins/int32-sink`, `bins/param-chatter-talker`,
  …) that the two funnels do NOT front. Fixing only the funnels was the #328
  shape.
* the `{triple}/` component is never synthesised. `--target-dir` moves the ROOT
  and nothing below it, so a cross row's leaf path carries its triple and keeps
  it, and a host row has none and does not gain one. No triple table.

Also gone: the Rust mirror of `NROS_FIXTURE_SHARED_PLATFORMS`. Eligibility is
decided once, in the shell, and arrives per row in the export. The mirror was
already WRONG — it read an empty value as "share nothing" while the shell's
`${…:-default}` reads it as "use the default". `build_root_derivation.sh` now
fails if any Rust `env::var` of that name reappears.

`check-fixture-groups`'s A2 arm was replaced with the agreement check its own
comment demanded: **A2a** the export and the gate's derivation must agree row
for row, **A2b** for every shared platform `artifact_root -> slug` must be a
function over non-empty, pairwise-unnested roots (the precondition that makes
the inversion unambiguous).

Cost: one `fixtures-manifest.py fixture-groups` per test process, measured
**111 ms** (122 cargo rows, 19 distinct slugs), lazy and `OnceLock`-cached. The
first cut was 583 ms; memoising the batch driver and dropping a fork per row
did the rest. If a sweep shows that mattering, make the EXPORT cheaper — never
add a second eligibility rule in Rust.

Verified without a provisioned tree: `just check fast`, `check-fixture-groups`,
`build_root_derivation.sh` (54 checks), `nros-tests` lib + lane +
`tests/fixture_group_resolution.rs`. That last one drives the export with
`NROS_FIXTURE_SHARED_PLATFORMS` widened to include `linux` — B3's exact change —
and asserts the four `talker` rows land in four different group dirs. Tripwires
(each verified to perturb its target): coarse key → the four-dirs test fails and
A1 reports the recorded 11 `linux` collisions; a resolver that assumes a triple
→ the triple test fails; two shared rows at one artifact root → A2b and its Rust
twin fail; export row-selection drift → A2a fails; widening the shipped list →
the inertness test fails. The make-leaf scenario in `build_root_derivation.sh`
caught a REAL bug during the work: the new `nros_fixture_platform_is_shared`
was missing from `fixtures-build.sh`'s `export -f` list, which would have made
every leaf emit an empty `--target-dir`.

**NOT verified:** anything requiring a fixture build (this was done in an
unprovisioned worktree). That is B3's acceptance, by construction.

**B3. Migrate `linux`, then the remaining platforms.** Measured prize on
`linux` alone: **46.08 GiB -> ~6.95 GiB, -84.9 %**. Acceptance is a native-lane
rebuild, because #393's failure mode is the build, the staleness probe and the
test resolver disagreeing — a gate-level check cannot see it.

After B2 the code change is one line — add `linux` to
`NROS_FIXTURE_SHARED_PLATFORMS` in `scripts/build/fixtures-target-dir.sh` — plus
two things the change makes fail on purpose:

* `tests/fixture_group_resolution.rs::the_shipped_eligibility_list_redirects_no_linux_row`
  asserts the inertness B2 claims; B3 must rewrite it to assert the migration
  instead.
* the leaf `target*/` trees under `examples/` and
  `packages/testing/nros-tests/bins/` become dead output the moment the build
  redirects. They are NOT deleted by the migration, and a stale one sitting
  beside a redirected build is invisible (the resolver stops looking there) —
  which is fine for correctness and is exactly the ignore sprawl item 7 collapses.

Then re-check: `require_shared_fixture_binary` is now a hard error for a
multi-group platform, so if any `linux` resolver still reaches it (none do
today — its only callers are `qemu-arm-baremetal`), it will say so by name
rather than silently answering for the default group.

**B4. Items 7 and 8 fall out of B3.**
* Item 7 becomes real once 110 collapses toward 1; until then every ignore line
  still names live output and the collapse is cosmetic.
* Item 8 re-measures and lowers budgets IN THE SAME COMMIT — but first explain
  the `worst crate` 5 -> **6**/9 drift on an ostensibly unchanged tree (Wave 1
  suspected another session's build state). Lowering against an unexplained
  number is how a gate starts lying. Current reading: `nros_core 4/8; worst
  crate 6/9; worst identity 5/5`.

#### The remaining roadmap (2026-08-10) — P1..P5

Wave 2 migrated six platforms. Re-measuring the source tree reframes what is
left: of the **266 source dirs still holding build output**, roughly **240 are
CMAKE-style** (`build` 85, `build-zenoh` 76, `build-cyclonedds` 36, `build-xrce`
12, `build-workspace-*`) against **~20 cargo-style** (`target*`). **The cargo
half is essentially done; the remaining mass is cmake**, which the shared-group
mechanism never touched.

**P1 — item 8. DONE 2026-08-10.** `nros_serdes` at 6 is decomposed (3 identity
pairs x 2 `--target` spellings — see the gate's own comment), and
`CEILING_IDENTITIES` is lowered 12 -> 6. Note issue 0485 had already fixed a
counting bug in the gate, so part of the "5 -> 6 drift" was the gate, not the
tree. ~~The ceiling should reach 3 when W3's cargo-leaf half lands.~~

**P1 corrected, 2026-08-10 (same day, second reading) — the decomposition above
is wrong and the prediction with it. This is the SIXTH documented premise in
this phase to fail under measurement, and the first one the phase wrote itself.**
`CEILING_IDENTITIES` is now **5**, measured on a REBUILT tree. Two errors, both
readable in the fingerprints (`<root>/<profile>/.fingerprint/nros-serdes-*/lib-*.json`):

* **Two of the three "pairs" are not `--target` pairs.** Their members differ in
  FOUR fields — features `[]` vs `["alloc","std"]`, build-override vs product
  profile, rustflags `[]` vs `["-C","symbol-mangling-version=v0"]`, and
  `compile_kind` — because the host member is the PROC-MACRO graph
  (`nros-macros` -> `nros-orchestration-ir` -> `nros-rmw` -> `nros-core` ->
  `nros-serdes`), which cargo builds for the host inside the SAME explicit
  invocation. No spelling merges a unit that also differs in features and
  profile. The gate's own R3 note already said this ("the host column can never
  reach zero"); the decomposition read the report's two columns as two
  invocations.
* **The one real spelling pair was already dead.** Its implicit member is
  residue from before W3's cmake half landed on 2026-08-08: the post-fix
  invocation wrote only `…/target/<triple>/<profile>/deps`, and the host dir's
  newest file predates it by 13 hours. A fresh
  `workspace-fixtures-build.sh linux mixed` writes no host copy at all under
  `nano_ros_cpp_ffi_*/target/` — which is also the first REBUILD confirmation of
  W3's cmake half on the native lane (it landed verified only by the buildless
  gate).

So the number the gate reads is 5, and **the cargo-leaf half cannot move it in
either direction**: every writer under the measured tree is corrosion
(`cargo/<root>_<h>`, which hardcodes `--target`) or `nros_generate_interfaces()`
glue. There is no cargo LEAF build in it.

**3 is still the right target — it belongs to item 5 (W2), not to W3.** The two
corrosion roots' members differ only in cargo's `path` field
(repo-root-relative vs absolute), so collapsing R2 merges host with host and
target with target: 5 -> 3. Predicted here so the next reader can falsify it the
same way.

**It was falsified the same way, 2026-08-10 — and that is SEVEN.** The
observation survives; the work item built on it does not. `path` is not a
spelling any caller chooses, so there is no edit that "collapses the two roots'
path spelling". See "R2 re-measured — `path` is a RELATION, not a spelling"
below. 5 -> 3 is still the right target and still R2's, but it costs a root,
not a string.

**P2 — close the second build path. DONE 2026-08-10.** See "P2 — what the second
path actually was" below. The `examples/**/target/` population is empty and
gated; the residue (per-leaf dirs under `packages/testing/**`, and authored
`target-<variant>/` dirs on migrated platforms) is issue 0488. P4 is unblocked
by P2; it still waits on P3.

**P3 — relocate the cmake artifacts. SPLIT OUT to
[phase-344](phase-344-cmake-cache-relocation.md); MEASURED 2026-08-10, and this
paragraph is the FIFTH documented premise in this phase to fail under
measurement.** It read "~240 dirs … the mass … different mechanism from the
cargo groups — `CMAKE_BINARY_DIR`, not `--target-dir`". Re-derived:

* **151 dirs, not ~240.** The `build` column (107) is not cmake — 93 of it is
  `nros sync`'s `<leaf>/build/{nros,nros-metadata}/` model+metadata output
  (phase-330's subject, `model_location`'s to move), 12 is codegen output, and
  **one is `scripts/build`, a source directory the name glob matched**. The
  suffixed families (`build-zenoh` 76, `build-cyclonedds` 36, `build-xrce` 12,
  `build-workspace-fixtures*` 26) reproduce exactly.
* **It is not a different mechanism; it is 83.2 % the SAME one.** Of the
  182.3 GiB in those dirs, **151.7 GiB is `<CMAKE_BINARY_DIR>/cargo/` —
  corrosion's cargo target dir**. Only 30.6 GiB is cmake/ninja output.
* **And relocation cannot cash it.** `CMakeCache.txt` pins
  `CMAKE_HOME_DIRECTORY`, so cmake binary dirs cannot merge across leaves;
  corrosion v0.6.1 derives its target dir from `CMAKE_BINARY_DIR` with **no
  override** (and passes `--target-dir` on the command line, beating both
  `CARGO_TARGET_DIR` and `build.target-dir`). So moving these dirs frees
  **zero bytes**. The 50.1:1 duplication inside them — the largest measured
  anywhere in this tree — is a separate item with its own hazard (132 copies of
  `libnros_c.a` in **15 distinct sizes**, i.e. W1's silent-overwrite refutation).

P3 therefore proceeds as an R1/R2/R3 **compliance** move that unblocks P4, and
must not be budgeted as a disk phase. Waves, disk budgets (69 G free vs 88.1 GiB
for `build-zenoh` alone) and the 7-dir attribution gap that must close first are
in phase-344.

**P4 — item 7 final.** Delete the `.gitignore` block once P2 and P3 land. It is
already written to say it should be deleted rather than extended.

**P5 — debt, parallelisable throughout.** Issue 0481 owns the readiness-marker
class (do not duplicate); issue 0472's 13 unguarded opaque macros; the ~13 real
`ci-matrix` failures, several confirmed flakes.

**P6 — validation: run issue 0200's timing campaign, AFTER item 5.** 0200 has sat
blocked since phase-226 on "needs a runner with ≥200 GiB scratch", a requirement
that is a consequence of the duplication this phase removes. phase-343 W1 already
recovered 63.1 GiB and B3 took `linux` from 26796 MB to 5226 MB; item 5's measured
prize is another −84.9 %. **Re-derive 0200's disk requirement against the
post-item-5 tree before treating it as blocked** — it may simply run on the
maintainer host now. It belongs after item 5, not before: item 5 changes both
quantities the campaign measures, so a pre-item-5 number is obsolete on arrival.
0200's own follow-up candidate ("shared Corrosion `--target-dir` per role group")
is this phase's W2 and is already decided; the issue should stop carrying it.

Order: P1 and P2 in parallel -> P3 -> P4 -> P6, with P5 filling gaps.

**Note for P4.** The per-leaf `.gitignore` files that `8f5cb2d18` restored carry
more than build DIRECTORIES: the NuttX leaves' `/.built`, `/.depend`,
`/Make.dep`, `/src/*.o` cover output the apps build writes **in place**, beside
the sources. That is a FILE-grained R1 violation no coordinate can express and no
`--target-dir` can redirect (issue 0488 residue 4), so those lines outlive P4 —
they die only when the NuttX apps build moves out of tree. Deleting a leaf's
whole ignore file on P4's schedule un-ignores 60 files across 12 leaves; that
regression was observed in the window between `53681ecbc` and `8f5cb2d18`.

**Carry this rule into P3:** re-measure before building. This phase refuted FOUR
documented premises that way — F3's "net loss", the umbrella workspace, the
platform-grained key, and the "feature-invariant host graph" framing. The
240-dir cmake figure above is itself one `find`; re-derive it before designing
against it.

**It was re-derived, and it was wrong — that is now FIVE.** See P3 above and
[phase-344](phase-344-cmake-cache-relocation.md) §1. The instruction worked; the
figure it warned about did not survive one `CMakeCache.txt` test.

**SIX, and this one the phase wrote about itself.** P1's decomposition of
`nros_serdes` = 6 into "3 identity-pairs x 2 `--target` spellings", and the
success criterion built on it ("should fall to 3 when the cargo-leaf half
lands"), did not survive reading the four fingerprint JSONs it summarised — see
"P1 corrected" above. A budget lowered on a decomposition is only as good as the
decomposition; check it against `.fingerprint/*/lib-*.json`, which names every
field cargo actually keyed on, before predicting which work item moves it.

#### R2 re-measured — `path` is a RELATION, not a spelling (2026-08-10)

The premise above was checked before anything was changed, on the fresh mixed
tree the gate reads. **Its observation is confirmed and its conclusion is
wrong**, which is the seventh premise this phase has lost to a measurement.

**Confirmed: the two roots' units differ in exactly one field.** All four
`nros-serdes` fingerprints
(`<root>/[<triple>/]<profile>/.fingerprint/nros-serdes-*/lib-*.json`) agree on
`rustc`, `features`, `declared_features`, `target`, `profile`, `deps`,
`rustflags`, `config` and `compile_kind`; `path` alone splits them, the same
way in the host pair and in the target pair:

| unit | `path` |
| --- | --- |
| `cargo/nano-ros_0b88c/…` (host and target) | `4098220467150443427` |
| `cargo/nros_ws_runtime_14eac/…` (host and target) | `2387503793444693694` |

**What that field contains, read rather than inferred** — the dep-info files
beside those rlibs spell the same source two ways:

```
nano-ros_0b88c/…/deps/nros_serdes-a41154198c4836e2.d:
    packages/core/nros-serdes/src/lib.rs
nros_ws_runtime_14eac/…/deps/nros_serdes-4e5cd29b6c626931.d:
    <abs-repo-root>/packages/core/nros-serdes/src/lib.rs
```

**And that is cargo choosing, not nano-ros.** `path_args` spells a unit's source
RELATIVE to the workspace root when the package sits inside it and ABSOLUTE
otherwise. Root A builds `nros-serdes` as a **member** of the repo-root
workspace; root B reaches it as an **out-of-workspace path dep**. So `path`
records the RELATION between the invoking workspace and the crate. Nothing in
the generated manifest, the `MANIFEST_PATH` argument or the `path =` dep line
selects it.

**Measured, not reasoned — a 30-line reproduction of the whole phenomenon.**
One dependency crate `b`, three invocations, default profile, no features:

| arm | artifact |
| --- | --- |
| `b` is a MEMBER of the workspace above it | `libb-6531a4de8c413d0d.rlib` |
| `b` is a path dep of a build-dir crate, dep line **absolute** | `libb-a3d9a90756a4909e.rlib` |
| `b` is a path dep of a build-dir crate, dep line **relative** | `libb-a3d9a90756a4909e.rlib` |

Arms 2 and 3 are byte-for-byte the same identity, and arms 1 and 2's
fingerprints differ in exactly one field — `path` — i.e. the mixed tree's split,
in miniature. **So re-emitting `nros_write_runtime_umbrella_crate`'s
`nros-cpp = { path = "…" }` line relative changes nothing**, which was the
obvious edit and is a trap: it would have produced a green gate reading the same
5, and a commit message claiming a mechanism that does not exist.

**The same tree carries the control case.** `stable_deref_trait` — a REGISTRY
dependency, so its source id is not a path and there is no workspace root to
strip — appears under BOTH corrosion roots with the SAME two hashes
(`ef7a4cf9ec5986e9` host, `926c47a90bb1b2ec` target). The roots are not split by
a profile, a flag or an environment difference: they split exactly where cargo
records a path relation, i.e. on nano-ros' own crates. (Registry crates can
still split for a different reason — `thiserror` shows four, because the two
roots resolve through different LOCKS and pick different versions.)

**Two cargo rules close the remaining space, both observed here:**

```
$ cargo metadata            # build-dir workspace, members = [<in-repo crate>]
error: package `…/packages/core/nros-serdes/Cargo.toml` is a member of the wrong workspace
$ cargo metadata            # same, with the repo root EXCLUDING that crate
error: workspace member `…/crates/b/Cargo.toml` is not hierarchically below the
       workspace root `…/synth2/Cargo.toml`
```

A synthesised workspace in `CMAKE_BINARY_DIR` therefore can never take the
in-repo crates as MEMBERS — not even if the root workspace excludes them — so
the only relation it can have to them is the path dep it already has. The
mirror-image move fails on the same rule: the umbrella cannot join the repo-root
workspace, because a user's `CMAKE_BINARY_DIR` is not below the repo root, and
because it path-deps the USER's node packages against a per-configure generated
lock that the tracked root lock cannot promise.

**What actually reaches 3.** The two roots share an identity only if they relate
to `packages/**` the same way. Three ways exist and none is a path spelling:

1. **Delete root A from the configure** — issue 0493's single-provider design
   (one synthesised provider owning the archive AND the generated headers).
   Attempted and reverted there; note root A also provides `nros_c-static`,
   which a mixed workspace's C nodes need, so the provider must subsume the C
   half too. This is the direction 0493 already recommends.
2. **Move `nros-c` / `nros-cpp` out of the repo-root workspace** (own
   `[workspace]` table), so the cmake import reaches the stack as path deps as
   well. Costed here rather than guessed: both manifests inherit
   `version/authors/repository/homepage/documentation.workspace` and ~20
   `workspace = true` dependency rows, ~12 `justfile` / `scripts/` sites build
   them with `-p` from the root workspace, and each would need its own tracked
   lock. It is a repo-structure decision, not a build-wiring edit.
3. **Give the umbrella the repo root as its workspace root** — refuted above by
   the hierarchy rule plus the tracked lock.

So `CEILING_IDENTITIES` stays **5**: the truth did not move, and the gate must
not either. The R2 axis is still worth ×2 on every crate the two roots share —
the prize is real; the cheap lever was not.

#### P2 — what the second path actually was (2026-08-10, LANDED)

**The sweep first, because the reported site was not the only one.** Every
`cargo build` / `cargo rustc` in `just/*.just` and `scripts/**` whose working
directory is a leaf, classified by what it writes:

| writes | site | in `build-test-fixtures`? |
| --- | --- | --- |
| `examples/**/target/` | `just/freertos.just` `build-examples` — six Entry pkgs | **yes** |
| `examples/**/target/` | `just/native.just` `build-examples` — 27 dirs | no |
| `examples/**/target/` | `just/qemu-baremetal.just` `build` — 15 + 5 dirs | no |
| `examples/**/target/` | `just/nuttx.just` `_run-qemu` — and it hand-spelled its own `-kernel` path | no |
| `examples/**/target/` | `scripts/qemu/run-multi-node-test.sh` | no |
| `packages/testing/**/target/` | 4 sites (wake-latency pair, smoltcp bridge, rtic e2e, ros-edition pose-pub) | 1 of 4 |
| authored `target-<variant>/` | 6 sites on migrated platforms (threadx ×2, freertos dev ×2, ros-editions, fixture-make-driver) | 1 of 6 |

Only the first block blocks item 7 / P4, and the reason is exact: the repo-root
ignore is `examples/**/target-*/`, **with a dash**. A plain `target/` is covered
only by the 391 per-leaf `.gitignore` files item 7 deletes. So P2's scope is the
first block; the rest is issue 0488, with the measurement recorded rather than
deferred to another sweep.

**The mechanism, per the work order's preference (a row, not a second rule):**

*freertos* got a `[[fixture]]` row, and it turned out the row already existed
and was pointing at the wrong build. The six rows carried
`no_default_features = true` + `target_dir = "target-zenoh"`; the recipe ALSO
built the same six dirs bare. Both facts that made this a duplicate rather than
two builds were measured, not assumed:

- `--no-default-features` was a **no-op** — none of the six packages declares a
  `[features]` table, so the two builds compiled identical inputs;
- the `target-zenoh` output had **no live reader**. Its only resolver is
  `build_freertos_rust_example_rmw`, whose two callers are the `#[ignore]`d
  cyclonedds tests, which take the Cyclonedds arm and error before the path is
  built. The bare build's `target/` output had **two** readers.

So the row was repointed at the build the tests consume and the loop deleted:
one build where there were two, with a coordinate and therefore a group.

*native / qemu-arm-baremetal / nuttx* are `git ls-files` sweeps over every
tracked example crate, including crates with no fixture row. A row is WRONG for
them — it would put non-fixture crates into the fixture coordinate space,
changing lane coverage and the A1 collision surface — so they were placed via
the existing derivation (`nros_fixture_target_dir_flag`, and its inverse
`nros_fixture_row_artifact_dir` for nuttx's `-kernel` lookup), never a literal.
Bin-name collisions were checked before merging each sweep into its group:
`linux` 43 dirs / 0, `qemu-arm-baremetal` 20 / 0, `freertos` 7 / 0. For the ~22
dirs that DO have a row, the sweep now lands on the row's own artifacts and
becomes an incremental no-op instead of a second copy.

`scripts/qemu/run-multi-node-test.sh` was deleted: it built `examples/qemu-test`
(a directory that does not exist) and `-p native-talker -p native-listener`
(packages the standalone-example migration renamed). It could only ever fail.

**The bug the profile carve-out then exposed, which is the real #393 content
here.** The six FreeRTOS resolvers read `FREERTOS_QEMU_PROFILE`; the manifest
builder uses the ambient profile; so the lane had to move onto the carve-out
(`NROS_CARGO_PROFILE`, the shape `just nuttx build-fixtures` already used).
That surfaced a mapping living in three shapes — a helper call in
`freertos.just`, the bare literal `nros-minsizerel` in eight `nuttx.just` lines,
and **nothing at all in the staleness probe**. `rust-fixture-stale.sh` rebuilds
a row with its exact flags to ask cargo whether it is fresh; at a profile the
builder never wrote that is a full compile into a second profile dir and a
permanent false-STALE. It had gone unnoticed only because every nuttx row is
`skip_probe = true`, so `freertos` was the first carve-out lane the probe
watched. All three collapsed onto `nros_cargo_platform_profile <platform>`.

And two test-side locators moved in the same commit, which is the point:
`freertos_run_plan_runtime.rs` carried a private `locate_entry_binary` reading a
hardcoded `target/…/release/` — a directory phase-336 had already moved away
from — so its six gates could only ever report `[SKIPPED] … not prebuilt`, no
matter what the build did. It now calls
`fixtures::freertos::require_entry_binary`, the one spelling, which also applies
the shared-group rewrite.

**Gate:** `check-example-leaf-target-dirs` (in `check-fast`). Rule: a cargo
build whose cwd is an `examples/` leaf must pass a `--target-dir` — literal or
resolver-derived, but never the default. It joins lines across `\`
continuations and carries a bare `cd` forward, because the sites use all three
idioms; it self-tests its classifier on every run, and it was tripwired in both
directions against the REAL tree (T1 re-introduce the exact defect → fails and
names the file; T2 same line with a derived flag → passes; T3 a `*tdir_flag`
variable in a file that never derives one → still fails), with each perturbation
confirmed present before the verdict was read.

**Acceptance was a rebuild, not the gate** — gate-level checks were green for
all six platforms while the build was broken, which is why.

Verified in a pristine worktree (no `build/`, no per-leaf dirs, so "not
recreated" is provable rather than inferred): `just setup-cli` →
`just freertos build-examples` →

- all six Entry ELFs in `build/fixtures-cargo/freertos/thumbv7m-none-eabi/nros-minsizerel/`
  (2.84 MB each);
- `find examples/qemu-arm-freertos -maxdepth 3 -type d -name 'target*'` → **empty**;
- `fixtures-manifest.py fixture-groups` attributes all seven freertos rows to
  group `freertos`, eligible;
- **all six `freertos_run_plan_runtime` gates PASS** — they had only ever been
  able to skip, because of the hardcoded `release/` above — plus
  `logging_smoke_freertos_mps2` (the row that moved onto the carve-out) and
  `freertos_qemu`. The other `logging_smoke` cells fail on fixtures this
  worktree never built; unrelated.

**Two defects the verification found, neither of them P2's:**

- **issue 0490 (fixed + gated here).** The probe reported all seven rows STALE
  immediately after building them. Cause, from cargo's own fingerprint log:
  `packages/rmw/cffi/build.rs` declared `rerun-if-changed=../nros-rmw-abi/…`, a
  path that does not exist since phase-321 W2.e moved the crate out of `core/`.
  A missing rerun-if-changed input is permanently dirty, and `nros-rmw-cffi` is
  under every image — so **every Rust fixture in the repo had been recompiling
  its whole chain on every build**, silently. Gated by
  `check-build-rs-rerun-paths` over all 57 tracked build scripts.
- **issue 0491 (filed, not fixed).** With 0490 fixed, a row is stable ALONE but
  siblings in one group still rebuild each other: leaf `[env] relative = true`
  values are per-leaf STRINGS for one directory, and cargo compares
  `rerun-if-env-changed` textually. Per-leaf `target/` dirs hid it; sharing the
  dir is what surfaced it. It costs the group's CPU win, not its disk win —
  which means any "the group made it faster" number measured over more than one
  row in a group is measuring this too.

**Not verified:** the freertos WORKSPACE lanes (`workspace-fixtures-build.sh
freertos {rust,c,cpp,mixed}`) — this worktree lacks the `nuttx-libc` vendored
source they need, so `build-examples` exits non-zero after the rust lane. P2
changes nothing in those lanes. `native` / `qemu-arm-baremetal` / `nuttx`
rebuilds were not run either (their toolchains and fixtures are not provisioned
here); their change is the same one-line derivation and their collision
preconditions were checked, but the REBUILD proof stands only for freertos.

#### Item 7 — attempted again after wave 2 (2026-08-10). Blocked by a MIGRATION GAP, not housekeeping

The sprawl item 7 targets is **391 per-leaf `.gitignore` files**, each holding
`/target/`. They are load-bearing today: the global pattern is
`examples/**/target-*/` (with a dash) and does NOT cover a plain `target/`, so
deleting a leaf's ignore while anything still writes its `target/` un-ignores
build output.

**And something still does — including on a MIGRATED platform.** After wave 2,
`examples/qemu-arm-freertos/rust/talker` is a shared-eligible cargo row (the
manifest names that dir) and yet:

```
build/fixtures-cargo/freertos                       01:53   <- the shared group
examples/qemu-arm-freertos/rust/talker/target       01:55   <- written AFTER
  target/nros-minsizerel/{deps,incremental}
```

So a SECOND build path writes per-leaf dirs and does not transit the group
resolver — the `build-examples` lane rather than `build-test-fixtures`. That is
#393's shape (two build paths disagreeing about where artifacts live), and it
means B3's migration covers the fixture lane only.

**Consequence for the roadmap:** item 7 is not "delete ignores once platforms
migrate". It is blocked on closing that second path, which is real work in the
B-chain, not cleanup. Surviving per-leaf dirs after wave 2, for the record:
`qemu-arm-freertos` 6, `native` 2 (`target-cyclonedds`), `qemu-esp32-baremetal`
2, `workspaces` 9.

`esp32` / `nuttx-riscv` are settled separately: they have workspace fixture rows
but NO standalone cargo rows, so they are correctly outside the shared-group
mechanism rather than pending migration.

#### Items 7 and 8 — attempted 2026-08-10, BOTH still blocked (evidence, not deferral)

**Item 7 — the esp32 half of its rationale was wrong, and is now fixed. The
item itself stays blocked, on the gap `d05ecf1fa` found.**

Two sessions reached item 7 from opposite ends on 2026-08-10 and each got half
of it. `d05ecf1fa` has the blocker right and it is NOT the ignore pattern:

> `examples/qemu-arm-freertos/rust/talker` is a shared-eligible cargo row, and
> its `target/nros-minsizerel` was written at 01:55, two minutes AFTER
> `build/fixtures-cargo/freertos` at 01:53. A second build path
> (`build-examples`, not `build-test-fixtures`) writes per-leaf dirs without
> transiting the group resolver.

That stands, and `53681ecbc` consolidated the 391 leaf ignores into one
rule-shaped block rather than deleting them, which is the right call while
anything still writes those dirs.

**What does not stand is its closing line**, "esp32 / nuttx-riscv have workspace
fixture rows but no standalone cargo rows, so they sit outside the shared-group
mechanism by construction". That is the third time this platform has been asked
about under a spelling the manifest does not use. `esp32` has zero `[[fixture]]`
rows; **`qemu-esp32-baremetal` has three**, all rust, all standalone cargo rows:

```
examples/qemu-esp32-baremetal/rust/talker
examples/qemu-esp32-baremetal/rust/listener
packages/testing/nros-tests/bins/logging-smoke-esp32-qemu
```

It is now migrated — A1/A2/A3 clear (7 platforms, 19 groups, 122 rows, 0
collisions, 122 roots invertible), rebuilt, and its artifacts land in
`build/fixtures-cargo/qemu-esp32-baremetal/`. `nuttx-riscv` IS correctly
excluded: its single `[[fixture]]` row is C. The workspace-fixture half of the
claim is right for both and irrelevant to eligibility — `fixture-groups` reads
only the `[[fixture]]` table, and a workspace row keeps its authored
`target_dir`, so gate scope and mechanism scope already agree.

**Migrating it exposed the SAME class as `d05ecf1fa`'s gap, one path over.**
Gates were green while the build failed:

```
ERROR: examples/qemu-esp32-baremetal/rust/talker/target/riscv32imc-unknown-none-elf/
       nros-relwithdebinfo/esp32_qemu_talker is missing, and nothing narrowed this build.
```

`just esp32 build-qemu` packs its ELF into a flash image and hand-wrote the leaf
path to find it. The migration redirected the BUILD and left the pack step
reading a directory nothing writes any more. Two independent sites now — a
post-processing step here, `build-examples` there — both spelling the artifact
path by hand instead of asking the group.

So the fix is the shared one: **`nros_fixture_row_artifact_dir`** in
`fixtures-target-dir.sh`, the inverse of `nros_fixture_target_dir_flag` from the
same `nros_fixture_group` call, returning the leaf's own `target/` for an
unmigrated platform so a call site is correct on both sides of a migration.
`d05ecf1fa`'s `build-examples` gap should close onto this helper rather than
gaining a third spelling — that is what makes it "real work in the B-chain"
rather than cleanup.

Verified by rebuild: all three ELFs in the group dir, the leaf dir not recreated
(mtime still 2026-08-03), and the pack step reaching its `espflash` check, which
sits AFTER the existence check — so reaching it is the proof the ELF was found.
**Not verified: the flash image itself**, because `espflash` is not installed
here; the step skips with a warning rather than passing silently.

**Housekeeping done alongside, and it removed two of the four "live platforms"
the old blocker rested on.** Re-measured: 84 per-leaf target dirs, 81 dead
(84.0 GB), 3 live (3.0 GB) — deleted, along with `examples/stm32f4`, which was
**entirely untracked**: 1.8 GB orphaned from a removed example tree, the same
shape as the `ws-*` leftovers, counted as a live platform because nobody asked
git about it.

**Item 8 — the precondition is now specific rather than vague.** Post-B3 reading
on a current mixed tree (rebuilt during the 2026-08-10 `lane=all` run):

```
nros_core 4/8 identities; worst crate 6/9; worst identity 5/5 copies
R3 axis (host vs explicit --target): identities 141/52, copies 192/80
```

The drifting crate is **`nros_serdes`, at 6 identities** (the recorded budget was
written when it was 5). B3 cannot explain it: B3 moved `examples/native`, and
this gate reads `examples/workspaces/mixed`, which B3 does not touch.

**RESOLVED 2026-08-10 — there was no drift. The instrument was broken (issue
0485).**

`nros_serdes` was never at 6. It measures **5**, under both the old counter and
the new one, and it is not the worst crate. The figure that moved 5 → 6 → 7
across sessions was `worst crate`, and it moved because
`check-artifact-identity-budget` counted one crate as two:

```sh
awk '{print $1, $2}' | sort -u | awk '{print $1}' | uniq -c
```

`uniq -c` collapses only ADJACENT duplicates, and glibc `en_US.UTF-8` collation
ignores the space and the underscore. `nros 079babbe…` and `nros ecf76437…`
therefore sorted on either side of `nros_board_common`, `nros_core` and
`nros_cpp`, and the crate was emitted twice — as 7 and as 5. **The run boundary
moved as hashes changed between builds.** That is the whole "drift".

The cost was not only a misreported headline. `awk '$1 > CEILING_IDENTITIES'`
compared 7 and 5 against 9, so `nros` at **12** identities passed the tree-wide
ceiling silently on every run since the gate landed on 2026-08-07 — the ceiling
had stopped gating the moment it was written. And `crate_identities()` returns
two lines for a split crate, which makes `[ "$n" -gt "$k" ]` a bash syntax
error; the budgeted crate `nros_core` stayed contiguous only because its four
hashes start 0/4/6/9.

Fixed by counting in one awk pass over an array keyed `(crate, hash)`, plus a
self-test that runs on every invocation — because nothing about a wrong reading
looks wrong, and the old pipeline printed a plausible smaller number and exited
0. Both fixes verified by reverting the counter and watching the self-test fail.

**First honest numbers, and the budgets now set to them:**

| | recorded 2026-08-07 | true, 2026-08-10 |
| --- | ---: | ---: |
| `nros_core` (budgeted) | 8 | **4** |
| worst crate | 9 (`nros_serdes`) | **12** (`nros`) |
| worst identity, copies | 5 | 5 |

`12` is not a raised ceiling — it is the first reading of this axis that
measured what it claimed, and it decomposes exactly: **2 workspace roots × 2 R3
halves × 3 feature identities**. `nano-ros_23c15` and `nros_ws_runtime_16b35`
are the roots (Wave 1's "22/22 leaves are workspace roots"); host `debug/deps`
versus explicit `x86_64-unknown-linux-gnu/debug/deps` is the R3 split W3 made
universal. Nothing is unexplained, which is exactly the precondition this item
demanded before any number moved. `nros_core` tightens 8 → 4 in the same edit,
as the item asked.

**Item 8 is therefore CLOSED, and by its own rule rather than in spite of it.**
The rule said not to ratify an unexplained number; the resolution was that the
number was an artifact, and the explained one is now pinned.

**This is the phase's standing rule applied to the phase's own instrument.**
"Re-measure an N of M claim before building on it" had already been paid for
three times here — F3's "net loss", the impossible umbrella workspace, the
platform-grained key. This is the fourth, and the first where the unreliable
measurement came from the gate this phase built.

#### Independent — no blocker, best value/effort on the board

**I1. phase-343's probe-dir wiring fix — 76.8 GiB.** The sharing mechanism
already EXISTS, is already keyed correctly, and landed 2026-08-04 — but it is
opt-in via one exported variable in `scripts/build/cargo.sh`, and **the default
is the 195 MiB-per-instance branch**. 425 nested probe dirs leaked; the sizes
probe alone is 63.1 GiB deduplicating 81:1. Wiring, not design; needs no path
move. **Highest value per unit of work currently available — do it first of
everything.**

**I2. Standing debt.** Issue 0481 owns the readiness-marker class (do NOT
duplicate it); issue 0472's 13 unguarded opaque macros; the ~13 real
`ci-matrix` failures (junit, not console — several are confirmed to pass solo,
so part of that set is flake triage).

#### Order

`I1` -> `B1` -> `B2` -> `B3` -> `B4`, with `I2` filling gaps. I1 and B1 are
parallelisable; B2..B4 are strictly serial.

#### The standing rule this phase paid for three times

**Re-measure an "N of M" claim before building on it.** F3's "net loss" stood
for months on evidence that varied `incremental` rather than sharing; the
umbrella workspace was impossible (22/22 leaves are workspace roots); the
platform-grained key silently overwrites artifacts. Each was refuted by
measurement, and each had been written down as settled.

Corollary, paid for twice in one session: **verify that a tripwire actually
perturbs the code path it targets.** One landed in a docstring and passed; one
used a coords path bogus enough that the guard refused for the wrong reason.
Both looked like green tripwires.

### Wave 1 — cash the win that is already proven

The mechanism is decided and measured (arm B: 455 MiB vs 9.70 GiB, 100 % of the
umbrella's benefit at 1/9 its complexity). It is deployed on ONE platform.

1. ~~Platform-grained group key~~, then migrate `linux`. **The platform-grained
   key is REFUTED (2026-08-08) — see "W1 — the platform-grained key, refuted"
   below.** The remaining shape is the alternative W2.a kept on the table: teach
   the Rust resolver the hashed variant slug by shelling into
   `nros_fixture_group_slug`, then migrate. The prize is measured and large
   (`linux` alone: 46.1 GiB → ~7.0 GiB). Verification is a native-lane rebuild.
2. **Item 7 (334 W2.c)** — collapse `.gitignore` once those paths move.
3. **Item 8 (340 W7)** — re-measure both axes; lower all three identity budgets
   IN THE SAME COMMIT.

(2) and (3) are hours once (1) lands, and neither can precede it: a budget
lowered against an unchanged tree fails on the truth, and there is no ignore
sprawl to collapse until a path moves. **Both were re-confirmed blocked on
2026-08-08**: `build/fixtures-cargo/` holds exactly one entry
(`qemu-arm-baremetal`), against 116 live per-leaf target dirs under `examples/`
(64 `target/`, 52 `target-*/`), so every ignore line still names live output;
and the identity gate reads `nros_core 4/8; worst crate 6/9; worst identity 5/5`
on the provisioned tree, i.e. one of the three has drifted UP since W4 recorded
it (`worst crate` was 5) — a lowered budget would fail on the truth in exactly
the way W2 predicted.

### W1 — the platform-grained key, refuted (2026-08-08)

**A group's members share a flat artifact namespace, and cargo does not hash the
final artifact name.** That is already the premise of `check-fixture-groups`; W2
applied it between packages and not within one. A platform-grained key puts
every variant of a leaf in one group, so all of them write ONE
`<group>/<profile>/<bin>`:

| key | colliding artifact names |
| --- | --- |
| variant-grained (today) | **0**, across all 7 platforms |
| platform-grained | **17** — 11 on `linux`, 6 on `threadx-linux` |

All 17 are *same-package* clashes: one leaf, several manifest rows. `talker`,
`listener`, `action-client`, `action-server`, `service-client`,
`service-server`, `int32-sink` and four more each have 2–4 rows.

**These are different binaries, not redundant copies.** Measured read-only on
the provisioned checkout, `examples/native/rust/talker`:

| row | dir today | bytes | sha256 (16) |
| --- | --- | --- | --- |
| default | `target/` | 8 616 504 | `f8a81814edec155e` |
| `rmw-zenoh` | `target-zenoh/` | 8 616 504 | `183853240bc2014c` |
| `rmw-xrce` | `target-xrce/` | 6 514 392 | `10b8273c6723f534` |
| `link-tls` | `target-tls/` | 9 034 536 | `4ac89bf3f73cc73a` |

**And cargo does it silently.** The `output filename collision` warning W2
measured fires only when ONE invocation builds both targets. Arm B is N
sequential invocations, so there is no warning at all — verified with a minimal
two-feature crate into one `CARGO_TARGET_DIR`: `deps/` correctly held both
identities (`probe-58132cd911fcb933`, `probe-aaff35a67a56532a`) while
`debug/probe` was replaced, hash and behaviour both. Last writer wins and the
test resolver greens on the other variant — the outcome this gate exists to
prevent.

**Why the earlier check said "collide on nothing".** `check-fixture-groups`'s A1
keyed its owner set on `(package, directory)`. `linux` has 65 rust rows and 41
distinct leaf dirs; the four `talker` rows deduped to one owner, so A1 reported
zero. Run against a coarsened key it printed *"2 shared platform(s), 2 group(s),
61 row(s) — no artifact-name collisions"* and exited 0. **The coarse key also
disarms A2 by construction** — A2 requires every group to be the default group,
which becomes vacuously true — so neither arm of the gate could have caught it.

Fixed here: A1's owner is now the manifest ROW. Same experiment, 11 failures on
`linux`. Tripwired both directions (widened gate + coarse key → FAIL; shipped
gate + coarse key → PASS, which is the defect).

The key-level assertion in `build_root_derivation.sh` *did* already block the
coarse key — but its failure message said "it changes `-C metadata`", which is
true and useless (`deps/` puts that hash in the filename, so variants coexist
there fine) and reads as an obsolete artefact of the umbrella shape. It now
names the flat artifact namespace instead, and gained a sibling arm for the
build env, which is in the key for the same reason
(`nros-bench/stress-zenoh` has a bare row and a
`ZPICO_SUBSCRIBER_BUFFER_SIZE=8192` row, one package, one binary name).

#### What the variant-grained key is worth on `linux`

Measured read-only over the provisioned checkout — for each row, the target dir
it actually built into; `deps/` deduplicated by artifact NAME, which is cargo's
own judgement since `-C metadata` is in the filename:

| group | rows | on disk | shared-dir estimate | deps files | distinct |
| --- | --- | --- | --- | --- | --- |
| `linux` (default) | 38 | 31.67 G | 3.14 G | 19 921 | 2 152 |
| `linux-3263301353` (zenoh) | 10 | 6.21 G | 0.86 G | 3 998 | 500 |
| `linux-3000917972` (xrce) | 8 | 3.45 G | 0.58 G | 2 338 | 384 |
| `linux-553222167` (cyclonedds) | 5 | 2.34 G | 0.55 G | 1 558 | 366 |
| `linux-1147932602` (tls) | 2 | 1.23 G | 0.65 G | 796 | 420 |
| `linux-865285299` (zero-copy) | 1 | 0.63 G | 0.63 G | 392 | 392 |
| `linux-228170020` (large-buf) | 1 | 0.54 G | 0.54 G | 335 | 335 |
| **total** | **65** | **46.08 G** | **~6.95 G** | | |

**~39 GiB, an 84.9 % reduction, on one platform** — and the seven groups are not
a cost: six of them are the reason the migration is correct at all. The two
singleton groups save nothing by construction, which is the honest shape of the
long tail W2 described.

#### What is left to build

`fixture_shared_target_dir(platform)` must become
`fixture_shared_target_dir(platform, variant)`. The variant cannot be
re-derived in Rust (that is a second spelling of a `cksum`); shell into
`nros_fixture_group_slug`. The caller-side problem is that a resolver knows the leaf
and the binary but not the row: `build_example("native/rust/talker", "talker")`
and `build_example_rmw(…, Rmw::Zenoh)` distinguish variants today only by the
authored dir name (`target/` vs `target-zenoh/`), which is exactly the string
the group strips. So the mapping has to come from the manifest, not from the
call site. Two further specifics, both already recorded and still true: 0 of 65
`linux` rust rows carry a `--target`, so `require_shared_fixture_binary`'s
hardcoded `{triple}/` component is one directory too deep for a host build; and
`build_example` / `build_example_rmw` are the two funnels, not ~30 sites.

### Wave 2 — the lever nobody priced → **measured, and the premise failed; see [phase-343](phase-343-host-build-graph-duplication.md)**

> **Superseded 2026-08-08.** The 240.2 GiB / 91.1 % reproduces (241.3 / 91.1 on a
> tree three days newer). **Everything this wave said about it does not.** Kept
> unedited below so the reasoning that produced the wrong plan stays legible —
> this is the THIRD documented premise in this phase to fail under measurement,
> after F3 and the umbrella shape.
>
> * **"feature-INVARIANT, one identity each, so no group key is needed"** — false
>   as stated. **0 of 91** host-only crates carry a single `-C metadata`
>   identity; `syn` has 45, `winnow` 32, `toml_edit` 31. The conclusion happens
>   to survive, for a different reason: identities coexist in one dir *because
>   the hash is in the filename*, not because there is only one of them. That
>   reason applies to the PRODUCT half too — which settles **W2.a's A2 blocker
>   in favour of the coarse platform-grained key**, without needing the lane.
> * **"one shared host target-dir across leaves"** — cannot be written. Cargo's
>   `--target-dir` is "Directory for all generated artifacts"; there is no
>   host-scoped target dir, as a flag, a config key, or behind `-Z`. The host
>   half is an output LAYOUT, not an input knob, so it can only be shared by
>   sharing the whole dir — which is arm B, already decided above.
> * **The mass is not where this wave looked.** 32 % of it (**76.8 GiB**) is
>   inside 425 nested target dirs that BUILD SCRIPTS create — the sizes probe
>   (63.1 GiB, 98.8 % duplicate, 81:1) and the cmake metadata probe (13.7 GiB).
>   Their sharing mechanism already exists, is already keyed correctly, and
>   landed as `82b31d08e` on 2026-08-04; it leaks because it is opt-in through
>   one exported variable in `scripts/build/cargo.sh`. That is a wiring fix.
> * The remaining ~160 GiB is the leaf + corrosion population, which is **this
>   phase's arm B applied to more platforms** — Wave 1, not a new wave.
>
> Net: there is no separate Wave-2-shaped phase to build. phase-343 carries the
> decomposition, the rejected mechanisms with their evidence, and the probe-dir
> work items.

Target the host proc-macro / build-dep graph directly — one shared host
target-dir across leaves, or sccache tuned for it. Deserves a phase doc of its
own because the prize is roughly an order of magnitude above this phase's stated
goal AND the mechanism is different in kind: these units are feature-invariant,
so no group key is needed. Measurement-first, like W2: establish what a shared
host dir actually saves before building anything.

### Wave 3 — make tier 2 honest AND cheap — **LANDED 2026-08-08**

Issue 0482 fixed correctness (tier 2 selected 46 of 240 rows; now 109) but not
affordability: tier 2 only passed on top of an `all` build, so the ladder's
middle rung was a fiction.

W3 narrows the RUN so a `lane=tier2` build suffices. Of 0482's two candidate
designs it ships **design 2 (filter at resolution time)**; design 1
(`lane-filter.sh`) stays a dead end for the reason 0482 gives — tier 2 is 1-wise
over platform, so every platform is in the lane, platform-token filtering
excludes nothing, and the saving is in lang × rmw *within* a platform, which
test names do not encode (issue 0357).

**The blocker on design 2 was about the ~30 hand-written resolver functions, not
about the resolver.** Every one of them computes its path under the manifest
row's own artifact root — necessarily, because that is where the build wrote it.
So the link back to the row is DERIVABLE:

* `fixtures-manifest.py` gained `row_artifact_root()` beside `row_coord()`, one
  expression shared with the cmake record's `build_subdir`, so where the build
  WRITES and where attribution LOOKS cannot drift.
* `nros_tests::fixtures::lane` inverts it at the two resolution chokepoints
  (`require_prebuilt_binary`; `require_prebuilt_workspace_binary` by `id`, since
  several workspace rows share a `dir`). No per-resolver edit — that would have
  been the #328 shape the objection named.
* Measured over the manifest as shipped: **all 240 buildable `[[fixture]]` rows
  have distinct, pairwise-unnested artifact roots**, so the inversion is exact
  rather than heuristic, and a gate keeps it so.

The result is the invariant this whole area exists to protect — one predicate,
one coordinate file:

```text
BUILD skips row R  ⟺  row_coord(R) ∉ lane_coords   (fixtures-manifest.py --coords-from)
RUN   skips row R  ⟺  row_coord(R) ∉ lane_coords   (fixtures::lane)
```

`CiLane::run_scope` gained `RunScope::LaneCoords`; `nros_lane_build_lane` maps
`tier2`/`tier2-nightly` to themselves; the recipes export `NROS_TEST_COORDS`
naming the same `nros_lane_coords_file` output the build and the staleness gate
already use.

**Why this is not the 0445 laundering hazard.** The skip is keyed on "this row's
coordinate is outside the lane" and never on "the artifact is missing". An
in-lane fixture that is absent or stale fails exactly as hard as before; a path
no row claims is never skipped (the Zephyr west leaves, the compile-check lane
and the shared `build/fixtures-cargo` dirs are built module-level, so a lane
omits nothing there); an empty or unreadable `NROS_TEST_COORDS` is a hard error,
not "no narrowing".

**Verified.** Gate level: `tests/lane_run_narrowing.rs` (build-set == run-set
over four coordinate subsets for BOTH row kinds; attribution totality;
fail-closed; the skip decision in both directions) plus the unnested-root and
component-wise-containment units in `fixtures::lane`. Seven tripwires, each
confirmed to turn its gate red and then restored. Process level, on an
UNPROVISIONED tree — which is the only lane check a worktree agent can make:
with `linux,rust,zenoh` out of the lane a resolver reports `[SKIPPED] out of
lane …`; with it in the lane the same resolver still reports `Test fixture
binary not prebuilt`; with no `NROS_TEST_COORDS` behaviour is unchanged.

**NOT verified, and it is the acceptance criterion.** `just build-test-fixtures
lane=tier2 && just ci-matrix` has not been run — a fresh worktree is
unprovisioned and the volume had ~20 GB free. The post-merge `lane=all` +
`ci-matrix` this phase already mandates should be a `lane=tier2` + `ci-matrix`
this time, read from `target/nextest/default/junit.xml` and not the console
count (which reports `skip!` panics as failures — and W3 deliberately produces
many more of them). Expect the ~12 standing failures of Wave 4 minus whichever
sit at out-of-lane coordinates.

### Wave 4 — standing debt

* The ~12 real `ci-matrix` failures (junit, not the console count): ~3 fixture
  coverage, ~6 readiness marker, ~5 delivery assertions (zephyr cortex-m, rtic,
  riscv-nuttx). ALL predate this work — the count went 19 -> 12 across the
  wave-0 merges.
* Issue 0481's readiness-marker conversion (0480 is the site audit; 0481 owns
  the class and has the measurement).
* Issue 0472's 13 unguarded opaque macros.

### Two process rules this phase paid for

* **Parallel worktree agents verify at GATE level only** — a fresh worktree is
  unprovisioned, so no agent can build a fixture or run a sweep. A post-merge
  fixture build + `ci-matrix` is mandatory, and it is what caught the
  `enforce_registry` regression that four green `check-fast` runs did not.
  Since W3 that build is `lane=tier2`, not `lane=all` — and the FIRST such run
  is also W3's own acceptance check.
* **Re-measure an "N of M" claim before building on it.** F3 stood for months on
  evidence that varied `incremental` rather than sharing.
* **A gate cannot vet a change to the thing it is keyed on — apply the change and
  read the gate's MESSAGE, not its exit code.** W2 asked "does a platform-grained
  key collide?" of a gate whose collision key was coarser than the question, and
  got "no". The tell was in the passing message: *61 rows* for two platforms that
  carry 85. Both counts a gate prints are assertions; check them. (This is the
  second defect in this same gate found by reading a message rather than an exit
  code — the first is recorded in W2.a.)

## Work order (both phases)

**phase-334 and this phase are one program on two axes**, and their work items
had begun to overlap: 334's W3.a was this phase's W2, and its W3.b was this
phase's W3. Two spellings of one mechanism is the drift RFC-0070 R3 forbids for
paths, applied to work items. Resolved 2026-08-07:

* **phase-334 owns WHERE a cache lives and what it is called** (RFC-0070) —
  W2.b, W2.c.
* **this phase owns WHAT gets compiled and how often** — and absorbs 334's W3.a
  (→ W2), W3.b (→ W3) and W3.c (→ **W7**, new).

They meet at one point: a grouped build needs a derived path to write to. That
is why 334 W2.b comes before the grouping work here.

| # | item | why here |
| --- | --- | --- |
| ~~1~~ | ~~**340 W4 follow-up**~~ | **DONE** `ee016145a` — the find prunes `*/out/sizes-probe-target-*`; verified against a replica, and against the pre-fix script to show the prune is load-bearing |
| ~~2~~ | ~~**340 W5.b / W5.c**~~ | **DONE** 2026-08-07 — a straight deletion, not a feature gate: 181→165 units, overlap 12→8 |
| ~~3~~ | ~~**340 W6 step 1**~~ | **DONE** 2026-08-07 — not the remap: `incremental = false` on `dev`. 115/143 rlibs byte-identical, incremental state 185 MB → 1 MB per fixture |
| ~~4~~ | ~~**334 W2.b steps 2–4**~~ | **DONE 2026-08-08** — step 2 complete: four more families + a four-site tail, `git grep` for rooted literals now returns nothing. Steps 3-4 merged into item 5 (below), since the path changes there anyway. Original note: **DECIDED 2026-08-07** — the source-relative class is NOT a separate pass: 128 of 137 authored manifest paths are reproducible from (kind, platform, rmw) and the other 9 from the feature signature, so the column is DELETED, not derived. That merges into item 5. What remains here is the ROOTED side only (R3, one spelling) |
| ~~5~~ | ~~**340 W2**~~ **DONE 2026-08-12** — the shared group dir landed (B2/B3) and the manifest column it absorbed is deleted (issue 0517). Original: **mechanism DECIDED 2026-08-08 — one shared `--target-dir` per group, NOT the umbrella** (measured: same bytes, no wall-clock regression, no generated state). Blocker 1 of 2 cleared (`3ebc32110`, artifact-name collisions). Blocker 2 (the Rust resolver's group key) **settled 2026-08-08: the platform-grained shortcut is refuted (17 artifact collisions), so the variant slug gets built**. Prize measured at 46.1 → ~7.0 GiB on `linux`. Still absorbs the manifest `target_dir` / `build_subdir` column |
| ~~6~~ | ~~**340 W3**~~ | **DONE for the cmake lane 2026-08-08** (`c1cec0ef4`) — direction decided by measurement: EXPLICIT-ALWAYS, because corrosion hardcodes `--target` and is not ours to fork, and because the explicit spelling costs zero extra units (165 = 165). Three generators that could still emit the implicit spelling now share one resolver that cannot return empty; gated by `check-cargo-target-spelling`. The cargo-LEAF half is deliberately left to item 5 — a 58-site / 104-literal path move (re-derived 2026-08-10). **Its pre-registered criterion — the identity gate 6 -> 3 — was measured unreachable 2026-08-10: the gate's tree contains no cargo leaf build, and 4 of the 6 are the proc-macro host graph, not a `--target` spelling. See "The leaf half, re-measured".** The cmake half is now REBUILD-verified on the native lane |
| ~~7~~ | ~~**334 W2.c**~~ **WITHDRAWN** — not blocked: the per-leaf `.gitignore` files are the copy-out contract (RFC-0070 R1, amended 2026-08-10), so collapsing them is the wrong move whether or not paths have moved. Original: collapse `.gitignore`, once (4) has moved the paths — **BLOCKED: no path has moved.** Re-confirmed 2026-08-08: `build/fixtures-cargo/` holds one entry, against 116 live per-leaf target dirs (64 `target/`, 52 `target-*/`). Every ignore line still names live output |
| ~~8~~ | ~~**340 W7**~~ **DONE 2026-08-12** — [data](data/phase-340-w7-remeasure.md): wall 6794 s -> 581 s (11.7x) at 72 fixtures; budgets verified at the truth, so none lowered. Original: re-measure both axes against phase-331's pair (was 334 W3.c). Budgets: `nros_core` 8 -> 4 and the ceiling 12 -> 6 landed 2026-08-10 (P1), then 6 -> **5** the same day on a REBUILT tree — the 6th was pre-W3 residue, not a compilation. Gate now reads `nros_core 4/4; worst crate 5/5; worst identity 5/5`. The remaining 5 -> 3 is item 5's (R2), not W3's — but **not as a path-spelling edit**: measured 2026-08-10, cargo's `path` field records member-vs-foreign-path-dep, so 5 -> 3 costs a corrosion ROOT (see "R2 re-measured — `path` is a RELATION, not a spelling"). Budget stays 5 |

(1) and (2) are small and unblock reading the gate honestly. (3) is the biggest
win needing no restructuring. (4) unblocks every path move. (5) and (6) are the
restructuring proper and cannot precede the derivation. (7) is cleanup, (8) is
proof.

**Sequencing hazard.** (5) and (6) both change what a build writes and where.
Landing either before (4) means editing literals that (4) is about to derive —
the 236-literal problem, re-created. Landing them together means two path
conventions in flight at once, which is what makes a family's build, staleness
probe and test resolver disagree (#393). One at a time, each with its own
measurement.

### W7 — re-measure both axes (was phase-334 W3.c)

- [x] **DONE 2026-08-12** — `docs/roadmap/data/phase-340-w7-remeasure.md`.
      Against W5, at the same 72 fixtures and 0 errors: wall **6794 s -> 581 s**
      (11.7×), native stage **5222 s -> 342 s** (15.3×), seconds-per-fixture
      **72.5 -> 4.8**. Two runs are recorded because the first was not steady
      state; the second is 37 % faster again and is the figure.
      **Read the caveat with the number:** the method is identical (wipe the
      manifest-declared workspace trees, leave cargo alone) but "leave cargo
      alone" changed meaning — W1/W5's warm state was ~116 per-leaf `target*`
      dirs, this one's is 18 group dirs that actually get reused. That IS the
      change being measured, and it also means a cache-cold tree would show a far
      smaller ratio, which this does not bound.
- [x] **DONE 2026-08-12 — and the answer is "no change", which is a result.**
      On a freshly rebuilt tree with the `started_at` filter live (245 of 245
      rlibs counted): `nros_core 4/4; worst crate 5/5; worst identity 5/5`. Every
      budget already equals the truth, so lowering any would make the gate fail
      on the tree it is supposed to describe. The remaining `5 -> 3` is R2's and
      was already measured as costing a corrosion ROOT, not a path edit.

**Acceptance:** MET. Both sections are restated in
`docs/roadmap/data/phase-340-w7-remeasure.md`; the budgets are verified at the
truth rather than lowered, because they already are it.

**Two findings the re-measure produced that the phase did not go looking for:**

* **The dominant disk class is no longer this phase's.** `examples/` is 402 ->
  306 GiB and the five-target-dirs-per-leaf pattern is gone, but classifying
  what remains: the cmake **metadata probe** holds **108 dirs / 82.4 GiB**
  against 28.9 GiB of example-leaf residue. `nros metadata --build` gives every
  component a private cargo target dir; 162 such trees hold 312 `libnros_core`
  rlibs with **16 distinct identities**. That is this phase's thesis on a build
  path this phase never touched — filed as issue 0522, not fixed here.
* **A cold build is the only routine check on the workspace-runtime graph.** The
  first attempt failed outright: `eyre::Context` is `#[cfg(feature = "anyhow")]`
  from eyre 0.6.13, and that graph resolves fresh while `packages/cli/Cargo.lock`
  pins 0.6.12 — so every warm build and every `just check` was green while the
  path was broken. Fixed (66 call sites to `WrapErr`) and gated
  (`check-eyre-context-alias`) before the timed run was repeated.

## Work items

### W1 — decide `incremental` for the shared profile

- [x] A/B `just build-test-fixtures lane=native` with `incremental` on/off in
      `[profile.nros-relwithdebinfo]`, alternating reps as above. Record BOTH
      wall-clock and the target-dir bytes each arm leaves behind.
- [x] If it holds at lane scale, drop `incremental = true` from that profile and
      give interactive work a separate profile that keeps it (the local-iteration
      case it actually serves). **Done 2026-08-06**: `incremental` dropped from
      all three copies of `nros-relwithdebinfo`; local iteration is the new
      `nros-iterate` preset, selected by `NROS_CARGO_PROFILE=nros-iterate`.

The profile is defined in **THREE** places, kept in agreement by two tests in
`packages/tooling/nros-cargo-profile/src/lib.rs`:

| copy | why it exists | gate |
| --- | --- | --- |
| `RELWITHDEBINFO` preset in `nros-cargo-profile` | cmake/bash/tests read it without parsing TOML | — (the SSoT) |
| `[profile.…]` in the root `Cargo.toml` | a bare `cargo build --profile` in this repo | `root_manifest_matches_this_table` |
| `[profile.…]` in the root `.cargo/config.toml` | a bare `cargo build --profile` in any LEAF, via cargo's config walk-up | `config_toml_matches_this_table` |

The third is the one that matters here and is the easiest to miss. Leaf
workspaces carry no `[profile.*]` block of their own; they resolve the name
through the config walk-up, so **editing the manifest alone changes nothing for
any leaf** — and the gates will fail the build before a half-applied edit can
be measured, which is the intended outcome.

For MEASUREMENT, do not edit any of the three. `CARGO_INCREMENTAL=0` in the
environment overrides the profile's `incremental` setting for every cargo in the
run, so both arms differ by exactly one variable and neither needs an `nros`
rebuild (which would stale every workspace fixture and confound the timing).
Edit the three copies only once the answer is known.

Verified that this override survives the cmake path, which is the one that could
have defeated it: `nros_cargo_profile_env()` injects
`CARGO_PROFILE_NROS_RELWITHDEBINFO_INCREMENTAL=true` explicitly for corrosion
targets, and `CARGO_INCREMENTAL=0` still wins — the `incremental/` directory is
created but stays empty (4096 B), and the target dir comes out at the
incremental-off size. Without this check the lane's "off" arm could have been
measuring the status quo under a different name.

**Acceptance:** the lane's wall-clock difference AND its disk difference are
measured and recorded, and whichever way it goes, the reason is written at the
profile — in all three spellings.

#### W1 result — single leaf (2026-08-06, 20-core host)

`examples/native/rust/talker`, 112 crates, `target/` removed before every run,
three alternating reps, sccache live. Steady state (reps 2–3, warm cache):

| arm | wall | `target/` | of which `incremental/` | sccache hits / misses |
| --- | --- | --- | --- | --- |
| profile `incremental = true` | 5.5 s | 649 MiB | 178 MiB | 98 / 1 |
| `CARGO_INCREMENTAL=0` | **5.0 s** | **482 MiB** | 1 MiB | 124 / 3 |

Cold-cache first rep: 8.8 s vs 5.0 s. Disk reproduced to within a few KB across
reps within each arm (674.01–674.03 MB incremental-on, 500.64 MB off) — this
factor is stable, unlike the timing.

**Incremental costs ~10 % wall-clock and ~35 % disk here.** The direction matches
the phase doc's premise; the magnitude does not — the doc's "~37 % slower" does
not reproduce at this scope, so treat that figure as scope-specific rather than
a property of the setting.

#### The sccache question, answered

The doc listed "incremental makes Rust compilations non-cacheable" as the
obvious hypothesis and the next experiment. It is **wrong**, and the truth is
sharper:

* `non_cacheable_compilations` was **0** in every arm. sccache caches
  incremental builds fine.
* What changes is the number of units it sees: 99 requests with incremental on
  versus 127 with it off. Fewer compilations reach the cache, so the warm-cache
  win is smaller — 98 hits versus 124.
* Separately, and much more sharply: **sccache 0.8.2 refuses to run at all when
  the `CARGO_INCREMENTAL` environment variable is set to 1.** It aborts during
  the `rustc -vV` probe with `sccache: increment compilation is prohibited` and
  the build exits 101 having compiled nothing.

That last point is a trap for anyone measuring this. `CARGO_INCREMENTAL=1` and a
profile's `incremental = true` are **different inputs**: cargo does not set the
env var for a profile setting, it passes `-C incremental=<dir>` on the rustc
command line. So the env var is not a louder spelling of the profile setting,
and an A/B that uses it for the "on" arm measures a build that never happened.
The first attempt at this measurement did exactly that and had to be redone.

A second trap: `command -v sccache` finds nothing unless `activate.sh` has been
sourced, so a harness that sets `RUSTC_WRAPPER` from it silently runs every arm
uncached. The `sccache` columns above are only meaningful because the harness
sources `activate.sh` first — which itself needs `set +u`, since ROS's
`setup.bash` reads `$AMENT_TRACE_SETUP_FILES` unguarded.

#### W1 result — lane scale (`build-test-fixtures lane=native`)

Four runs, alternating, every native-lane target dir removed before each:

| rep | arm | wall | target dirs | sccache hits / misses |
| --- | --- | --- | --- | --- |
| 1 | profile (on) | 1882 s | 43643 MiB | 9626 / 1888 |
| 1 | off | 1927 s | 31600 MiB | 13692 / 5088 |
| 2 | profile (on) | 1366 s | 43643 MiB | 10874 / 76 |
| 2 | off | **1199 s** | **31600 MiB** | 17846 / 222 |

**Disk: −12043 MiB, −27.6 %, and identical to the MiB across both reps.** This is
the solid result. It scales past the single leaf and it does not depend on cache
state, machine load, or run order.

**Wall-clock: +2.4 % in rep 1, −12.2 % in rep 2.** The sign flips, which is
precisely the pattern the doc warns about — but here the cause is visible in the
miss counts rather than left as "warm caches dominated":

* Rep 1 is cache-POPULATING. The `off` arm sends more units to sccache (18780
  requests vs 11514) so it pays a bigger one-time population cost: 5088 misses
  vs 1888. It comes out 45 s slower.
* Rep 2 is cache-WARM, and the same property now pays: 17846 hits vs 10874,
  misses collapsed to 222 and 76. It comes out 167 s faster.

So the two reps do not disagree; they measure two different regimes. The
steady-state regime is rep 2, and its −12.2 % matches the single leaf's −10 % in
both sign and magnitude. **The honest reading is that dropping `incremental`
wins ~10–12 % once the compile cache is warm, loses ~2 % on the very first cold
run, and saves ~28 % disk unconditionally.**

`non_cacheable_compilations` was 0 in all four arms, across ~30 000 compilations
— the refutation above holds at lane scale, not just on one leaf.

**A flaw in this harness, stated so the numbers are read correctly.** The two
disk metrics have different scopes: the target figure is `-maxdepth 4 -name
'target*'`, the incremental figure is an unlimited-depth `-name incremental`. The
latter therefore counts `incremental/` dirs under `build/` trees the former never
visits, which is why it reads LARGER than the total it appears to be a share of
(46589 vs 43643 MiB). Both arms were measured identically so the delta is sound,
but the incremental column is not a subset of the target column and must not be
quoted as one. Fix the scopes before re-running.

#### Two hazards the new profile ran into, now gated

Adding the opt-in profile was not a one-line mirror of the old settings. Both of
these were found by shipping the obvious version first and watching it fail:

1. **A chained `inherits` does not survive env injection.** The natural spelling
   is `inherits = "nros-relwithdebinfo"`, but [`env`] emits ONE profile's
   settings, so outside this checkout — where the `.cargo/config.toml` walk-up
   does not reach — cargo fails `profile 'nros-relwithdebinfo' is not defined`.
   Presets must bottom out in a cargo builtin and repeat the settings.
   `presets_all_inherit_a_builtin` *claimed* to enforce this and did not: it
   checked only that some `inherits` key was present. Now it checks the value.

2. **The obvious NAME is ambiguous.** `nros-relwithdebinfo-incremental`
   uppercases to `CARGO_PROFILE_NROS_RELWITHDEBINFO_INCREMENTAL`, which is also
   `nros-relwithdebinfo` + its `incremental` key. Cargo resolves it as the
   latter and dies with `could not load config key
   profile.nros-relwithdebinfo`. Hence `nros-iterate`. The new
   `preset_names_cannot_collide_with_another_preset_key` compares each preset
   name against every preset crossed with **every cargo profile key**, not just
   the declared ones — the first version of that test compared declared keys
   only and happily passed the very name that had just failed, because
   `incremental` had by then been removed from the parent.

Both gates were tripwired: each fails on its own defect with the other reverted.

#### What the idle time actually is

Sampled every 15 s across all four runs (554 samples): **mean 2.8 `rustc`, 5.4
`cargo`, load 5.7 on 20 cores**, and **22.9 % of samples had ZERO `rustc`
running**. That corroborates the "0–2 rustc" symptom this phase is built on.

It also qualifies it. Part of that zero-rustc time is the serial `nros sync` /
`generate-rust` codegen prologue, which compiles nothing and which W2's
deduplication would not touch — the first ~3.5 minutes of every run were spent
there before the first `== native ==` banner. Before crediting W2 with the whole
gap, the prologue should be timed separately; the joblog records per-PLATFORM
stages only, so it cannot currently answer this.

### W2 — collapse the R1 duplicates

**Rewritten after W1.** The original plan — "share ONE target dir per identity
group" — names the right grouping and the wrong mechanism, and it would make the
lane slower for a benefit sccache already delivers. Three findings force the
revision.

#### F1 — CPU duplication and DISK duplication are different problems

The phase opened by treating "106 rlibs, 5 identities" as 106 compilations. With
a warm sccache it is not: rep 2 of the lane A/B recorded **17846 hits against
222 misses**. Those 106 are 106 *materialisations* of a handful of actual
compiles. "The machine is not compiling; it is repeating" is a **cold-cache**
statement, and the lane is cold only on a fresh machine.

Disk is the part nothing dedupes. sccache caches the compile and each consumer
still writes its own copy: 327 `libnros_core-*.rlib` on disk, 402 GB under
`examples/`. So W2's target is **bytes, not CPU** — and that changes which
mechanism is correct, because the two have opposite contention profiles.

#### F2 — the mechanism already exists, and R1 is opted OUT of it

`scripts/build/fixtures-target-dir.sh` (phase-226.D) already groups rows by
platform, triple, profile, no-default flag, sorted features, sorted env and sync
mode — essentially this phase's identity tuple — and hands the group ONE
`--target-dir`. It is gated to a single platform:

```sh
export NROS_FIXTURE_SHARED_PLATFORMS="${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal}"
```

and it explicitly yields to rows that author their own dir:

> the manifest row did NOT author its own `--target-dir` (authored dirs such as
> `target-zenoh` / `target-safety` win and stay per-example).

Those authored dirs ARE the R1 population this phase measures. R1 is therefore
not an unbuilt feature; it is an **opt-out**, and W2 is mostly a question of
extending an existing, proven resolver rather than writing a new one.

#### F3 — sharing a dir across CONCURRENT cargo processes is the wrong shape

> **REFUTED by measurement, 2026-08-08. Read "The mechanism, decided" below
> before acting on anything in this subsection.** F3 was a prediction from
> cargo's flock, never an experiment: the evidence phase-334 W1.a cites for it
> is this phase's W1 lane A/B, and that A/B varied `incremental` — it never
> varied target-dir sharing, in either arm. The serialisation F3 describes is
> real and reproduces exactly; the conclusion drawn from it does not. Kept
> unedited below so the reasoning that produced the wrong answer stays legible.

Cargo takes an **exclusive** lock on a target dir for the whole build. The
fixture fan-out is parallel (`run()` → `run_with_make`, unless
`NROS_JOBSERVER=1`), so pointing N concurrent rows at one shared dir converts N
parallel builds into N serial ones. Against a warm sccache — where the duplicate
work was already cheap — that is a **net loss**: it removes redundancy that cost
little and adds serialisation that costs a lot.

The alternative shape is the one to build: **one cargo invocation over N
packages**, not N invocations over one dir. A single invocation takes the lock
once, builds each identity once by construction, parallelises internally through
its own jobserver, and writes one copy to disk. That is the difference between
lock contention and inner parallelism, and it is the whole design question in
this work item.

#### The disk number this phase should have been quoting

Before the mechanism: the size of the thing. Read-only over the provisioned
checkout, 2026-08-08, 366 `target*` / `build*` dirs under `examples/`:

| | GiB | files |
| --- | --- | --- |
| total | **478.9** | 1 489 146 |
| `deps/` | 263.8 | 284 376 |
| build-script output | 165.1 | 476 892 |
| `incremental/` | 34.6 | 255 603 |
| `.fingerprint` | 0.1 | 342 770 |

Deduplicate `deps/` by artifact NAME — which is cargo's own judgement, since
`-C metadata` is *in* the filename — and 263.7 GiB of materialised artifacts
collapse to **23.5 GiB across 17 163 distinct names**. So **240.2 GiB, 91.1 % of
`deps/`, is duplicate identity.** That is the quantity W2 is about, and it is an
order of magnitude larger than anything else this phase has priced.

The top of that list is not what R1 predicts:

```
391x  11.4 MiB   4.34 GiB  libnros_macros-574421335b3823cb.so
174x  24.5 MiB   4.15 GiB  libnros_c.a
391x   9.8 MiB   3.72 GiB  libros_launch_manifest_model-3eb7ae0fc81a9650.rlib
391x   8.6 MiB   3.28 GiB  libsyn-20f98dae27aa346c.rlib
391x   7.0 MiB   2.67 GiB  libtoml_edit-234bebb8e17a39fb.rlib
512x   3.5 MiB   1.73 GiB  libwinnow-0a8647b48c0c8892.rlib
504x   2.9 MiB   1.40 GiB  libcc-831acfa26eb8eb1f.rlib
```

`syn`, `winnow`, `toml_edit`, `serde_derive`, `cc`, `cbindgen`, `nros_macros`,
both `ros_launch_manifest` crates — the **host build-dependency and proc-macro
graph**, one identity each, 391–512 copies. One hash, appearing in every leaf
regardless of that leaf's RMW: this block is **feature-invariant**, so the
largest single mass of duplicate bytes is precisely the part the RMW split does
not partition. It is W5's build-dep graph, seen on the disk axis instead of the
CPU axis, and no grouping key needs to be clever to catch it.

#### The mechanism, decided — three arms, measured 2026-08-08

**Set-up.** 37 generated clones of `examples/native/rust/lifecycle-node` built
`--no-default-features --features lifecycle-services` (117 packages, no vendored
submodule in the graph, so it runs on an unprovisioned worktree). Each clone is
a genuine standalone leaf — its own empty `[workspace]` table, its own
`.cargo/config.toml` patch table, its own lock — differing only in package and
binary name. That is a fixture group's shape: N standalone leaves that resolve
identically. 20 cores, sccache live, `NROS_CARGO_FLAGS=` (see below), arms
rotated *inside* each rep per the W1 method. The harness is deliberately out of
tree (`tmp/w2exp/`) — it is a measurement, not a fixture.

| arm | mechanism |
| --- | --- |
| **A** | N separate target dirs, N parallel invocations — **status quo** |
| **B** | ONE shared `--target-dir`, N parallel invocations — **phase-226.D, as widened by W2.a** |
| **C** | ONE umbrella workspace (generated symlink farm), 1 invocation — **W2.b's proposal** |

Wall clock at N = 37, the real size of `linux`'s default group:

| rep | A | B | C |
| --- | --- | --- | --- |
| 1 | 117.4 s | 26.6 s | 5.8 s |
| 2 | 82.7 s | 76.8 s | 11.5 s |
| 3 | 93.0 s | 54.7 s | 9.8 s |

Disk and identity, same 37 leaves:

| arm | target bytes | `nros_core` rlibs / identities | all `deps/` files / distinct names |
| --- | --- | --- | --- |
| A | **9.70 GiB** | 74 / 2 = **37.0 : 1** | 8214 / 294 = 27.9 : 1 |
| B | **455 MiB** | 2 / 2 = **1.0 : 1** | 294 / 294 = **1.0 : 1** |
| C | **455 MiB** | 2 / 2 = **1.0 : 1** | 294 / 294 = **1.0 : 1** |

N = 8 for scale: A 17.4 / 16.7 s, B 10.3 / 10.8 s, C 5.3 / 6.7 s; 2.10 GiB /
305 MiB / 305 MiB.

**1. F3 is refuted. A shared dir is not a net loss; it is a smaller win.** The
serialisation is exactly as described — instrumenting each invocation's own
elapsed at N = 8 shows them finishing 0.6 s apart in a staircase, which is the
flock — but B is **never slower than A in any rep at either N**, and averages
1.9× faster at N = 37. The reason F3 missed is that it priced the serialisation
and not what the serialisation *removes*: every invocation after the first finds
the whole shared graph already fresh in the dir and has only its own crate left
to build. Serialising work that no longer exists is cheap.

**2. B captures 100 % of the disk win. Not most of it — all of it.** B and C
agree to within 8 KiB on 455 MiB, and produce byte-identical dedup ratios
(294 / 294). Since F1 established that W2's target is bytes rather than CPU,
this is the criterion that decides, and it does not distinguish the two
mechanisms at all.

**3. C is genuinely much faster, and that buys nothing here.** One invocation is
~9× A and ~1.7× B at N = 37 — inner parallelism works exactly as RFC-0070 R4
says it does. But it converts a disk problem this phase has measured at 240 GiB
into a wall-clock saving on a lane whose wall clock W1 already improved, at the
price of a generated-state subsystem (below).

**Decision: W2 ships B.** It is a flag, not a subsystem — `phase-226.D`'s
resolver, already widened by W2.a step 1, already gated by
`check-fixture-groups`, with a Rust-side mirror that already reads the same env
var. C is recorded as a real and measured option, not rejected on principle;
revisit it if wall clock ever becomes the binding constraint, and re-price it
first, because two of the three costs W2.b listed were wrong in opposite
directions.

**One caveat on B's numbers.** Its wall clock is the noisiest column here
(26.6 / 76.8 / 54.7 at N = 37) because which invocation wins the lock first
decides how much overlaps before it. One standalone re-run of arm B at N = 8
came in at 28 s against four other observations of 9–11 s, and that outlier is
unexplained. B's *disk* figure, by contrast, reproduced to the kilobyte across
every rep — take B's disk as settled and B's timing as "not worse than A", which
is all the decision needs.

**The constraint that bounds group size: feature unification.** Cargo unions
features across workspace members built in one invocation (resolver 2 only
avoids unification across build-dep / dev-dep / target boundaries). So a group
may contain only rows whose feature set is IDENTICAL. Grouping `rmw-zenoh` with
`rmw-xrce` rows would union them — and because nano-ros has **no
`compile_error!` on multiple RMW features** (only on multiple ROS editions),
that union does not fail loudly. It silently builds every backend into every
fixture and changes what the tests are exercising. A silent behaviour change is
a worse failure than a build error, so the group key must keep features exact.

**How many groups are actually worth sharing — measured 2026-08-06.** Over the
117 `linux` fixture rows there are 60 distinct variant signatures, and the
distribution is bimodal:

```
 37 rows   (default features)
 10 rows   --no-default-features --features rmw-zenoh
  8 rows   --no-default-features --features rmw-xrce
  5 rows   --no-default-features --features rmw-cyclonedds
  2 rows   --features link-tls
 ---
 62 rows in 5 signatures   |   55 rows in 55 singleton signatures
```

**55 of the 60 signatures are singletons** — they can never share with anything,
and sccache is their only dedup. **Five signatures cover 53 % of the rows.** So
W2.b's target is those five groups specifically, not "same-identity groups" in
general: five umbrella builds replace 62 separate cargo invocations, and the
long tail keeps today's shape.

#### Relationship to phase-334 — the questions were framed there first

**F1/F2/F3 above are not new questions.** Phase-334 W1 asked all three before
this phase existed: W1.d ("if cache-hit builds get within ~15 % of shared-dir
builds, PREFER separate dirs + sccache"), W1.a (measure the target-dir lock's
serialisation cost, "report [phase-226's] measured numbers first"), and W1.b
(the feature-unification hazard and the signature count). This phase re-derived
them without noticing, and what it contributed is the **measurements**, not the
framing.

The verdicts are recorded in phase-334's status block, which now owns the W1
answers; phase-334 keeps W1.c and all of W2 (layout and naming), which this
phase does not touch. Keep the two in sync rather than restating: **334 owns the
sharing verdict and the layout rule; 340 owns the identity measurements and the
W2/W3 implementation.**

#### Work items

- [x] **W2.a** DONE 2026-08-08 — the group KEY was split from ELIGIBILITY, so an
      authored `--target-dir` names a group instead of opting the row out. Landed
      INERT (0 of 20 qemu-arm-baremetal rows author one) per RFC-0070's
      "paths last"; B3 then migrated `linux` and wave 2 four more platforms.
      Original text: Extend the phase-226.D resolver so a manifest-authored
      `--target-dir` names a GROUP rather than opting the row out, for rows whose
      identity already matches. No new mechanism; widen the existing key.

      **Blocker removed 2026-08-07 — the eligibility rule had TWO spellings.**
      The shell read `NROS_FIXTURE_SHARED_PLATFORMS`; the Rust resolver
      hardcoded `match platform { "qemu-arm-baremetal" => … }`. Widening the
      shell list alone would have made the BUILD write to the shared dir while
      the TEST kept looking in the leaf — every row of the new platform
      reporting its binary missing, with nothing naming the cause. That is #393
      verbatim, latent in the design phase-226.D shipped. The Rust side now
      reads the same env var with the same default, and
      `build_root_derivation.sh` asserts the two defaults match and that the
      Rust side still READS the variable (tripwired: diverging them fails).

      **What the widening is worth, measured.** Of 118 `linux` rows, **85 carry
      no cargo args at all** — they need only the platform gate, not the
      authored-dir change, and today each writes its own `<leaf>/target/`. The
      other 27 carry an authored `--target-dir` (10 zenoh, 8 xrce, 5 cyclonedds,
      2 tls, 1 zero-copy) and additionally need the authored flag STRIPPED from
      `cargo_args`, or cargo receives two `--target-dir` flags.

      **Step (1) LANDED 2026-08-08.** `nros_fixture_group_slug` splits the group
      KEY out of `nros_fixture_group`, which conflated it with ELIGIBILITY;
      `nros_fixture_target_dir_flag` no longer bails on an authored dir; and
      `nros_fixture_strip_authored_target_dir` removes the row's own flag in
      BOTH callers (`fixtures-build.sh`, `rust-fixture-stale.sh`). Inert at
      today's default list — no `qemu-arm-baremetal` row authors a
      `--target-dir`, so nothing moved (RFC-0070's "paths last"). Four new
      tripwired arms in `build_root_derivation.sh`.

      The split was forced by the gate below rather than chosen: a check on the
      preconditions for MIGRATING a platform has to ask "which group would this
      row land in?" for a platform that is by definition not migrated, and the
      old function answered "none".

      **Steps (2)/(3) are BLOCKED, and the blockers were not known.** Adding
      `linux` looked like a one-word edit; it fails two preconditions, now gated
      by `check-fixture-groups` (`check-fast`):

      * **Artifact-name collisions.** A group's members write into ONE flat
        `<group>/[<triple>/]<profile>/` namespace. Cargo hashes `deps/` by
        `-C metadata`; it does NOT hash the final artifact name. Measured over
        the whole manifest with the widened key, `linux`'s default group has two
        names claimed twice: `talker` (`native-rs-custom-transport-talker` vs
        `native-rs-talker`) and `listener` (same pair). Last writer wins and one
        test silently runs the other's binary — strictly worse than the
        missing-binary error #393 produced. `qemu-arm-baremetal` is
        collision-free, so phase-226.D has been protecting this invariant by
        luck for two phases. **Cargo does not save you here even in the umbrella
        shape**: measured, it emits `warning: output filename collision … this
        may become a hard error in the future` (rust-lang/cargo#6313) and
        builds anyway.
      * **The Rust resolver cannot express a variant group.**
        `fixture_shared_target_dir` returns `build_dir("fixtures-cargo",
        &[platform])`. Its own doc comment says a feature/env variant "would
        need an explicit mirror" — a comment, not a gate. `linux` produces SIX
        variant groups today, so the build would write
        `fixtures-cargo/linux-<cksum>` while the test looked in
        `fixtures-cargo/linux`. Note the mirror problem is real: the shell slug
        is a `cksum` of the variant signature, and reimplementing a checksum in
        Rust is a second spelling. Prefer shelling into
        `nros_fixture_group_slug` (nros-tests already shells into
        `fixtures-manifest.py` for `current_workspace_fixture_record`) over
        porting the hash.

      A third, smaller one that costs an hour if met by surprise:
      `require_shared_fixture_binary` hard-codes a `{triple}/` path component,
      because every migrated row so far cross-compiles. **0 of 65 `linux` rust
      rows carry a `--target`**, so a host build writes
      `<group>/<profile>/<bin>` with no triple component and the existing
      resolver would look one directory too deep.

      So the order is now: fix the two colliding binary names → teach the Rust
      resolver the variant slug and the no-triple case → add `linux` → rebuild
      the native lane. Only the last of those needs the lane.

      **Step 2 of that order LANDED 2026-08-08** (`3ebc32110`). The two binaries
      are `custom-transport-talker` / `custom-transport-listener`; renamed rather
      than recorded, so `KNOWN_COLLISIONS` is now empty and that is its end
      state. Recomputed at platform granularity as well: **0 collisions across
      all 7 platforms / 122 rust rows**. Tripwired both ways — the record
      populated against a fixed tree fails "observed: []", and an empty record
      against a deliberately re-collided `native-rs-xrce-serial-talker` fails
      "observed: [1 entry]". With it in, `NROS_FIXTURE_SHARED_PLATFORMS=
      "qemu-arm-baremetal linux"` no longer trips A1 at all.

      ~~**The A2 blocker is probably smaller than it looks…**~~ **REFUTED
      2026-08-08 (W1). The platform-grained key does not work, and the reason is
      not feature unification at all.** The paragraph below is kept unedited
      because the reasoning that produced the wrong answer is the useful part —
      it is correct about cargo and wrong about what a group's output namespace
      is. See "W1 — the platform-grained key, refuted" for the measurement.

      > The variant sig is in the group key because an umbrella invocation would
      > UNION features across its members; arm B never does, so under arm B the
      > key can be platform-grained and `fixture_shared_target_dir`'s existing
      > `build_dir("fixtures-cargo", &[platform])` is already the right answer.
      > The namespace half of that is checked: at platform granularity `linux`'s
      > 41 rows collide on nothing. The churn half is phase-334 W1.c's ~6 %.

      The load-bearing error is in "the namespace half is checked". It was
      checked with `check-fixture-groups`, whose A1 arm keyed its owner set on
      the leaf DIRECTORY — so `linux`'s 65 rows collapsed to 41 dirs before any
      collision logic ran, and the four rows of `examples/native/rust/talker`
      (default, `rmw-zenoh`, `rmw-xrce`, `link-tls`) counted as one owner of the
      name `talker`. **41 rows was the tell**: the platform has 65.

      **So the alternative is now the only path: teach the Rust side the variant
      slug** by shelling into `nros_fixture_group_slug`, as
      `current_workspace_fixture_record` already shells into
      `fixtures-manifest.py`. It is strictly more work, and the measured prize
      below says it is worth doing.

      **Reconciling phase-343's parallel finding (both are right, about
      different namespaces).** phase-343 concluded the coarse platform-grained
      key is "semantically sound" because distinct identities coexist in one
      directory by construction — `-C metadata` is in the `deps/` filename, and
      17 195 distinct artifact names currently share 366 directories without
      colliding. That is TRUE OF `deps/` AND IRRELEVANT TO THE DECISION, because
      a fixture consumes the FINAL artifact, `<group>/<profile>/<bin>`, whose
      name cargo does not hash.

      Measured on a two-feature probe crate, two sequential invocations into one
      target dir: `deps/` kept BOTH identities; `debug/probe` was replaced —
      different sha256, different behaviour, and NO warning, because cargo's
      `output filename collision` diagnostic only fires when a single invocation
      builds both, and a group is N invocations. Under the coarse key `linux`
      produces 17 such collisions.

      So the two findings do not conflict once the namespace is named: identity
      coexistence is a `deps/` property, and the group's output path is where
      the decision is made. phase-343's other handback — that the collision gate
      scans BINARY names while `libnros_c.a` sits unhashed at x174 across 30
      distinct sizes — stands, and widening it is still required before any path
      moves.


      **One lesson from building the gate, because it generalises.** Its three
      arms initially shared a filter: the collision INVENTORY skipped platforms
      already in the shared list, on the theory that the enforcement arm owned
      those. Running the gate's own tripwire and *reading the message* — not the
      exit code — showed the inventory reporting "observed: []" for `linux` and
      instructing the reader to delete two live blockers from the record.
      Following that would have erased the only written trace of the collisions.
      The gate still exited 1 throughout, which is exactly why the exit code hid
      it. **Arms of one gate must observe independently, and a tripwire has to be
      run in BOTH directions** — collisions present (must not claim stale) and one
      genuinely fixed (must claim stale). This defect existed because the arm had
      only ever been exercised in one of them.
- [x] **W2.b — NOT THE MECHANISM, and now formally CLOSED (2026-08-10).**
      The umbrella is IMPOSSIBLE as specified, not merely unattractive: 22/22
      example leaves carry `[workspace]` (that is how RFC-0026's copy-out promise
      is kept) and cargo hard-errors `multiple workspace roots in the same
      workspace`. Arm B captures 100 % of the disk win at 1/9 the complexity.
      Original heading: NOT THE MECHANISM. Superseded by "The mechanism, decided"
      above; kept for the shape analysis, which stands.** The umbrella works and
      is the fastest of the three arms, but it buys zero bytes over arm B and
      costs a generated-state subsystem. Do not build it to close W2.

      Original item: convert the FIVE head signatures (above) from N parallel
      cargo invocations into ONE invocation each over a build-time-only umbrella
      workspace — 62 of 117 linux rows. Leaves keep their standalone manifests
      (RFC-0026's copy-out promise); the umbrella is generated for the fixture
      build and never committed. The 55 singleton signatures are out of scope
      by construction: they have nothing to share with.

      **The five-head/55-singleton split is an artefact of the umbrella
      shape, and does not carry over to arm B.** Feature unification is what
      forces a group's features to be exact, and cargo unions features only
      across members of ONE invocation. Arm B is N separate invocations: each
      resolves its own features, gets its own `-C metadata`, and the variants
      coexist in `deps/` — measured directly in phase-334 W1.c (alternating two
      feature sets in one dir reused 139 of 149 units, ~6 % churn, and saved
      25 % disk). So under arm B the 55 singletons are **not** out of scope;
      they share the whole feature-invariant lower stack — which the disk
      measurement above shows is where the bytes actually are.

      **The literal shape is IMPOSSIBLE — measured 2026-08-08.** "Leaves keep
      their standalone manifests" and "one workspace over those leaves" are
      mutually exclusive. Every example leaf carries an empty `[workspace]`
      table (Phase 208.F1, which is *how* RFC-0026's copy-out promise is kept),
      and cargo refuses a member that is itself a workspace root:

      ```console
      $ cargo metadata          # umbrella whose members are two such leaves
      error: multiple workspace roots found in the same workspace:
        …/leafA
        …/leafB
        …/root
      ```

      **41 of 41** `linux` rust fixture leaf dirs carry that table, so this is
      the whole population, not an edge.

      **The shape that DOES work, verified in the same session:** a generated
      symlink farm under the build root. Each member dir holds a REWRITTEN
      `Cargo.toml` (the `[workspace]` table stripped, everything else copied)
      plus symlinks to `src/`, `generated/`, `package.xml`. Two members with
      colliding bin names built in ONE invocation into ONE target dir. What that
      shape costs, stated before anyone builds it:

      * It is **generated build state with a staleness input** — the rewritten
        manifests go stale whenever a leaf manifest changes. That is issue
        0196's class, so the farm's freshness has to be probed by the same
        derivation the build and the test resolver use, not by a sibling check.
      * `--locked` is injected project-wide by the `scripts/bin/cargo` PATH
        shim. A generated umbrella has no committed lock, so the driver needs a
        deliberate answer (generate then pin, or a scoped `NROS_CARGO_FLAGS=`),
        not an accidental one.
      * ~~Identity changes again by R2~~ — **did NOT happen, measured
        2026-08-08.** The farm and the standalone leaves produced the *same two*
        `libnros_core-<hash>.rlib` identities (`8173b710b3981013`,
        `b1294a2e7ccd3459`), so an umbrella artifact and a leaf artifact were
        directly comparable after all. The correction generalises past this
        bullet: **R2's mechanism is the RESOLUTION, not the workspace-root
        path.** Leaf-vs-root changed identity in the incompatibility table
        because the root workspace resolves different versions and features, not
        because cargo puts the root's path in `-C metadata`. An umbrella whose
        members resolve what the leaves resolved therefore shares with them; one
        whose unified lock picks different versions does not, and that is a
        property of the members, not of the shape.
      * ~~One suspected blocker that is NOT one: 0 of 41 linux leaves carry
        their own `.cargo/config.toml`~~ — **that measurement was taken on an
        unprovisioned worktree, where the file does not exist yet.** On any tree
        that can actually build, **22 of 22** native rust leaves carry one:
        gitignored, written by `nros sync`, holding a leaf-RELATIVE
        `[patch.crates-io]` plus `include = ["…/nros-patch.toml"]`. And the
        repo-root `.cargo/config.toml` has **no `[patch.crates-io]` at all** —
        so the walk-up this bullet relied on does not exist, and without the
        leaf's own file `nros-log = "*"` resolves against public crates.io,
        which is #378's class.

        Cargo reads `.cargo/config.toml` from the **invocation directory**
        upward, so a farm member's config is never consulted whatever depth it
        sits at: the umbrella root must carry a **merged** patch table covering
        every member — including each member's *optional* backend deps, which
        must resolve even when the feature is off. That is a second spelling of
        `nros sync`'s output, and RFC-0070 R3 exists to forbid exactly that. It
        is the largest of C's costs and it was recorded as a non-cost.
- [x] **W2.d — DONE 2026-08-12.** Predicate half `cf8d2d18e`; column deleted in
      `eac40ab0d` via issue 0517. The blocker recorded below was real but the
      framing was wrong, and the correction is worth carrying: **all 124 cargo
      rows are shared**, so every one builds into `build/fixtures-cargo/<slug>`
      and none writes to its leaf `target*` at all. The column was not a
      location — it was a KEY encoded as a path, alive only because
      `require_in_lane` re-derived the row FROM that path. Making the lane check
      take the ROW removed the need for it entirely; the group slug is
      byte-identical for all 124 rows before and after, so nothing moved on disk
      and no rebuild was needed. The `target-<slug>` derived suffix proposed
      below was never built, and should not be: it would have kept the encoding
      and merely derived it. Original text follows.

      *Done.* `--core-only` no longer reads `target_dir` as a proxy for "is this
      a variant row?". `row_is_variant()` derives it from AUTHORED configuration
      (`features` / `no_default_features` / `env` / `cargo_args`) and joins
      `row_coord` / `row_artifact_root` as one more single derivation over the
      row (R3). `rmw` is deliberately excluded: it is a coordinate, so a
      default-rmw row is core. Held by `core_only_predicate.sh`, which derives
      the CONSUMED platforms from the tree (a new caller joins the check on its
      own) and fails if the two spellings disagree on any of them. The 2026-08-08
      note above expected them to diverge on the whole `qemu-arm-nuttx` rust set;
      they do — which is why the gate is scoped to platforms a caller actually
      passes rather than to the whole manifest, and why the nuttx divergence is
      the gate's own tripwire.

      *NOT done, and no longer believed mechanical.* I wrote in `cf8d2d18e` that
      removal "is mechanical, but it moves 39 rows". That is wrong. The column is
      the resolver's only handle on WHICH VARIANT a hardcoded leaf path means.
      `fixtures::groups::attribute()` maps a leaf path to a group slug by longest
      match on `row_artifact_root`, and today that is injective — measured over
      the 124 emitted rows, **0 artifact roots map to more than one slug**. Strip
      the authored dir and **17 roots become ambiguous, up to 5 slugs each**:

          examples/native/rust/talker/target
            -> linux, linux-1147932602, linux-3000917972,
               linux-3263301353, linux-553222167

      `attribute()` returns exactly one row, so this does not fail loudly — it
      resolves to a binary built with DIFFERENT features and the test runs
      against it. A false green, which is the one failure mode this phase's
      acceptance rule (a build, never a gate) exists to catch.

      So the order is fixed, and it is the reverse of what this item assumed: the
      resolver must name a COORDINATE before the column can go. That is issue
      0482's "the resolver has no link back to the manifest row" — the same
      handle `binaries/mod.rs` hand-spells as `target-tls` / `target-fixtures` /
      `target-large-buf`. W2.d's remaining half is therefore blocked on that,
      not on a rebuild lane.

      Original text: Delete the manifest `target_dir` / `build_subdir` column (absorbed
      from phase-334 W2.b, work-order item 5). Not done here: the column is still
      read as a **predicate**, not only as a path. `fixtures-manifest.py`'s
      `--core-only` (issue #29) treats "authored `target_dir`" as "is this a
      variant row?", and the obvious replacement is not equivalent — measured
      over the 122 rust rows, 30 author a `target_dir` while 37 have a non-empty
      variant signature, the 7 extra being the whole `qemu-arm-nuttx` rust set
      plus `logging-smoke-nuttx-qemu-arm`. Swapping the predicate would silently
      drop those from the host-integration lane, so it is a decision, not a
      mechanical substitution. The other consumers are `fixture-inventory.py` and
      the `binaries/mod.rs` resolvers that spell `target-tls` / `target-fixtures`
      / `target-large-buf` directly, and those move only when the paths do.
- [x] **W2.c** DONE 2026-08-08, and it is what killed W2.b: three arms over 37
      leaves gave separate-dirs 9.70 GiB, shared-dir 455 MiB, umbrella 455 MiB —
      arm B and the umbrella agree to 8 KiB, so the deciding criterion does not
      distinguish them. Original text: Measure W2.b against the status quo,
      alternating reps per the W1 method.

**~~Rejected design, recorded so it is not re-proposed~~ — UN-rejected
2026-08-08, and it is now the chosen mechanism.** The rejection below asked any
future proposal to "first show a cold-cache scenario". That test was aimed at
the wrong axis and has been answered on the right one: with a **warm** cache, at
the real group size, this shape is 1.9× faster than the status quo and removes
21.8:1 of duplicate bytes. What follows was the reasoning, and it was never
measured:

> N concurrent cargo processes sharing one target dir. It serialises on cargo's
> exclusive flock while sccache had already made the duplicate compiles cheap.
> Any future "just point them at the same dir" proposal must first show a
> cold-cache scenario.

The flock is real; sccache making the duplicate *compiles* cheap is real. What
neither covers is that sccache does not make the duplicate **materialisations**
cheap — 37 leaves each still write their own copy of the whole graph and link
their own binary, which is the 82–117 s and the 9.70 GiB in arm A.

**Considered and not taken:** post-build hardlink dedup of identical artifacts.
W1 restored byte-reproducibility so this is now *possible*, but the volume is
**ext4** — no reflink, so it would be true hardlinks, and any tool that rewrites
an artifact in place instead of replacing it would corrupt every sibling. Revisit
only with a measured list of what writes into these trees.

**Acceptance:** `nros-core` rlib count drops from 106 toward the identity count,
the native lane's wall-clock does not regress, and the target-dir bytes for the
grouped leaves drop by roughly the share the duplicates accounted for. The disk
figure is the one that proves the duplicates are GONE rather than merely
counted differently — an rlib count can fall because the probe changed.

**Status against that acceptance, 2026-08-08.** All three criteria are met *by
the mechanism, on a controlled 37-leaf group* — 37.0:1 → 1.0:1 on `nros_core`,
27.9:1 → 1.0:1 on all of `deps/`, 9.70 GiB → 455 MiB, and no wall-clock
regression in any rep. **None of them is met on the tree**, because no fixture
row has moved into a shared dir yet. W2 is therefore *decided* and not *done*,
and the remaining work is a platform migration (`linux`) that only a provisioned
tree can verify. Do not read the controlled numbers as tree numbers; that is the
distinction W7 exists to close.

### W3 — the corrosion `--target` split

- [x] Establish whether corrosion's explicit `--target` is load-bearing for the
      host-native case or incidental. **ANSWERED 2026-08-08: load-bearing, and
      not ours to change** — see "The direction is decided" below.
- [x] If incidental, align it so cmake-driven and cargo-driven host builds share
      one identity. If load-bearing, record WHY at the call site so the split
      stops looking like an accident. **DONE for the cmake lane 2026-08-08**
      (`c1cec0ef4`): the reason is written at
      `_nros_resolve_rust_target()` in `cmake/NanoRosCodegenCore.cmake`, and the
      three generators that could still emit the implicit spelling now cannot.
      **The cargo-leaf half remains OPEN** — scope and why it is deferred below.

#### The direction is decided: EXPLICIT-ALWAYS (2026-08-08)

Three measurements, taken in this order, and the third is the one that settles
it.

**1. The split is real and reproduces here.** `nros-core`,
`--no-default-features --features alloc,std`, `nros-relwithdebinfo`, one factor
varied:

| build | artifact |
| --- | --- |
| implicit host | `target/nros-relwithdebinfo/deps/libnros_core-0f6269f7a00e4b29.rlib` |
| `--target x86_64-unknown-linux-gnu` | `target/x86_64-unknown-linux-gnu/nros-relwithdebinfo/deps/libnros_core-842ac3b7840799eb.rlib` |

Note the second column carries the whole problem: `--target` is not only an
identity knob, it is a PATH knob.

**2. sccache does NOT bridge the two — measured, where it had only been
asserted.** The claim above ("a different `-C metadata` is a different cache
key") was reasoning about sccache's hashing. Against the shared host cache
every arm reads as a hit, because that cache has seen both spellings; the
question is only answerable on a cache that has seen neither. On a PRIVATE cold
sccache (own dir, own port):

| arm | `nros-core` | `nros` |
| --- | --- | --- |
| 1. implicit, cold | 0 hits / 7 misses | 0 hits / 62 misses |
| 2. explicit, same cache | **0 hits / 7 misses** | **0 hits / 62 misses** |
| 3. implicit again (control) | 7 hits / 0 misses | 44 hits / 18 misses |

Arm 3 proves the cache does serve a repeat, so arm 2's zero is the answer and
not a broken harness. **The split is duplicated CPU, not just duplicated
bytes.**

**3. Normalising toward explicit costs nothing in work done.** The obvious
objection to explicit-always is that `--target` splits the unit graph into a
host half and a target half, so shared crates get built twice.
`cargo --unit-graph` for `nros-c` (`std,rmw-zenoh`, `nros-relwithdebinfo`)
refutes it:

| | units | distinct compilation signatures | `platform=host` | `platform=<triple>` |
| --- | --- | --- | --- | --- |
| implicit | 165 | 160 | 165 | 0 |
| explicit | 165 | 160 | 128 | 37 |

Same count, same partition. Comparing the two unit multisets modulo the
platform label, the ONLY per-unit difference is `debuginfo`, which goes 0 → 1
on the 128 build-graph units: cargo stops stripping debuginfo from build
dependencies. That is the one measured cost of the explicit spelling, and it is
confined to the build graph. Wall clock on a cold private cache moved 7.3 s →
8.1 s for `nros`, a single rep and not a claim.

**So the direction is not a preference.** Corrosion hardcodes `--target` —
"We always set `--target`, so that cargo always places artifacts into a
directory with the target triple" (`Corrosion.cmake`) — because its artifact
path model IS `<target-dir>/<triple>/<profile>/`. It is an upstream dependency
this repo deliberately does not fork (`nros-sdk-index.toml` pins a stock
`v0.5.1`; the root `CMakeLists.txt` FetchContents stock `v0.6.1` as a
fallback). Implicit-always would mean carrying a patch on the cmake↔cargo
bridge in both provisioning paths. Corrosion is therefore the fixed point, and
everything else normalises TO it.

#### What landed: the cmake lane, as a class (2026-08-08)

The inconsistency was not only "cmake vs cargo". It was **inside one cmake
build**: `nros_generate_interfaces()` built the C++ FFI glue crate with

```cmake
if(DEFINED Rust_CARGO_TARGET)   # cross → --target
else()                          # host  → no --target
```

and on a native build `Rust_CARGO_TARGET` is NOT visible there — it is a normal
variable owned by whichever scope called `find_package(Corrosion)`. Verified on
a built native workspace: `src/*/nano_ros_cpp_ffi_*/target/` held
`nros-minsizerel/` with no triple directory, sitting next to corrosion trees
that had one. Five such trees in `examples/workspaces/mixed` alone.

Three generators carried the same "empty triple ⇒ omit `--target`" shape, and
all three now go through ONE resolver, `_nros_resolve_rust_target()`, which
cannot return empty (explicit `Rust_CARGO_TARGET`, else FindRust's CACHE copy,
else Corrosion's, else `rustc -vV`, else FATAL):

* `cmake/NanoRosGenerateInterfaces.cmake` — the native/corrosion lane
* `zephyr/cmake/nros_cargo_build.cmake` — the unknown-arch fallback
* `zephyr/cmake/nros_generate_interfaces.cmake`

Reading the CACHE copy also closes phase-155's bug class as a side effect: the
normal variable is published `PARENT_SCOPE`, which does not cross
`add_subdirectory()`, and a generator reading only it built host x86_64 objects
into an ARM link.

`_nros_ffi_cargo_args()` now REJECTS an empty `RUST_TARGET`, so the retired
spelling cannot return through a new caller. The one legitimate reason to omit
the FLAG — a generated `.cargo/config.toml` already carrying `[build] target`,
the NuttX path — is the explicit `TARGET_IN_CONFIG` option, which drops the
flag and KEEPS the triple, because the artifact still lands under it.

Gate: `check-cargo-target-spelling` in `check-fast`
(`packages/testing/nros-tests/tests/cargo_target_spelling.sh`). Buildless — it
configures a NONE-language cmake project against the module in four scopes and
asserts the "nothing readable" one FAILS rather than falling back. Tripwired:
reverting the FATAL reds the empty-`RUST_TARGET` arm; blanking the resolver's
fallback reds five arms.

Not verified on the Zephyr lane (no west workspace in the worktree this landed
from). The zephyr edits are symmetric — artifact path and flag key on the same
variable — and every KNOWN board already resolved a non-empty triple, so only
the already-warned unknown-arch fallback changes behaviour.

**Native lane verified by REBUILD 2026-08-10**, which the buildless gate could
not do: a fresh `scripts/build/workspace-fixtures-build.sh linux mixed` writes
its five `nano_ros_cpp_ffi_*/target/` glue trees with a `<triple>/` component
and NOTHING under `target/<profile>/deps` — where the pre-fix build had put the
whole five-crate lib graph. That is the generator change observed in its output,
not in its source.

#### What is still OPEN, and why it is not a "finish the job" away

The cargo-LEAF half. Every native example leaf and fixture row still builds
implicit, so `just check` at the root and a corrosion build of the same crate
still miss each other. Two reasons it is not next:

1. **It is a path move, not a flag flip.** `--target` relocates artifacts from
   `target/<profile>/` to `target/<triple>/<profile>/`. 45 sites call
   `cargo_target_profile_dir()` / `nros_cargo_profile::target_dir()`, and a
   wider grep for a hardcoded `target/<profile>/` segment across `just`,
   `scripts`, `packages`, `cmake` and `zephyr` returns 115 hits. That is the
   issue-0196 class, so it is a class-wide sweep or nothing.
2. **It buys nothing until R2 moves.** Corrosion resolves the shared crates
   through the ROOT workspace manifest (`nano-ros_0b88c` in every workspace —
   phase-334 W1.c). A native example leaf resolves through its OWN
   `Cargo.lock`, which is a different identity whatever the `--target` spelling
   is. So normalising the LEAVES changes their hashes without making them share
   with anything. The population that would actually start sharing is the
   root-workspace host builds, and moving those means moving the developer's
   `target/` — the same 115-site sweep, for a benefit W4 already showed is
   partial ("Fixing W3 alone would not make D build once for this user").

The honest sequencing is therefore: leave the leaf half to the same pass that
derives the paths (work-order item 5), rather than opening a second path
convention beside it (#393's shape).

#### The leaf half, re-measured 2026-08-10 — its success criterion was unreachable

The deferral above was re-opened on the ground that its stated precondition had
been met: wave 2 put 18+ leaves into shared `--target-dir` groups, so "it buys
nothing until R2 moves" no longer held. The pre-registered criterion was the
identity gate falling 6 -> 3. **Both numbers were re-derived first, and the
criterion turned out to be unreachable by this work item — for a reason that
had nothing to do with whether it lands.** Full decomposition in "P1 corrected"
above; the three facts, in the order they were established:

1. **The gate's tree holds no cargo leaf builds at all.** Every writer under
   `examples/workspaces/mixed/build-workspace-fixtures` is corrosion
   (`cargo/<root>_<h>`; `nros_ws_runtime` is a crate cmake SYNTHESISES into
   `CMAKE_BINARY_DIR` and imports with `corrosion_import_crate`) or
   `nros_generate_interfaces()` glue. Corrosion hardcodes `--target`. There is
   nothing in that tree for a leaf-side spelling change to normalise.
2. **Four of the six identities are the proc-macro graph, not a spelling.**
   Host and target members differ in features, profile, rustflags AND
   `compile_kind`; `--target` is one field of four, so no spelling merges them.
3. **The fifth and sixth were a real spelling pair that W3's cmake half had
   already killed** — the implicit member is pre-2026-08-08 residue. A fresh
   build writes 5, not 6.

The re-derived sweep sizes, for whoever does land the leaf half: **58** call
sites of `cargo_target_profile_dir` / `target_profile_dir` across `just`,
`scripts` and `packages` (the earlier figure was 45), and **104** hits for a
hardcoded `target/<profile>/` segment across `just`, `scripts`, `packages`,
`cmake`, `zephyr` and `examples` (the earlier figure was 115). Both are one
grep; both moved because the greps differ, not because the tree did.

**What the leaf half is still worth, and how to measure it before spending the
sweep.** The remaining payoff is not identity COUNT — it is sccache. `RUSTC_WRAPPER`
is wired globally in the justfile and sccache is installed here, and W3's
measurement 2 showed the two spellings share zero cache entries (0 hits / 62
misses on a cold private cache). So the leaf half's real claim is "leaf builds
start hitting corrosion's cache entries". That claim is testable in one leaf
build against a primed cache, and it should be tested BEFORE the 104-literal
move — because a leaf's units must also match on features, profile, rustflags,
`config` and cargo's `path` field to hit, and this phase has now refuted six
premises that were reasoned rather than measured. Two of those fields can be
read today, from the same fingerprints, and they point opposite ways: `path`
ALREADY agrees (the glue trees and the `nros_ws_runtime` root both hash
`2387…3694`, because a package outside the workspace root is spelled
absolutely — so every out-of-root leaf in a checkout agrees with them), while
`config` ALREADY disagrees between the two cmake-side roots in one tree
(`9396…2401` vs `1190…4724`), since each dir resolves its own
`nros sync`-managed `.cargo/config.toml`. A leaf brings its own config too.
`--target` is one of five fields, and it is not the one currently failing. Note also that adding `--target` to a
leaf whose graph contains `nros-macros` SPLITS one compilation into two (host
proc-macro graph + target lib), so the leaf-side identity count goes UP; the
win, if any, is cross-tree cache reuse, not fewer compilations.

#### Reading the axis, from now on

`check-artifact-identity-budget` now reports the R3 split it could previously
only contribute to, since cargo encodes it in the path. Measured on the mixed
tree BEFORE the cmake change is rebuilt into it:

```
nros_core 4/8 identities; worst crate 5/9; worst identity 5/5 copies
R3 axis (host vs explicit --target): identities 137/46, copies 188/54
```

The host column has exactly two populations there: the two corrosion roots'
host halves (`cargo/<root>/nros-relwithdebinfo/deps` — build scripts and proc
macros, unavoidable) and the five `nano_ros_cpp_ffi_*/target/nros-minsizerel/`
trees, which are what this work item moved. It is a REPORT and not a budget on
purpose: the host column has a floor nobody controls.

**Direction, given W2's reframing.** Unifying means picking ONE spelling for
every host build, and the cheap-looking direction is the wrong one. Dropping
corrosion's `--target` is not a local edit — corrosion derives it from
`Rust_CARGO_TARGET` and the cmake integration is built around it. Adding
`--target` to the native leaves is mechanical but WIDE: with an explicit target
triple cargo moves artifacts from `target/<profile>/` to
`target/<triple>/<profile>/`, and every fixture resolver, staleness probe and
staging path that spells the former would have to move as one change. That is
the issue-0196 class — a path convention with many spellings — so it is a
class-wide sweep or nothing, never a per-site fix.

Note this split costs CPU as well as disk, and sccache does NOT paper over it:
a different `-C metadata` is a different cache key, so the cmake-driven and
cargo-driven builds of an identical crate miss each other's entries. Unlike R1,
this duplication is real compilation even with a warm cache — which makes W3 the
better CPU target and W2 the better disk target.

#### Measured 2026-08-06 — the split is total, and it is the FLAG not the driver

Same crate, same features (`std,rmw-zenoh`), same triple, one factor varied:

| build | `nros_core` `-C metadata` |
| --- | --- |
| implicit host (what a cargo leaf does) | `d43b191ebc13de43`, `ef26eca89af52b10` |
| `--target x86_64-unknown-linux-gnu` (what corrosion does) | `2b8d1fd2adda0290` |

Intersecting the identity SETS actually on disk:

```
corrosion trees  ∩  native cargo leaves      = {}          <- zero overlap
corrosion trees  ∩  explicit --target build  = {2b8d1fd2adda0290, 482da2a2e947742f}
corrosion trees  ∩  implicit host build      = {2b8d1fd2adda0290}
```

Three things follow.

1. **Feature-set equality is NOT sufficient for sharing.** The cmake-driven and
   cargo-driven halves of this repo share *nothing* — the intersection is empty,
   not small. Every crate in the shared stack is built at least twice.

2. **The cause is the `--target` flag, not cmake or corrosion.** A plain cargo
   build that passes the same explicit `--target` immediately shares two of
   corrosion's four identities. So this is normalisable by changing one flag's
   spelling, which is what W3 is.

3. **Part of the tree already normalised, by accident.** `2b8d1fd2adda0290`
   appears even in the IMPLICIT build — via the nested `nros-sizes-build` probe,
   which passes its own explicit `--target` regardless of the outer mode. The
   probe's sub-builds therefore already sit on corrosion's side of the split.
   That is a working demonstration that the fix produces sharing rather than a
   theory that it would.

**Acceptance:** either the two paths share an identity, or the reason they
cannot is written down where the next reader will find it. **Met for the cmake
lane** (they share; the split was inside nano-ros' own generators and is gone,
gated). **Met by the second arm for the cargo-leaf lane** — the reason is
recorded above and at the resolver, and it is R2, not `--target`.

### W4 — gate the property

#### The user-facing reproducer, measured 2026-08-06

The question a user actually asks: *my project needs a Rust leaf R and a C++
leaf C, both depending on a Rust dependency D, and I choose the feature set — is
D built once?*

`examples/workspaces/mixed` IS that project: Rust and C++ node packages in one
workspace over a shared `nros-core`. Counting `nros-core` in its build tree:

```
08130df18f473a4b  nano-ros_0b88c/x86_64-unknown-linux-gnu/nros-relwithdebinfo
2b8d1fd2adda0290  nano-ros_0b88c/x86_64-unknown-linux-gnu/nros-relwithdebinfo
482da2a2e947742f  nano-ros_0b88c/nros-relwithdebinfo
b2744896132993cc  nano-ros_0b88c/nros-relwithdebinfo
4a86c738c0e9ce80  nros_ws_runtime_14eac/x86_64-unknown-linux-gnu/nros-relwithdebinfo
623c410f90cfce6a  nros_ws_runtime_14eac/x86_64-unknown-linux-gnu/nros-relwithdebinfo
a46af0c36bd41dd5  nros_ws_runtime_14eac/nros-relwithdebinfo
cf7d853a4c3ed530  nros_ws_runtime_14eac/nros-relwithdebinfo
```

**D is built EIGHT times, with eight distinct identities — zero sharing**, in a
single workspace, at one user-chosen feature set. The split is three-way and
each axis is a different cause:

| axis | copies | cause |
| --- | --- | --- |
| two corrosion roots — `nano-ros_0b88c` (the C++ side) vs `nros_ws_runtime_14eac` (the Rust umbrella) | ×2 | **R2** — separate cargo invocations resolving through DIFFERENT workspace manifests. Corrosion keys its dir on `sha1(workspace_manifest_path)`, so they cannot even land in the same tree. **The identity split is upstream of the dir split**: root A builds the shared crates as workspace MEMBERS, root B as out-of-workspace PATH DEPS, and cargo spells (and hashes) the source differently for each — see "R2 re-measured — `path` is a RELATION, not a spelling". |
| host vs `x86_64-unknown-linux-gnu` | ×2 | **R3** — the explicit `--target` split, here appearing WITHIN one invocation (build scripts/proc macros vs the library). |
| two identities per root+arch cell | ×2 | **unattributed.** Both fingerprints report identical `features = ["alloc","std"]`, so it is not feature-driven; `-C metadata` folds in dependency metadata, so the likeliest cause is the two staticlibs (`nros-c`, `nros-cpp`) resolving an intermediate crate differently. Not yet proven — do not quote a cause. |

> **Cross-reference — issue 0493.** See "Relationship to issue 0493" at the end
> of this document: the same multi-identity class surfaces there as a hard LINK
> failure, and it carries negative evidence on this table's "unattributed ×2".

Note what this rules out: the R/C split here is **not** the corrosion-vs-cargo
flag difference measured in W3, because BOTH sides go through corrosion with an
explicit `--target`. Same driver, same flag, same features — and still no
sharing, because they are different workspaces. **Fixing W3 alone would not make
D build once for this user.**

#### Work item — LANDED 2026-08-07

- [x] A check that fails when the same `-C metadata` identity is built into more
      than N target dirs in one lane, so this cannot silently regrow.
- [x] Use the mixed workspace as the gate's fixture: assert `nros-core` is built
      at most K times for one workspace at one feature set. It is the smallest
      honest reproducer of the whole phase, it already exists, and today it
      answers 8.

`scripts/check-artifact-identity-budget.sh`, wired into `check-fast` as
`check-artifact-identity-budget`. It reads `lib<crate>-<hash>.rlib` filenames
under `examples/workspaces/mixed/build-workspace-fixtures` — cargo's own
`-C metadata` judgement, so "same compilation?" is answered by construction —
and asserts three numbers, all measured 2026-08-07 on a full native-lane tree:

| budget | value | what it pins |
| --- | --- | --- |
| `nros_core` identities | 8 | the headline number this phase measured |
| any crate's identities | ≤9 | `nros_serdes` is the max: the 8 plus one more from the nested per-msg-package `nros-minsizerel` trees |
| copies of ONE identity | ≤5 | the five `src/*/nano_ros_cpp_ffi_*/target/` trees each write their own copy of the SAME hash — R1, verbatim |

The named budget pins the crate the phase measured; the two ceilings are the
class-wide arm, so regrowth in a crate nobody named still fails. **When W2/W3/W5
land, lower the numbers in the script** — a budget left above the truth is a
gate that has stopped gating.

Tier, and its cost: the gate is buildless (filenames only, no cargo/rustc/
resolution) but not source-free, since it needs a tree someone already built. On
the pristine per-push CI checkout it therefore SKIPS — loudly, naming the build
command. It is deliberately NOT in `build-test-fixtures`: a long-lived
incremental tree accumulates rlibs from earlier builds, so an over-count from
history alone is possible, and a gate that can red a BUILD on stale history gets
switched off. Failing a static check whose remedy is "wipe the tree and rebuild"
is survivable.

**Acceptance — met.** Tripwired on a filename-only replica of the real tree
(`find … -name 'lib*-*.rlib'` mirrored as empty files, which reproduces the real
verdict exactly): a 9th `nros_core` identity fails the named budget; a 10th
`nros_serdes` identity fails the ceiling; a 6th copy of one identity fails the
copies arm; and a tree holding rlibs but none for `nros_core` fails rather than
passing on a tree it did not understand. All four restored to green afterwards.

**What it cannot catch.**

- It counts **rlibs**. Proc-macro dylibs, build-script executables, staticlibs
  and binaries are invisible to it, so duplication that lands only in those
  forms is not counted.
- It reports a **number, not a cause**. It cannot say which of the axes grew,
  and a swap — one identity disappearing while a different one appears — keeps
  the count at 8 and passes.
- Only the **native mixed tree**. `build-workspace-fixtures-freertos`,
  `-threadx`, and the other three workspaces are unread; a regression confined
  to them is silent unless someone points `NROS_IDENTITY_BUDGET_TREE` at them.
- **Nothing at all** on a checkout that has not built the mixed workspace, which
  includes the per-push CI lane.
- It **over-counts on a long-lived incremental tree**: cargo never collects old
  rlibs, so identities from earlier builds accumulate on disk. That is the
  deliberate direction of the error — it can cry wolf, it cannot under-count —
  and the failure text says so, naming "wipe the tree and rebuild" before
  believing the number.

### W5 — the duplicate INSIDE one invocation: the build-dependency graph

Everything above is about duplication ACROSS invocations. Measured 2026-08-06,
the largest single source is inside ONE. `cargo --unit-graph` for `nros-c`
(`std,rmw-zenoh`, `nros-relwithdebinfo`):

```
181 units, 4 distinct profiles
  123 units  opt=0 panic=unwind debuginfo=0   91 lib + 27 custom-build + 5 proc-macro
   26 units  opt=2 panic=abort  debuginfo=1   25 lib + 1 staticlib   <- the product
   21 units  opt=0 panic=unwind lto=false     custom-build
   11 units  opt=2 panic=unwind lto=false     custom-build
```

**Only 26 of 181 units are the product.** Of the 14 product libraries, **12 are
ALSO built in the build-script graph** — `heapless`, `log`, `bitflags`,
`byteorder`, `cfg-if`, `hash32`, `atomic-waker`, `portable-atomic`,
`portable-atomic-util`, `stable_deref_trait`, `nros-rmw-cffi`, and `nros` itself.

**Features are NOT the cause here.** `nros-core` appears twice with an identical
feature set:

```
nros-core feats=['alloc','std']  panic=abort   opt=2     <- product
nros-core feats=['alloc','std']  panic=unwind  opt=0     <- build-dep graph
```

Same crate, same features, one invocation, two compilations. And sccache cannot
absorb it: different profile means different rustc arguments, so a different
cache key.

**It cannot be fixed by making the profile consistent.** `build-override` aligns
`opt-level`, `debuginfo`, `lto` and `codegen-units`, but cargo rejects the one
that matters:

```console
$ CARGO_PROFILE_NROS_RELWITHDEBINFO_BUILD_OVERRIDE_PANIC=abort cargo build …
error: `panic` may not be specified in a `build-override` profile
```

Build scripts and proc macros are always unwinding, by construction. **So while
the product profile carries `panic = "abort"`, every crate in both graphs is
compiled twice and no configuration prevents it.** Aligning the other four
settings buys nothing on its own, because `panic` alone still splits identity.

**The lever is the GRAPH, not the profile.** The whole runtime stack is in the
build-dependency graph because of one edge in `packages/api/nros-c/Cargo.toml`:

```toml
[build-dependencies]
# Phase 77.25: … force nros to compile before
# this build.rs runs so the size probe has a rlib to read.
nros = { version = "0.5.0", path = "../nros", default-features = false }
```

`nros` is a build-dependency **purely to force build ordering** — the size probe
locates its rlib with `find_dep_rlib`. That one ordering trick pulls
`nros → nros-node → nros-core → heapless, portable-atomic, log, …` into a graph
that then compiles all of it a second time at `opt=0 panic=unwind`.

- [x] **W5.a** ANSWERED 2026-08-06 — **the edge is dead weight for the DEFAULT
      probe path and load-bearing for the FALLBACK one.** It is a trade, not a
      free deletion. Detail below.

#### W5.a result

`find_dep_rlib` has two implementations and tries them in order:

* **`find_dep_rlib_isolated`** (default, first) — builds the crate in its own
  nested target dir. Needs no ordering from the outer cargo, so the build-dep
  buys it nothing.
* **`find_dep_rlib_filesystem`** (fallback, and what
  `NROS_SIZES_PROBE_MODE=filesystem` forces) — polls the OUTER target dir for the
  rlib with a 60 s timeout. The build-dep is what makes that deterministic
  instead of a race.

Removing the edge from `nros-c` and rebuilding:

| | units | product libs | build-graph libs | built twice |
| --- | --- | --- | --- | --- |
| with the edge | 181 | 14 | 68 | **12** |
| without | 165 | 14 | 64 | **8** |

The build succeeds, the isolated probe does NOT fall back (no
`cargo:warning=… isolated probe failed`), and the generated sizes are
**byte-identical** (`NROS_EXECUTOR_SIZE 89392`, …) to the with-edge tree. So
correctness is preserved on the default path, and removal buys 16 units and
drops `nros`, `nros-rmw-cffi`, `atomic-waker`, `portable-atomic` and
`portable-atomic-util` out of the duplicated set.

**But do not delete it outright.** `NROS_SIZES_PROBE_MODE=filesystem` still ran
green twice without the edge — which is NOT evidence of safety. That path is a
poll with a timeout, so it usually wins; the manifest comment records it losing
historically ("the probe ran against a missing rlib on clean" builds), and
`just verify-size-probe` exercises both modes as a parity gate. Two green runs
cannot refute a race the code was written to prevent.

The remaining 8-crate overlap (`heapless`, `log`, `bitflags`, `byteorder`,
`cfg-if`, `hash32`, `stable_deref_trait`) comes from the OTHER build-deps —
`nros-build-helpers`, `nros-zpico-build`, `nros-board-common` — so this edge was
never the whole story.
**Update 2026-08-07 — issue 0464 changed W5.b's cost.** The filesystem fallback
the edge existed to serve is GONE, and NuttX was verified building through the
isolated probe alone. So the edge now serves nothing on any exercised path: W5.b
is closer to a deletion than to the feature-gate below, and the feature-gate is
only needed if a future target reintroduces a fallback requirement. Re-measure
before choosing.

- [x] **W5.b/W5.c — LANDED 2026-08-07, and as a DELETION, not the feature-gate
      this item specified.** The gate was scoped when the filesystem fallback
      still existed; issue 0464 removed it, so `find_dep_rlib` is the isolated
      nested build alone and never reads the outer graph. The edge therefore
      orders something nobody waits for — there is no configuration in which it
      is needed, so there is nothing to gate. Removed from BOTH `nros-c` and
      `nros-cpp`; `nros` stays a REGULAR dependency, which is what the nested
      probe's `cargo build -p nros` resolves through.

      Measured on `nros-c` (`std,rmw-zenoh`): **181 → 165 units**, product/
      build-graph overlap **12 → 8** — exactly W5.a's prediction. Sizes
      unchanged at `NROS_EXECUTOR_SIZE 89392`, and `just verify-size-probe`
      passes both `cargo clean` soak rounds.

      **The clean-build scenario 77.25 cited was tested directly**, because that
      is the case the edge was added for: a fresh target dir emits correct sizes
      without it. A `cargo build` of `nros-cpp` at DEFAULT features does still
      warn `EXECUTOR_SIZE probe returned 0`, but that is not this change —
      restoring the edge reproduces it, and with the edge BOTH crates warn where
      only one does without. It is the unenforced zero-probe path already filed
      as issue 0472.
- [x] **W5.d** **ANSWERED 2026-08-11 via the acceptance's second clause** — the
      reason is recorded rather than the overlap removed, because the edge is
      not a stray dependency.

      None of the three names a runtime crate. Measured from their manifests:

      ```
      nros-build-helpers   cbindgen cc nros-cc-flags nros-sizes-build
      nros-zpico-build     cbindgen cc nros-cc-flags nros-board-common
                           nros-build-paths nros-zephyr-build pkg-config
      nros-board-common    serde toml cc nros-build-paths
      ```

      All build tooling. `heapless`, `log` and the rest enter through
      **`nros-sizes-build`, which spawns a nested `cargo build -p <crate>`** to
      read `__nros_size_*` symbols — the probe must COMPILE the product crate to
      measure it, so the product graph is inside the build graph by
      construction, not by a manifest mistake.

      **Removing the overlap would mean removing the probe**, and the probe is
      what keeps the C `_opaque` buffers correctly sized (the silent 0-byte
      `NROS_*_SIZE` class, phase 77.23/77.24). The overlap is the price of
      measuring sizes at build time, and the manifest edge that causes it is
      `nros-build-helpers -> nros-sizes-build`.

      Two things this phase changed already reduce its COST without removing it:
      issue 0490 (the probe's chain no longer rebuilds permanently) and the
      shared probe dir becoming the default (phase-343 I1, ~76.8 GiB).

      Left open would be misleading: the number cannot drop while the mechanism
      is wanted.

**Acceptance:** the product/build-graph overlap drops from 12 of 14 with
`just verify-size-probe` still green in BOTH modes, or the reason it cannot is
recorded at the manifest edge that causes it.

### W4 follow-up — the budget counted W5's probe dirs, so it drifted with build history — FIXED

`check-artifact-identity-budget` went RED on my tree today with
`nros_core has 9 distinct -C metadata identities … (budget 8, recorded
2026-08-07 by phase-340 W4)`, on a working tree whose only source change was
documentation. Counted:

```
52  libnros_core-*.rlib under examples/workspaces/mixed/build-workspace-fixtures
40  of them (76 %) inside .../build/nros-c-<hash>/out/sizes-probe-target-.../
 8  distinct identities once the sizes-probe dirs are excluded  ← the budget
```

The 9th identity comes entirely from the size probe's NESTED target dirs — one
per `nros-c` build-script instance, each rebuilding `nros_core` inside its own
`sizes-probe-target-rustc-…` tree. That is W5's duplicate-inside-one-invocation,
observed from the gate's side, and it is also what issue 0464 is about.

The consequence for the gate: **its count grows with how many times the tree has
been built, not with what the source says.** A budget recorded on one tree
red-lights another that merely built more, and `just check fast` then fails for
a reason no diff explains — which is expensive precisely because the gate is in
the fast tier that every task runs first.

**RESOLVED 2026-08-07 (`ee016145a`)** — the first option was taken: the find
prunes `*/out/sizes-probe-target-*` before counting.

```sh
rlibs="$(find "$TREE" \
    -type d -path '*/out/sizes-probe-target-*' -prune -o \
    -path '*/deps/*' -name 'lib*-*.rlib' -print 2>/dev/null | sort)"
```

Verified on a replica carrying 8 ordinary identities plus a 9th existing ONLY
inside a probe dir: the current script reports `nros_core 8/8` and passes, while
the same tree run through the pre-fix script (the prune removed) exits 1. So the
prune is doing the work, not merely present.

**What excluding costs, stated so it is not silently lost.** Those probe
compilations are real duplicates — they are exactly W5's
duplicate-inside-one-invocation — and this gate no longer counts them. That is
deliberate: their number tracks how many times the tree was built, so as a
BUDGET they are unusable, and W5 owns the population with a measurement that
does not depend on build history. Work item 2 of the work order (`W5.b/c`)
removes most of them at the source; when it lands, this exclusion should be
re-examined rather than assumed permanent.

**Second-order note.** With the probe dirs pruned, the `worst identity ≤ 5
copies` ceiling now measures a smaller population than when it was recorded, so
it is looser than intended rather than tighter. W7's re-measure must reset all
three budgets together.

### W6 — the ZEPHYR fixture lane, which no measurement above covers

Everything measured so far is the `linux` fixture rows (117 rows, 60 signatures)
plus the build-dep graph. The **Zephyr west lane is a separate population and a
bigger one**, and it is opted out of every mechanism this phase has considered.
Measured 2026-08-07 from the driver's own per-fixture records
(`build/zephyr-fixture-make-driver/status/<run>/*.status`, which carry
`start_epoch` / `end_epoch` / `duration_s`).

#### The lane is CPU-bound on duplicated Rust compiles, not on overhead

Two consecutive `just zephyr build-fixtures` runs:

| run | fixtures | wall | CPU-sum | median/fixture |
| --- | --- | --- | --- | --- |
| colder | 68 | 33.2 min | 871 min | 776 s |
| warmer | 68 | 19.4 min | 509 min | 441 s |

Recipe start → driver start is **~40 s**, so the fan-out is essentially the
whole wall time; there is no setup tax worth chasing. Concurrency is already
32-way for 58 % of the wall (a 26× compression of 509 CPU-minutes). The lane is
not under-parallelised — it is doing too much work.

A single fixture's ninja graph is 42 steps, and three of them are the cost:

```
[1/44] Building nros-c via Cargo
[2/44] Building nros-cpp via Cargo
[4/42] Building nros-rmw-zenoh-staticlib via Cargo
```

`nros_cargo_build()` sets `CARGO_TARGET_DIR` to
`${CMAKE_BINARY_DIR}/nros-rust` — **per Zephyr build dir**. So all 68 fixtures
compile those crates into 68 private target dirs. On disk:

```
415 G   zephyr-workspace/            (85 build dirs)
286 G     of which per-fixture cargo target dirs (45 `nros-rust` dirs)
1069    libnros_core-*.rlib copies
  36    libnros_c.a  /  36  libnros_cpp.a
```

For scale, the tree-wide count this phase opened with was 327
`libnros_core-*.rlib`; the Zephyr lane alone holds **1069**.

#### The duplicates are the same compilation, differing only by an embedded path

This is the part that decides the mechanism. Two same-RMW C fixtures
(`build-c-talker-zenoh`, `build-c-listener-zenoh`) have **identical Kconfig** —
`diff` over their 29 `CONFIG_NROS_*` knobs is empty — so their `libnros_c.a`
should be one compilation. The artifacts differ by **8 bytes**:

```
34796696  build-c-talker-zenoh/.../libnros_c.a
34796704  build-c-listener-zenoh/.../libnros_c.a
```

and each embeds its own build-dir path twice (`strings | grep -c` gives 2/0 and
0/2). They are byte-different only because the target dir lives inside the
Zephyr build dir and rustc bakes that absolute path in.

That matters beyond disk. sccache's Rust hashing includes the CONTENT of the
`--extern` rlibs a crate is compiled against. Once the bottom of the graph
differs by a path string, **every crate above it misses across fixtures** — the
cache can only hit for leaf crates with no externs. So the lane pays real
compiles 68 times for work that is identical modulo a string.

#### sccache is wired and sized — but the size is only honoured if `just` starts the server

`RUSTC_WRAPPER` is exported globally (`justfile:13`) and
`SCCACHE_CACHE_SIZE := "30G"` right below it, with phase-165.perf's reasoning
attached: the 10 GiB default "evicts mid-sweep once the ~150 standalone
example/fixture crates plus the Zephyr C objects land in the cache". So the lane
is neither missing the wrapper nor under-sized, and "raise the cache" is NOT an
available win. I first wrote that it was, having read `Max cache size 10 GiB`
from a server I had started myself outside `just`.

That mistake is the finding. As the justfile comment says, the variable is
"only read at sccache server start" — so whichever process starts the daemon
fixes the size for every later user. A server started by anything outside `just`
(a bare `sccache --show-stats`, an editor, rust-analyzer) silently gives the
whole sweep a 10 GiB cache, and nothing reports it. The sweep gets slower with
no signal, which is the same silent-degradation shape this repository keeps
finding elsewhere.

Cheap guard: have the Zephyr lane compare the running server's `Max cache size`
against `SCCACHE_CACHE_SIZE` and say so when they disagree (restarting the
daemon is a one-liner, but even just printing it converts an invisible 3×
capacity loss into a line of output).

#### The remap is PROVEN, not proposed — measured 2026-08-07

Built `nros-c` twice into two different target dirs and compared:

| build | result |
| --- | --- |
| plain, two target dirs | differ |
| `--remap-path-prefix=<dir>=/nros-target`, dev profile | differ — **379 bytes** of 24.5 MB |
| same remap, non-incremental profile | **IDENTICAL — 0 differing bytes** |

Two things fall out of the middle row. The residual 379 bytes were entirely
codegen-unit name suffixes (`nros_c.<hash>.1w7uncg.rcgu.o` vs `…10ahzya…`) —
independent confirmation of this phase's R4, that `incremental` destroys
byte-reproducibility, arrived at from the opposite direction. And the symbol
hashes were IDENTICAL across the two builds (`17h41f705d7eeb2beb8E` in both),
which answers a question the remap raises: each fixture must pass a DIFFERENT
`--remap-path-prefix` flag (its own dir on the left-hand side), and that flag
difference does **not** perturb `-C metadata`. So per-fixture remapping still
yields one shared identity.

W1 having already dropped `incremental = true` from the shared profiles
(2026-08-06 — it survives only in the interactive `nros-iterate`), the
non-incremental row is the configuration the fixture lanes are in TODAY.
**The embedded target-dir path is the only thing left between the Zephyr lane
and bit-identical artifacts.**

Repro:

```sh
RUSTFLAGS="--remap-path-prefix=$D/pA=/nros-target" cargo build -p nros-c --release --target-dir $D/pA
RUSTFLAGS="--remap-path-prefix=$D/pB=/nros-target" cargo build -p nros-c --release --target-dir $D/pB
cmp $D/pA/release/libnros_c.a $D/pB/release/libnros_c.a   # → equal
```

#### Tried on the lane: remap is NECESSARY but NOT SUFFICIENT

I implemented the remap in `nros_cargo_build.cmake` and rebuilt the two C
fixtures. The flag reaches the command (3 occurrences in `build.ninja`) and the
artifacts remain different — same size, **1539 differing bytes**, and the build
dir path is STILL embedded twice. Reverted rather than landed: a flag with no
measured effect is not worth a full-tree rebuild.

Two residuals explain the gap between this and the clean experiment above, and
both are worth knowing before anyone retries:

* **`env!("OUT_DIR")` is not a source path.** The surviving string is
  `…/nros-rust/…/build/nros-c-<hash>/out/…`, captured by nros-c's build script
  and baked in as a string literal. `--remap-path-prefix` rewrites paths in
  debug info and `file!()`; it does not rewrite the CONTENT of an `env!`
  literal. Any crate using the standard `include!(concat!(env!("OUT_DIR"), …))`
  pattern carries its absolute target dir into the artifact regardless.
* **`codegen-units = 16`.** The clean experiment used `--release`
  (`codegen-units = 1`) and reached 0 differing bytes; the fixture profile
  `nros-relwithdebinfo` uses 16, and the residual diff is again CGU name
  suffixes (`…0omp3nw.rcgu.o` vs `…1dda5ar.rcgu.o`). With 16 units the
  partitioning is not reproducible run to run. That is a real build-speed
  tradeoff, not an oversight — CGU=1 is slower to compile.

So cross-fixture artifact identity on this lane needs all three of: the remap,
an answer for `env!("OUT_DIR")` literals, and `codegen-units = 1` (or accepting
non-identity). Any one alone changes nothing measurable.

**Corrected 2026-08-07 by measurement — it is TWO ingredients, and the middle
one was misattributed.** The embedded path is NOT an `env!("OUT_DIR")` Rust
literal: the generated `.rs` files under `OUT_DIR` contain no absolute path at
all. It came from `nros_variant_symbol.o` — a C translation unit that
`nros-build-helpers` GENERATES into `OUT_DIR` and compiles with `cc`, which
records `__FILE__` and the debug compilation dir. `--remap-path-prefix` is a
RUSTC flag and cannot reach a C compile; `-ffile-prefix-map` is the C-side
equivalent.

Measured on `nros-c` (`std,rmw-zenoh`, `nros-relwithdebinfo`), two target dirs:

| | `libnros_c.a` | path strings |
| --- | --- | --- |
| before (W6's attempt) | differ, 1539 B | present twice |
| `-ffile-prefix-map` alone | differ, **15 B** | **0** |

And per-rlib, which is what sccache actually keys on (`--extern` CONTENT):

| crate | C fix only | C fix + rustc remap |
| --- | --- | --- |
| `nros_core`, `nros_serdes` | **identical** | identical |
| `nros_node`, `nros_rmw_zenoh` | differ (34 B / 50 B) | **identical** |

So the two flags fix DIFFERENT populations and both are needed; together every
rlib compared is byte-identical with matching `-C metadata`.

**`codegen-units = 1` is NOT needed.** W6 predicted it as the third ingredient
from the residual CGU-suffixed names, but with the C path fixed the codegen-unit
member names are already deterministic — `nros_c.…1d51717e27cab089-cgu.00/01/02`
matched exactly across both dirs. The remaining 15 bytes in `libnros_c.a` are
cc-rs's OBJECT FILENAME hash (`5046a7ee…-nros_variant_symbol.o` vs
`f43824e5…-`), which it derives from the source path; neither flag renames it,
and no `codegen-units` value would. That is a cc-rs naming artifact, not a
reproducibility failure — the object's CONTENT is identical.

Dropping `codegen-units = 1` from the plan removes the one ingredient that cost
build speed and needed its own A/B.

**Which matters for CPU, not just disk.** sccache hashes a crate's inputs
including the CONTENT of its `--extern` rlibs. CGU nondeterminism and OUT_DIR
literals both make the bottom of the graph differ, so misses cascade upward
whichever one is left unfixed. That is why partial application yields nothing:
the cascade only stops when the artifacts are actually identical.

#### Proposed order — cheapest and least risky first

1. **Two flags, together** (revised — see the correction above):
   `-ffile-prefix-map` on the generated C TU and `--remap-path-prefix` on the
   Rust side. `codegen-units = 1` is NOT required, so the build-speed A/B this
   step used to need is gone.

   **Half of it has LANDED (2026-08-07):** `-ffile-prefix-map` is in
   `nros-build-helpers`' `variant_symbol` compile. It removes every embedded
   path string and makes `nros_core` / `nros_serdes` rlibs byte-identical across
   target dirs on its own.

   **Remaining:** the rustc `--remap-path-prefix`, which is what `nros_node` and
   `nros_rmw_zenoh` still need. Each fixture passes its OWN dir on the
   left-hand side — measured NOT to perturb `-C metadata`, so per-fixture flags
   still yield one shared identity.

   **Attempted 2026-08-07 in `zephyr/cmake/nros_cargo_build.cmake` and REVERTED
   — that file is the wrong code path for the population W6 targets.** A Rust
   Zephyr leaf's cargo command is not emitted by nano-ros at all. Forcing a
   reconfigure of `build-rs-talker-zenoh` and reading the generated
   `build.ninja` shows:

   ```
   cargo build --no-default-features --features rmw-zenoh \
       --config patch.crates-io.zephyr.path=…
   ```

   Those `--config` entries come from **zephyr-lang-rust**, and the added
   remap does not appear — `nros_cargo_build` is the path for the C/C++ side
   (`nros_c_cargo_build`, `zephyr/CMakeLists.txt:202`), not for a Rust leaf.
   Reverted rather than left in, on this phase's own precedent: W6 reverted its
   first attempt because "a flag with no measured effect is not worth a
   full-tree rebuild", and a flag that provably never reaches the command is a
   stronger case for the same call.

   **What a retry needs**, in order:
   1. Find where zephyr-lang-rust builds its cargo argv and whether nano-ros can
      contribute a flag to it. The `--config` mechanism itself is sound and
      MEASURED: it MERGES with `target.<triple>.rustflags` rather than replacing
      them (`["-C","link-arg=-DFROM_CONFIG","-C","link-arg=-DFROM_CLI"]`),
      unlike `RUSTFLAGS`, whose env-var precedence would silently DISCARD the
      board's link args — issue 0440's failure by another route. Whatever the
      injection point, use `--config`, never `RUSTFLAGS`.
   2. Note it must go on `target.<triple>.rustflags` when a triple is set: cargo
      takes target-specific rustflags OR `build.rustflags`, never both, so a
      `build.rustflags` entry is ignored wherever a target block exists.
   3. Verify on the lane, not the host — and force it, because the fixture
      freshness check silently skips a leaf whose signature still matches
      (`zephyr-fixture-run-one.sh` exits 0 printing nothing). Delete the build
      dir to force a reconfigure.

   The C/C++ Zephyr leaves may still benefit from the same flag in
   `nros_cargo_build.cmake`; that is untested and is a separate measurement.

   **Retried 2026-08-07 at the correct injection point — and the premise is
   REFUTED for this lane.** `scripts/zephyr/cargo-features-patch.sh` is the
   sanctioned seam for the vendored module (Phase 168.1 added the
   `EXTRA_CARGO_ARGS` pass-through there). Injecting the `--config` remap the
   same way DID reach the command — 3 occurrences in `build.ninja`, the check W6
   itself used. It changed nothing:

   | comparison | result |
   | --- | --- |
   | talker vs listener | differ 609918 B — but these have DIFFERENT Kconfig, so W6's own caveat says they are genuinely different compilations. Wrong pair. |
   | **talker vs talker, two build dirs** | **differ 609919 B** — same Kconfig, path variable isolated |

   And the artifacts contain **zero** references to either the build dir or the
   remap target, so there was no embedded path for the remap to rewrite.

   **The blocker on this lane is `incremental`, which is R4, not the path.** The
   Zephyr build runs at the `dev`/`debug` profile — 185 MB of
   `rust/target/debug/incremental` per fixture — and the differing bytes are
   precisely the per-session codegen-unit tokens R4 describes:

   ```
   nros_core-…0ixf3k5w57bzqjd27sxorel8t.03jlxue.rcgu.o
   nros_core-…0lo4ubhov927acmimhig7h5ke.04jgqw0.rcgu.o
   ```

   W1 dropped `incremental` from `nros-relwithdebinfo` and `nros-minsizerel`,
   but cargo defaults it ON for `dev`, and nothing pointed this lane at a
   nano-ros profile. **So step 1 for the Zephyr lane is a one-line profile
   question, not a remap**: give the lane a non-incremental profile (or set
   `incremental = false` on `dev`) and re-run the two-dir `cmp` above before
   adding any flag.

   Reverted the patch-script change rather than leaving it: it reaches the
   command and demonstrably changes no artifact, which is the same standard W6
   applied to its own first attempt.

   *Method note for the retry:* compare the SAME fixture built into two dirs.
   Two different fixtures cannot answer this — their Kconfig differs, so they are
   allowed to differ, which is how the first comparison here misled.

   **DONE 2026-08-07 — `incremental = false` on `dev`, and it is the whole
   blocker.** Set in the root `Cargo.toml` AND in `.cargo/config.toml` (a leaf
   resolves `dev` through cargo's config walk-up, not through the root manifest,
   so the lane only sees it in the second copy). Rebuilt the same fixture into
   two dirs:

   | | before | after |
   | --- | --- | --- |
   | `incremental/` per fixture | 185 MB | **1 MB** |
   | `nros_core` rlib | differs 609919 B | **IDENTICAL** |
   | `nros_node`, `nros_serdes`, `nros_rmw_zenoh` | differ | **IDENTICAL** |
   | all shared rlibs | — | **115 of 143 byte-identical** |

   No remap, no `codegen-units` change, no `OUT_DIR` work. One profile setting
   that W1 had already applied to the two `nros-*` profiles and that nobody had
   applied to the cargo built-in this lane actually builds at.

   **Not yet identical — OPEN, deferred for later investigation.** Two residuals,
   both measured, neither blocking the sccache win above:

   * **28 of 143 rlibs still differ.** Unidentified — the 115 that match include
     the whole nros stack, so these are likely third-party crates with their own
     `OUT_DIR`-generated content or build-script-baked paths. Nobody has listed
     WHICH 28; that is the first step, and it may split into two or three causes
     rather than one.
   * **`zephyr.elf` differs by 8.6 MB and the two images are different SIZES.**
     Size divergence means this is not merely embedded strings — something is
     genuinely built differently, or linked in a different order. The C objects
     are the obvious suspect (they carried build paths until
     `-ffile-prefix-map` fixed `nros_variant_symbol.o`), but a size delta is not
     explained by a path substitution alone.

   Neither is on the critical path: sccache keys on `--extern` rlib CONTENT, and
   that is what now matches. The image being non-reproducible costs disk dedup
   and any future content-addressed cache, not compile time. **Do not treat
   "115/143" as done** — record the remaining figure whenever it moves.

   **What this costs.** `incremental` on `dev` served interactive rebuilds of the
   root workspace. Those keep it via `nros-iterate` (W1's opt-in profile); what
   loses it is `dev` itself, which the fixture lanes use and never benefit from —
   the same argument W1 made, applied to the profile W1 missed.
2. **Report an sccache size/PID mismatch** on the lane (above). Not a speed-up
   in itself; it stops a 3× capacity loss from being invisible while measuring
   step 1.
3. **Only then consider sharing target dirs**, and note W2's F3 applies with
   full force here: the fan-out is 32-way, so pointing a group at one dir
   converts 32 parallel builds into serialised ones. Step 1 may make step 3
   unnecessary, which is the preferred outcome.

Steps 1 and 2 are additive — they neither serialise anything nor change what any
fixture builds — so they can land independently of W2's umbrella-invocation
design.

#### One caveat for whoever groups by identity here

The Zephyr lane's identity tuple must include the **resolved Kconfig knobs**,
not just profile/features/triple: `nros-node`'s `build.rs` bakes
`NROS_EXECUTOR_MAX_CBS` and friends into constants, so two fixtures with the same
cargo invocation and different `.config` are genuinely different compilations.
That divergence is newly REAL as of issue 0460's fix — before it, the Rust lane
silently compiled crate defaults regardless of Kconfig, so every image happened
to be identical for the wrong reason.


## Risks

**Shared target dirs serialise.** Cargo takes an exclusive lock per target dir,
so grouping leaves trades parallel-but-redundant work for serial-but-unique
work. That is a win only if the redundancy exceeds the serialisation; W2 must
measure rather than assume. The per-RMW isolation exists for a real reason
(issue 0400's host/box split is the same class) and must survive.

W1 sharpened this from a risk into a **design constraint**: with a warm sccache
the redundancy is already cheap, so the trade is a loss unless the sharing comes
from ONE cargo invocation rather than N invocations over one dir. See W2 F3.

**sccache's role — resolved for the `incremental` half, still open for the
rest.** The hypothesis recorded here was that `incremental` makes Rust
compilations non-cacheable, which would have made W1 a cache fix too. **It is
false**: `non_cacheable_compilations` was 0 in all four lane arms and in every
single-leaf arm, across ~30 000 compilations. What `incremental` actually does is
keep units away from the cache — 11514 sccache requests with it on versus 18780
with it off — so the cache serves less, without anything becoming uncacheable.

The 5310 non-cacheable calls in the original observation therefore have some
other cause, still unidentified, and the 68 % Rust hit rate is not explained by
this phase. **W2 must not be scoped as if fixing it were a known outcome.**
Finding what those calls are is worth its own investigation; the likely
candidates are the C/C++ compilations in the cmake fixtures rather than rustc at
all, since the overall rate (97 %) and the Rust rate (68 %) differ so widely.

## Relationship to issue 0493 (2026-08-10)

**Handed over 2026-08-10.** The 0493 investigation is not being continued by the
session that opened it; its issue carries a HANDOFF section with what is settled,
the three open questions in cost order, the proposed provider design, and the
environment fixes needed to reproduce `lane=native` on a non-Ubuntu host. The
highest-value one for this phase is question 2 — **which corrosion topology is
current** — because it decides whether this class is wasted disk or a build that
cannot link.

Two sessions reached one class from opposite sides. This phase measures it as
**disk** — "D is built EIGHT times, with eight distinct identities". Issue 0493
measures it as a **hard link failure**: `libnros_ws_runtime.a` bundling two
`-C metadata` identities of ten crates, so every `#[no_mangle]` export collides.
Map:

| this phase's axis | 0493 |
| --- | --- |
| two corrosion roots (R2) | the two identities that collide |
| host vs explicit `--target` (R3) | the ×2 within each root |
| "two identities per root+arch cell — unattributed" | 0493's open question |

### The one fact the two accounts disagree on

This document says corrosion keys its dir on `sha1(workspace_manifest_path)`,
"so they cannot even land in the same tree", and the W3 measurement re-states it
(`cargo/<root>_<h>`). Issue 0493 measured the opposite on
`examples/workspaces/mixed/build-workspace-fixtures`: **one** `cargo/build`,
holding **both** `libnros_rmw_cffi-*.rlib`.

Both are honest readings of different trees. The 0493 reading was re-taken on a
pristine checkout with its attempted fix reverted, and it reproduces: one dir,
two identities, 7 duplicate-symbol errors. So the two worktrees genuinely differ
in corrosion topology.

**That divergence is the finding, and it is unexplained.** Per-workspace keying
is what keeps the two roots apart; where it is absent they share one `deps/`, the
provider bundles both, and a disk-waste number becomes a build that cannot link.
Anyone continuing the identity items should establish WHICH tree state is
current before trusting either number — a bisect over the same-day changes
(B3 + wave 2 shared target dirs, phase-344 W2's builder-keyed driver) is the
cheapest way, and it has not been done. Do not quote either topology as settled.

### Vocabulary: "umbrella" means two different things

* **W2.b's umbrella** — one cargo WORKSPACE spanning the example leaves (a
  generated symlink farm). **CLOSED as impossible**: 22/22 leaves carry
  `[workspace]`, which IS the copy-out promise, and cargo hard-errors on nested
  roots.
* **`nros_synth_runtime_umbrella`** — a synthesised staticlib PROVIDER crate
  (phase-241 W11 Option D).

0493's proposed design makes the SECOND the single implementation provider per
configure. It does not re-propose the first. A reader who bounces that design off
"the umbrella is impossible" has conflated the two.

### Negative evidence for the "unattributed ×2"

This phase names the likeliest cause as "the two staticlibs (`nros-c`,
`nros-cpp`) resolving an intermediate crate differently", correctly adding "not
yet proven — do not quote a cause". 0493 tried retiring `nros-cpp` outright
(mirror rebound to the active provider, `EXCLUDE_FROM_ALL` on the displaced
build) and **the duplication was unchanged**. So `nros-cpp` alone is not the
second producer. The candidate narrows; it is not settled.

### A log-reading pitfall that cost 0493 three wrong attributions

`workspace-fixtures-build.sh` runs groups under `make -j`, so group banners and
compiler output INTERLEAVE. "The last `== workspace group: … ==` line before the
error" is therefore not the failing group — 0493 mis-attributed the same failure
to `mixed`, then `safety`, then `c` before noticing. Attribute by the failing
TARGET PATH (`src/<entry>/…`) and resolve that back to a workspace, or re-run the
one group serially.
