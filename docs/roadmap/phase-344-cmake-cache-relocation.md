# Phase 344 — Relocating the cmake caches: what it costs, what it buys, and what it does not

**Status (2026-08-10). MEASUREMENT COMPLETE; DECISION MADE; NO PATH HAS MOVED,
DELIBERATELY.** This phase is [phase-340](phase-340-build-artifact-reuse.md)'s
**P3**, split out as that item asked. The census was re-derived rather than
inherited, and it does not reproduce: the briefed "~240 cmake-style dirs" is
**151**, and **83.2 % of their bytes are a cargo target dir**, not cmake output.
That reframes the item — relocation is worth doing on RFC-0070's own terms
(one root, one vocabulary, one derivation, and it unblocks P4), but it is
**disk-neutral by construction**, and the ~150 GiB inside those trees is
**not reachable by moving them**. Section "The mechanism" carries the proof.
No code changed here because the acceptance rule is a rebuild and this worktree
cannot perform one (see "What could not be verified").

**Implements:** [RFC-0070](../design/0070-build-cache-layout.md) R1/R2/R3 for the
`cmake` kind. **Settles** the RFC's own step-2 precondition — "the in-source
suffix zoo … either needs a source-relative sibling derivation or belongs to
step 3. Deciding which is a precondition for touching it."
**Parent:** phase-340 P3. **Touches:** [phase-334](phase-334-build-cache-layout.md),
[phase-343](phase-343-host-build-graph-duplication.md) (the same corrosion mass,
approached from the cargo side).

---

## RE-SCOPED 2026-08-10 — W3..W6 are withdrawn

RFC-0070 R1 was amended to apply to the **nano-ros workspace only**, not to the
copy-out examples (see the RFC's R1 table). That removes this phase's premise:
W3..W6 relocate `build-<rmw>` dirs that live in `examples/**` copy-out leaves,
where `build/` beside the source IS the convention a copied-out project should
follow.

**They were also never a disk win.** This phase's own §1.5 measured it: 83.2 % of
those bytes is corrosion's cargo tree, and relocation frees ZERO. Moving 182 GiB
to gain nothing, against the convention, is not worth doing.

**Withdrawn:** W3 (`build-xrce`), W4, W5, W6 — the per-family moves.
**Kept:**
* **W2** — LANDED. Builder-keyed attribution was worth having on its own: it
  found six rust-through-cmake rows whose cyclonedds build had no manifest row.
* **W7** — RE-SCOPED to the workspace only. The gate must fail on build output
  inside a *workspace* source dir, and must NOT flag `examples/**`.

**What replaces them.** The real remaining prize is the one §1.5 measured and
this phase explicitly deferred: **50.1:1 identity duplication inside the
corrosion trees** (96,663 rlib/rmeta over 1,931 distinct names). That is a
SHARING problem, not a placement problem — R4's "one invocation over many
packages", applied to cmake via a single workspace configure. It wants its own
phase, and it carries W1's hazard: 132 `libnros_c.a` in 15 distinct sizes, which
corrosion copies OUT into the consuming tree.


## 1. The census, re-derived

phase-340 P3 says its own figure "is itself one `find`; re-derive it before
designing against it." Doing so changes the number four times over.

### 1.1 What a naive count says, and why it is wrong

```console
$ find . -type d -name 'build' -o -name 'build-*'      # no pruning
1620  build      76  build-zenoh      36  build-cyclonedds      12  build-xrce
```

`build` at 1620 is nesting: a cmake tree contains `_deps/*/build`, and a cargo
tree contains `target/<profile>/build/` (cargo's build-script output directory,
which the name glob cannot distinguish from a cmake binary dir). Pruning at the
first match and excluding `third-party/`, `.git`, `.claude/`, the repo-root
`build/`, the gitignored `tmp/` scratch and the **untracked** `zephyr-workspace/`
gives **351**. Removing paths that pass through a cargo `target*/` component, and
`esp-idf-workspace/` (also untracked), gives **257 candidates**.

### 1.2 The decisive test

A cmake binary directory contains `CMakeCache.txt`. Applying that to all 257:

| | dirs |
| --- | ---: |
| **have `CMakeCache.txt` — actual cmake binary dirs** | **151** |
| do not | 106 |

The 106 decompose, and none of them is cmake:

| what it is | dirs |
| --- | ---: |
| `<leaf>/build/{nros,nros-metadata}/` — `nros sync` output (phase-330 SystemModel artifacts + codegen metadata) | 93 |
| `build-workspace-codegen/demo_bringup` — codegen output | 10 |
| `build-fixtures/demo_bringup` — codegen output | 2 |
| **`scripts/build` — a SOURCE directory the glob matched by name** | 1 |

That last row is the reason to state the method: an unpruned `-name build`
census counts the repository's own build scripts as build output. It is the same
class of error as the "glob that scanned vendored `_deps/` build dirs instead of
sources" that phase-340 records paying for.

### 1.3 The correction, stated plainly

phase-340 P3's census reads `build` 107, `build-zenoh` 76, `build-cyclonedds` 36,
`build-xrce` 12, `build-workspace-*` ~34, and concludes "roughly 240 are
CMAKE-style". The 76 / 36 / 12 reproduce exactly. **The `build` column does not
belong to this phase at all** — it is `nros sync`'s model and metadata output,
whose location is phase-330's subject and whose locator is
`nros_orchestration_ir::model_location`. Moving it is a different change with a
different owner.

So the population is:

| family | dirs | size |
| --- | ---: | ---: |
| `build-zenoh` | 76 | 88.1 GiB |
| `build-cyclonedds` | 36 | 38.2 GiB |
| `build-workspace-fixtures[-<suffix>]` | 26 | 37.1 GiB |
| `build-xrce` | 12 | 18.9 GiB |
| stray (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/build`) | 1 | — |
| **total** | **151** | **182.3 GiB** |

### 1.4 All of it is live

150 of 151 have an mtime of today; one of yesterday. There is **no museum
residue here** — which is the opposite of the cargo half, where the parallel
sweep found 81 of 84 per-leaf target dirs dead (84.0 GB) and one entire example
tree (`examples/stm32f4`, 1.8 GB) untracked and orphaned. Nothing in this
population can be reclaimed by deletion alone; every byte is something the
2026-08-10 `lane=all` run wrote on purpose.

### 1.5 95.4 % of it is already derivable

`fixtures-manifest.py` already has the single derivation RFC-0070 R3 asks for:
`cmake_build_subdir()`, consumed by `row_artifact_root()`, consumed in turn by
the build record, the staleness probe and the test-side lane inversion.
Set-differencing the 151 on-disk dirs against every `row_artifact_root()` the
manifest produces:

| | dirs |
| --- | ---: |
| on disk **and** a manifest row | **144** |
| on disk with **no** row | 7 |
| a row, not built in this tree (lane scope, not an error) | 7 |

The 7 orphans are one real gap and one stray:

* **six** `examples/qemu-riscv64-threadx/rust/{talker,listener,action-client,action-server,service-client,service-server}/build-cyclonedds`.
  These are `lang = "rust"` rows built **through cmake** by
  `just/threadx-riscv64.just:278` (`build_threadx_cmake_rmw … cyclonedds
  build-cyclonedds`). `is_cargo_row()` keys on `lang`, so `row_artifact_root()`
  returns `<dir>/target` for them and the cmake dir they actually write is
  invisible to the derivation. **The predicate is asking the wrong question**:
  "which language" is not "which builder".
* **one** `packages/rmw/cyclonedds/nros-rmw-cyclonedds/build`, a crate-local
  cmake tree outside `examples/` entirely.

This is the P2 shape (a second build path that does not transit the derivation),
and it is far smaller here than it was there. It must be closed **before** any
path moves, for the reason P2 recorded: a migration redirects the writer and
leaves an unattributed reader looking at a directory nothing writes any more.

---

## 2. The mechanism

### 2.1 What is actually inside a cmake build dir

This is the measurement that reframes the phase. Decomposing all 151:

| | KiB | GiB | share |
| --- | ---: | ---: | ---: |
| total | 191 175 436 | **182.3** | 100 % |
| of which `cargo/` | 159 059 912 | **151.7** | **83.2 %** |
| genuinely cmake / ninja | 32 115 524 | 30.6 | 16.8 % |

132 of the 151 contain a `cargo/` subdirectory, and it dominates every family
(a representative `build-zenoh` leaf: 1.6 G of 1.7 G; `build-cyclonedds`:
1.3 G of 1.4 G; `build-workspace-fixtures`: 1.7 G of 1.8 G).

That directory is **corrosion's cargo target dir**. Ground truth from the
generated ninja file rather than from inference:

```console
$ grep -o -- '--target-dir [^ ]*' examples/native/c/action-client/build-zenoh/build.ninja
--target-dir /…/examples/native/c/action-client/build-zenoh/cargo/nano-ros_0b88c
```

### 2.2 Corrosion derives it from `CMAKE_BINARY_DIR`, and offers no override

Corrosion v0.6.1 (`cmake/Corrosion.cmake`, the copy FetchContent actually used):

```cmake
cmake_path(GET workspace_manifest_path PARENT_PATH parent_path)
cmake_path(GET parent_path PARENT_PATH grandparent_path)
string(REPLACE "${grandparent_path}/" "" cargo_folder_name "${parent_path}")
string(SHA1 cargo_path_hash ${workspace_manifest_path})
string(SUBSTRING "${cargo_path_hash}" 0 5 cargo_path_hash)
cmake_path(APPEND CMAKE_BINARY_DIR ${build_dir} cargo "${cargo_folder_name}_${cargo_path_hash}"
           OUTPUT_VARIABLE cargo_target_dir)
```

Three facts follow, and all three are load-bearing:

1. The hash is over the **workspace manifest path**. Every nano-ros build names
   the same workspace, so it is the **constant** `nano-ros_0b88c` — exactly what
   RFC-0070's Consequences section recorded ("`nano-ros_0b88c` in all nine
   workspaces"). Corrosion's intended anti-collision does nothing here.
2. **The only variable component is `CMAKE_BINARY_DIR`.** Today's separation of
   151 cargo trees is entirely the separation of their cmake binary dirs.
3. **There is no override.** Not a cache variable, not a target property, not an
   option, not an argument to `corrosion_import_crate`. And it is passed as
   `--target-dir` **on the command line**, which beats both the `CARGO_TARGET_DIR`
   environment variable and `build.target-dir` in `.cargo/config.toml` — so
   neither of the two obvious escapes works.

### 2.2b Not every cargo invocation in the tree is corrosion's (issue 0493)

> **Handed over 2026-08-10** — issue 0493 has a HANDOFF section. For this phase
> the open item is whether the hashed per-workspace dirs and the hashless
> `cargo/build` coexist by design or one is drift; it is unbisected, and the
> 151-dir census depends on the answer.


§2.2's "there is no override" is true **of corrosion**, and it is worth stating
that it does not generalise to the tree. Corrosion's directory always carries a
hash — `<folder>_<sha1[0:5]>`, e.g. `nano-ros_0b88c`, `nros_ws_runtime_14eac`.

Issue 0493 measured `examples/workspaces/mixed/build-workspace-fixtures` and
found the cargo artifacts under a **hashless** `cargo/build`, with two
`-C metadata` identities of ten crates in ONE `deps/`. Hashless means it is not
corrosion's naming: nano-ros has its own cargo invocations that pass
`--target-dir` directly (`_nros_ffi_cargo_args`,
`cmake/NanoRosCodegenCore.cmake:348`), and those are not bound by §2.2 at all.

Two consequences for this phase:

1. **The 151-dir census counts corrosion trees.** Invocations on the nano-ros
   path are separated by whatever `TARGET_DIR` their caller chose, which may be
   shared. Relocation work keyed on §2.2 alone will not reach them.
2. **The isolation §2.2 describes is what prevents the 0493 link failure**, and
   it is absent on that path — same `deps/`, two workspace roots, provider
   bundles both. Whether the two topologies (hashed per-workspace vs a shared
   hashless dir) coexist by design or one is drift is **unestablished**; a bisect
   over the same-day changes is the cheap way to find out and has not been done.

### 2.3 A cmake binary dir cannot be shared across source dirs

Verified, not assumed — `CMakeCache.txt` pins its source tree:

```console
$ grep -m1 CMAKE_HOME_DIRECTORY examples/native/c/action-client/build-zenoh/CMakeCache.txt
CMAKE_HOME_DIRECTORY:INTERNAL=/…/examples/native/c/action-client
$ grep -m1 CMAKE_HOME_DIRECTORY examples/native/c/talker/build-zenoh/CMakeCache.txt
CMAKE_HOME_DIRECTORY:INTERNAL=/…/examples/native/c/talker
```

One binary dir serves exactly one source dir. So the coordinate naming a cmake
cache **must be at least as fine as today's (leaf × rmw)**, and a relocation is a
bijection onto today's directories.

### 2.4 Therefore: relocation is disk-neutral, and the prize is not reachable by it

Compose 2.2 and 2.3. The cargo trees are separated *because* the cmake dirs are;
the cmake dirs cannot merge; corrosion's target dir cannot be pointed anywhere
else. So:

> **Moving 182.3 GiB from `examples/**/build-*` to `$NROS_BUILD_ROOT/cmake/…`
> frees zero bytes and enables no sharing.**

And the prize it does not reach is large. Across the 132 corrosion trees:

| | |
| --- | ---: |
| `deps/` rlib + rmeta files | 96 663 |
| distinct file **names** (cargo's own identity judgement — `-C metadata` is in the name) | 1 931 |
| **duplication** | **50.1 : 1** |

That is a higher ratio than either figure the cargo half measured (21.8:1 disk,
27.9:1 in `deps/`). It is the single largest remaining duplication in the tree,
and **this phase cannot cash it**.

### 2.5 The decision

**P3 proceeds as a pure R1/R2/R3 compliance move, budgeted as such — not as a
disk phase.** Its value is exactly what RFC-0070 claims for itself, and that
claim was never about bytes:

> "That literal count is the actual problem. A path convention with 236
> spellings cannot be changed, and it cannot be *verified* — which is issue
> 0196's class … expressed as directory names."

Concretely it buys: one root; a coordinate that attributes every cmake artifact
to its manifest row (already 95.4 % true, and the remaining 4.6 % is a real
attribution bug worth fixing on its own); a single `rm -rf` reap point where
today there are 151; and **P4**, the `.gitignore` collapse, which is blocked on
nothing else once these paths leave the source tree.

**It does not buy disk, and the phase doc says so up front so that no later
reader budgets against a number that is zero.** The 151.7 GiB is spun out as
item C below rather than smuggled into this phase's acceptance.

### 2.6 The R2 tension this surfaces

RFC-0070 R2 says a cache dir is `<kind>/<coordinate>` where the coordinate uses
"the fixture-manifest vocabulary already in use — platform, lang, rmw,
feature-sig — **and nothing else**."

By 2.3, that vocabulary **cannot name a cmake cache**: `native/c/zenoh` is one
coordinate and 27 distinct `build-zenoh` dirs, which cannot be one directory. The
coordinate for `kind = example` must additionally carry the **leaf**. This is not
an ad-hoc suffix — it is a missing axis, and the manifest already has it as
`dir`. R2 should be read as "the manifest's vocabulary", of which `dir` is part,
and the RFC's sketch of the axes is illustrative rather than exhaustive. Worth an
RFC amendment when this lands, so the next reader does not treat the leaf
component as the suffix zoo regrowing.

---

## 3. Rejected options, with the evidence

**A. Merge cmake binary dirs across leaves to cash the corrosion duplication.**
Rejected: `CMakeCache.txt` pins `CMAKE_HOME_DIRECTORY` (§2.3). Not a tuning
question — cmake refuses.

**B. Point corrosion's cargo dir at the shared cargo group via `CARGO_TARGET_DIR`
or `build.target-dir`.** Rejected by reading corrosion (§2.2): it passes
`--target-dir` on the command line, which overrides both.

**C. Symlink `<CMAKE_BINARY_DIR>/cargo` at a shared group dir.** *Not rejected —
deferred, with its hazard measured.* It needs no corrosion change and no flag
change, and symlinking the `cargo` **parent** (rather than the
`nano-ros_0b88c` child) is robust across corrosion layouts — worth noting that
the provisioned corrosion at `~/.nros/sdk/corrosion` uses `cargo/build` with no
hash at all, so any derivation keyed on the hashed name would be wrong on a
provisioned host. But merging these trees walks straight into phase-340 W1's
refutation: a group's members share a **flat artifact namespace** and cargo does
not hash the final artifact name.

| unhashed artifact | copies | distinct sizes |
| --- | ---: | ---: |
| `libnros_c.a` | 132 | **15** |
| `libnros_cpp.a` | 132 | **15** |
| `libnros_ws_runtime.a` | 3 | 3 |

Fifteen genuinely different archives under one name, and corrosion **copies
byproducts out of** `cargo_build_dir` into the cmake tree — so a last-writer-wins
there is not confined to the shared dir, it is propagated into every consuming
build. This needs B2's variant-grained group key applied to the corrosion call
site, and its own rebuild acceptance. **It is the 150 GiB item, it is real, and
it belongs in its own phase**, not smuggled into a relocation.

**D. Fork corrosion to add a target-dir option.** Viable — the repo has a
vendored-fork workflow — but it is an upstream change orthogonal to R1, and C
reaches the same place without one. Revisit only if C's symlink proves fragile.

**E. Give the six threadx-riscv64 rust-through-cmake rows a `build_subdir`.**
This is P2's precedent (prefer a row over a call-site rule) and is the right
shape, but it does not work as-is: `cmake_build_subdir()` sits behind
`is_cargo_row()`, which keys on `lang`. The fix is a **builder-shaped** predicate,
not a language-shaped one. Scoped as W2 below rather than rejected.

---

## 4. Work plan — ordered, with disk budgets

**The binding constraint is disk, and it is tight.** The volume holding the
checkout: **916 G total, 801 G used, 69 G free (93 %)**. Migration is additive in
transit — the new location fills while the old dir persists — and phase-340
records four platforms at once exhausting this same volume and producing five
phantom "lane failures" that were all `No space left on device`.

Against 69 G free, and with no reclaimable residue in this population (§1.4):

| wave | family | dirs | size | additive transit fits? |
| --- | --- | ---: | ---: | --- |
| W3 | `build-xrce` | 12 | 18.9 GiB | **yes** |
| W4 | `build-workspace-fixtures*` | 26 | 37.1 GiB | yes, barely — reclaim first |
| W5 | `build-cyclonedds` | 36 | 38.2 GiB | yes, barely — reclaim first |
| W6 | `build-zenoh` | 76 | 88.1 GiB | **NO** — must delete-on-verify per leaf |

**W1 — decide the derivation shape. (This document. Done.)**
RFC-0070 step 2 names this an explicit precondition for touching the in-source
class. Recorded above: the coordinate must carry the leaf (§2.6); the value is
compliance, not bytes (§2.5); `nros_build_dir cmake …` is the emitter.

**W2 — close the 7-dir attribution gap. No paths move.**
Replace the `lang`-keyed `is_cargo_row()` with a builder-keyed predicate so the
six `qemu-riscv64-threadx/rust/*` rows report their cmake dir from
`row_artifact_root()`, and decide whether the `nros-rmw-cyclonedds/build` stray
gets a row or an allowlist entry. Acceptance is that `row_artifact_root()` covers
**151/151**, measured the same way §1.5 measures it. Must precede W3 — a
migration that redirects six writers whose readers are unattributed is P2's
exact failure.

**W3 — move `build-xrce` (12 dirs, 18.9 GiB).** The only family that fits the
headroom additively, so it is the pilot regardless of any other ordering
argument. One commit carrying build + staleness probe + test resolver together
(#393). Acceptance: `just setup-cli` → the xrce lane build → the tests that
**consume** those fixtures → `find examples -type d -name 'build-xrce'` is empty
→ then delete the old trees.

**W4/W5/W6 — the remaining families, in the table's order.** W6 cannot transit
additively and must delete each leaf's old dir as its new one verifies.

**W7 — the gate + P4.** RFC-0070 step 4: fail on a `build-*` directory inside a
source tree and on a literal cache path in a script, with the `third-party/`
allowlist the RFC's Open section requires. Then phase-340 P4 deletes the
`.gitignore` block. **Tripwire both directions and confirm the perturbation
actually reaches the code path** — phase-340 records four malformed tripwires
that looked green (an edit landing in a docstring; a path bogus enough that the
guard refused for the wrong reason; a second `[lib]` making invalid TOML; a glob
scanning `_deps/` instead of sources).

**Not in this phase — item C (§3), the 151.7 GiB corrosion consolidation.** It is
the largest single duplication measured anywhere in this repository (50.1:1) and
it is independent of every wave above.

### Standing traps for whoever runs the waves

* **Acceptance is a rebuild, never a gate.** phase-340 P2 records gate-level
  checks green for all six platforms while the build was broken.
* **`row_artifact_root()` is repo-relative**, and it is what the coordinate-scoped
  test run inverts. `NROS_BUILD_ROOT` may point outside the repo, at which point
  "repo-relative" stops being expressible. Settle that in W3, not in W6.
* **A STALE verdict is absorbing** (issue 0445) — read the `probe:` and `NOT RUN`
  lines before believing one.
* **Read `target/nextest/default/junit.xml`, not the console summary** — the
  console counts `skip!` panics as failures (71 reported vs 12 in junit).

---

## 5. What could not be verified

**No rebuild was possible in this worktree, and therefore no path was moved.**
`just setup-cli` succeeds here (22 s, exit 0), but `third-party/` holds only
empty submodule directories — `third-party/xrce/agent` and
`third-party/zenoh/zenoh` are both empty — so no cmake fixture that needs an RMW
SDK can configure, let alone build. The brief's acceptance rule for this phase is
a real rebuild plus the tests that consume the fixtures; with that unavailable,
landing a path move would have meant shipping exactly the class of change
phase-340 P2 caught only at rebuild time.

Everything in §1 and §2 is a read-only measurement of the provisioned main
checkout and reproduces from the commands shown. Specifically **not** established
here:

* that a relocated cmake cell builds, or that its tests find the artifact;
* item C's symlink actually working — its hazard is measured (15 distinct
  `libnros_c.a`), its feasibility is not;
* whether the six threadx-riscv64 rows have any other reader that would need to
  move with them (W2 must sweep for that, not assume the count of writers is the
  count of sites).
