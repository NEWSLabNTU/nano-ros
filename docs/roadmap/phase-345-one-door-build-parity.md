# Phase 345 — One door: the build behaves the same however you enter it

**Status (2026-08-10). OPEN — nothing landed; the measurements below are done
and reproduce on this tree.** This phase is not a build-cache phase. It does not
move a path, so it does not collide with [phase-340](phase-340-build-artifact-reuse.md)
item 5 / P4 — with one exception, W2, which edits leaf `.cargo/config.toml`
files that item 5's grouping work reads. That fence is stated in "Sequencing".

**Closes:** issue 0451, issue 0491, issue 0452. **Advances (does not close):**
issue 0374 — its remaining direction 1 is out-of-repo and stays open.
**Touches:** RFC-0054 (the C headers are the ABI SSoT — this phase fixes the
*other* direction), RFC-0048 W9 (`nros sync` owns the leaf `.cargo/config.toml`),
RFC-0026 (copy-out examples are the user story W1 exists for).
**Related:** issue 0407 and issue 0420 (the same class, previously fixed one site
at a time), issue 0457 / 0463 (the tracked-vs-sidecar origin rule W2 must not
violate), issue 0466 (the tier-1 setup contract this makes statable).

---

## 1. The class

Five issues across three areas are the same defect: **a build that works when
entered through `just` behaves differently, or fails, when entered through
`cargo` / `cmake` / a copied-out leaf.** The repo names one door as the SSoT —
CLAUDE.md's pitfall index says "Activate files are the env/PATH SSoT" — and that
claim is currently false for the variables that matter most.

| issue | door A | door B | what differs |
| --- | --- | --- | --- |
| 0451 | `just <plat> build-examples` | `cargo build` in the leaf | 23 env vars exist only in door A |
| 0491 | leaf built alone | leaf built beside its siblings | the same var carries a different STRING per leaf |
| 0452 | any embedded lane | a clean worktree | two tracked headers get rewritten |
| 0407, 0420 | *(already fixed, one site each)* | | the precedent that this is a class |

CLAUDE.md's own rule applies to the phase itself: fix the class, not the site.
0407 and 0420 were each fixed where the symptom appeared, which is why 0451
exists.

## 2. Measurements (2026-08-10, this tree)

### 2.1 The env split — 0451

`just/sdk-env.just` carries **23** `export` lines. `activate.sh` carries **zero**
of them:

| origin of the default | vars |
| --- | --- |
| `third-party/` SDK root | 8 |
| first-party `packages/` source or include dir | 8 |
| board config dir (`packages/boards/*/config`) | 3 |
| esp-idf workspace | 2 |
| literal or derived (`FREERTOS_PORT`, `IDF_PATH`) | 2 |
| **total** | **23** |

`activate.sh` exports `NROS_REPO_DIR`, `nano_ros_ROOT`, `NROS_CARGO_FLAGS`,
`PYO3_USE_ABI3_FORWARD_COMPATIBILITY` and several `PATH` prefixes — and nothing
else. `.envrc` is a thin `source "$PWD/activate.sh"`, so direnv users get exactly
the same set, i.e. also none of the 23.

Every one of the 23 has a correct repo-relative default. The SDKs are sitting at
those paths. The build fails anyway, one variable per attempt, and — per 0451 —
the NuttX flavour of the failure reaches the LINKER and reads as
`undefined reference to open / socket / ioctl / malloc`, which is what it was
mistaken for during phase-338.

**`activate.fish` is a HAND-MIRRORED sibling** (`.envrc` says so in its own
comment). So the naive fix — paste 23 exports into `activate.sh` and 23 more into
`activate.fish` — creates a 46-line hand-mirror of a 23-line SSoT. That is the
mirror-drift class, not a fix for it. W1 is written to forbid that shape.

### 2.2 One variable, three spellings — 0491

Two of the 23 (`NROS_PLATFORM_FREERTOS_SRC`, `NROS_PLATFORM_CFFI_INCLUDE`) are
ALSO written into **13 tracked leaf `.cargo/config.toml` files**:

| leaves | family |
| --- | --- |
| 6 | `examples/qemu-arm-freertos/rust/*` |
| 6 | `examples/qemu-riscv64-threadx/rust/*` |
| 1 | `packages/testing/nros-tests/bins` |

as `{ value = "../../../../packages/…", relative = true }`. `relative = true`
roots the value at THAT leaf, so 13 leaves hand their build scripts 13 different
strings naming one directory, and `cargo:rerun-if-env-changed` compares them
**textually**. Issue 0491 measured the consequence: six sibling rows in one
shared cargo group, five dirty on pass 1 and all six on pass 2, indefinitely.

Note the relative values resolve to the repo root (`../../../../` from a
`examples/<plat>/rust/<leaf>` leaf). **They therefore do nothing for a
copied-out example**, which is the user story RFC-0026 defines and the one
argument for keeping them. Deleting them costs copy-out nothing.

### 2.3 The precedence hazard, measured — W1 breaks W2's rows if landed alone

Cargo's `[env]` defaults to `force = false`. The consequence is not documented
anywhere in this repo and it decides W1's shape, so it was measured rather than
cited — a throwaway crate with a leaf `[env] relative = true` row and a build
script that echoes the value:

```console
$ env -u NROS_ENVTEST cargo build          # door A: no ambient value
SAW=<leafdir>/leafrel                      #   the leaf row wins

$ NROS_ENVTEST=/abs/from/activate cargo build   # door B: activate.sh exported it
SAW=/abs/from/activate                     #   the AMBIENT value wins, and the
                                           #   build script re-ran because the
                                           #   string changed
```

**So exporting the 23 from `activate.sh` silently overrides all 13 leaf rows.**
That is half a fix and half a new bug:

* it *removes* 0491's thrash for anyone who sourced `activate.sh` — every leaf
  now sees ONE absolute string;
* it *creates* a sourced-vs-unsourced thrash — alternating between an activated
  shell and a bare one flips the string and re-runs every affected build script.

W1 without W2 is therefore not a partial improvement; it relocates the churn.
They land together or in the stated order, never W1 alone.

### 2.4 cbindgen drifts because two graphs resolve it independently — 0452

The Rust→C header generation runs **from `build.rs`, into a COMMITTED source
directory**:

| | |
| --- | --- |
| generator | `packages/tooling/nros-build-helpers/src/c.rs:418`, `…/cpp.rs:407` |
| destinations | `packages/api/nros-c/include/nros/nros_generated.h`, `packages/api/nros-cpp/include/nros/nros_cpp_ffi.h` (both tracked) |
| dependency form | `cbindgen = "0.29"` — a **library** dep, caret range, in `nros-build-helpers` and `nros-zpico-build` |
| root `Cargo.lock` resolves | **0.29.3** |
| an embedded leaf actually built | **0.29.4** — observed as `packages/testing/nros-bench/wake-latency-cortex-m3/target/release/build/cbindgen-*/out/tests.rs` referencing `…/cbindgen-0.29.4/…` |
| why the leaf may differ | that leaf has **no tracked `Cargo.lock`** (`git ls-files` on it lists `.cargo/config.toml`, `.gitignore`, `Cargo.toml`, `build.rs`, `memory.x`, `package.xml`, `src/*` — no lock), so it resolves the caret freshly |

That is the whole mechanism of 0452: **the root lock does not govern the graph
that writes the tracked header.** 0.29.4's output uses the narrower
`#ifdef __cplusplus` enum-base guard where the committed header uses the C23
`__STDC_VERSION__ >= 202311L` form, so ~36 lines flip on every embedded lane,
and committing them reverts an upstream improvement (it had to be hand-reverted
twice during phase-338).

Pinning a version is therefore necessary but **not sufficient** — a build script
writing into tracked source will dirty the worktree the next time any tool
version, feature set or cbindgen default moves. The repo already has the correct
shape for exactly this, in the *opposite* direction:

| direction | generator | invoked by | pinned | gated |
| --- | --- | --- | --- | --- |
| C header → Rust | bindgen | `scripts/gen-abi-bindings.sh`, by hand | **yes**, bindgen-cli 0.72.1 | `check-abi-bindings` |
| Rust → C header | cbindgen | **`build.rs`, on every build** | **no**, caret `0.29` | none |

`.clang-format-version` + `just setup-clang-format` is the same precedent again,
and its stated reason ("output drifts between major versions … an unpinned PATH
binary produces spurious diffs across machines") is verbatim this problem.

## 3. Work items

### W1 — the SDK env has one definition, reachable from both doors

- [ ] Move the 23 defaults to a **single machine-readable source** consumed by
      `activate.sh`, `activate.fish` and `just/sdk-env.just` alike. Any shape
      where a human maintains the list twice is rejected on sight — the
      `activate.fish` hand-mirror is the reason.
- [ ] `just/sdk-env.just` READS that source; it must not keep its own copy of a
      default. `env(NAME, <default>)` stays, so an explicit user override still
      wins over both.
- [ ] Keep the loud panics. Reword the ones that remain reachable: a variable
      that is deliberately recipe-scoped must say
      `set by 'just <platform> …'; not exported by activate.sh`, because a bare
      "not set" tells the reader they forgot something they never had.
- [ ] Update the CLAUDE.md pitfall line, which currently promises what W1
      delivers and today does not.

**Gate:** `check-sdk-env-ssot` — for every variable in the generated source,
assert (a) `sdk-env.just` names it, (b) `activate.sh` exports it, (c)
`activate.fish` exports it, (d) the defaults are byte-identical after
`$repo`-root normalisation. The three-file mirror is exactly what a gate is for.

**Acceptance:** in a clean shell with only `source ./activate.sh`, a bare
`cargo build` in each of the embedded example leaves 0451 names gets past the env
stage. Not "builds" — some need a toolchain — **gets past the env stage**, which
is the claim under test.

### W2 — the leaf `[env]` rows stop being a second spelling

- [ ] **Delete the `relative = true` rows from the 13 leaves** and let the W1
      variable serve them. §2.2 establishes they buy copy-out nothing; §2.3
      establishes that leaving them in place while W1 exports the same names
      makes them dead weight that still churns when the ambient value comes and
      goes.
- [ ] Verify the cmake/corrosion path for those families passes the vars
      explicitly, or make it do so — a leaf built through cmake must not depend
      on the shell that launched it.
- [ ] Re-run 0491's A/B/C probe (talker alone; six siblings in order; build a
      sibling then re-probe talker). All three must report fresh.

**Why not the alternatives**, recorded so they are not re-proposed:

| option | why rejected |
| --- | --- |
| keep the rows, make values absolute via `nros sync` | absolute host paths in a **tracked** file is precisely the origin split issues 0457/0463 settled — host-derived content belongs in the gitignored sidecar, and this content is not even host-derived, it is repo-relative |
| keep relative, canonicalize in the build script | does not help: `rerun-if-env-changed` compares the string cargo stores, before any consumer sees it |
| `force = true` on the rows | inverts §2.3 — the leaf would override an explicit user/CI value, which is worse than the bug |

**Gate:** extend `check-cargo-config-tracked` — no leaf `[env]` row may name a
variable owned by the W1 source.

**Acceptance:** 0491's probe C is fresh, AND the same probe is fresh in a shell
that never sourced `activate.sh` (the case §2.3 created).

### W3 — the Rust→C headers get the treatment the C→Rust ones already have

- [ ] Pin the generator: `.cbindgen-version` + `just setup-cbindgen`, mirroring
      `.clang-format-version` / `just setup-clang-format`. Record **0.29.3** (the
      root lock's answer, which produced the committed headers) unless a
      regeneration shows otherwise — verify before pinning, do not assume.
- [ ] **Stop `build.rs` writing into tracked source.** The build emits to
      `OUT_DIR`; a `just regen-c-headers` recipe writes the committed copies,
      the way `scripts/gen-abi-bindings.sh` does for the other direction.
- [ ] `check-cbindgen-headers` — regenerate with the pinned binary, diff against
      the committed headers, fail on drift. Same shape as `check-abi-bindings`.

**Acceptance:** `git status` is clean after `just nuttx build-examples` and after
`scripts/build/fixtures-build.sh nuttx cpp` — the two lanes 0452 names. Assert it
in the gate, not by eye: a lane that dirties the worktree is a failing lane.

**Note the two halves are separable and the ORDER matters.** Pinning alone
(without the `OUT_DIR` move) still leaves a build script writing tracked files —
it just makes them agree today. Moving alone (without the pin) makes
`check-cbindgen-headers` fail differently on different machines. Land the pin
first so the gate has a fixed point, then the move.

### W4 — a source recipe stops pulling a second Rust toolchain (issue 0374, direction 4)

- [ ] Make `nros setup`'s source recipes build with the workspace's pinned
      toolchain (`RUSTUP_TOOLCHAIN` / `cargo +<pin>`) instead of letting the
      checkout's own `rust-toolchain.toml` trigger a rustup sync — 0374 measured
      `1.85.0` being fetched for zenohd alongside the nano-ros pin.
- [ ] **Measure before committing to it**: zenoh 1.7.2 may not build on the
      nano-ros pin. If it does not, the deliverable is the *diagnostic* — name
      the extra toolchain and its size in the existing
      `warn_source_builds` heads-up — not a forced pin that breaks the build.

**Explicitly out of scope:** direction 1 (seed `1.7.2-nros2` assets on
`NEWSLabNTU/nano-ros-sdk`). It is not fixable in this repository. **Issue 0374
stays open when this phase archives** — say so in the archive note rather than
closing it on a partial.

## 4. Sequencing

```
W1 ──▶ W2        (§2.3: W1 alone relocates the churn)
W3 (independent — different subsystem, no path or env overlap)
W4 (independent)
```

**Fence against phase-340.** W2 edits leaf `.cargo/config.toml` files that
phase-340 item 5's shared-`--target-dir` grouping reads. Land W2 **before** item
5 starts or **after** it lands, never during — two conventions in flight over one
file is the #393 failure mode. W1, W3 and W4 touch nothing item 5 touches and
may run in parallel with it.

## 5. Tier

W1/W2 change what every embedded build sees in its environment, and W3 changes a
committed header: that is `packages/core` + `cmake/`-adjacent, so **tier 2
(`just ci-matrix`)** per RFC-0061, with `just build-test-fixtures lane=tier2`
first. W3's acceptance additionally requires running the two named embedded lanes
and checking `git status` — tier 1 cannot see it, because tier 1 does not build
NuttX.

## 6. What is NOT verified yet

* **W1's variable list is 23 today.** It was 23 when measured on 2026-08-10; the
  list moves. The gate must derive it, not hardcode a count.
* **The cbindgen pin value (0.29.3) is inferred**, from the root lock plus the
  committed headers' C23 guard being the newer form. Regenerate with 0.29.3 and
  diff before writing the pin file — if the committed headers came from
  something else, the pin is wrong and the gate will enshrine the wrong output.
* **Whether the cmake/corrosion path for the freertos and threadx families
  already passes the two `NROS_PLATFORM_*` vars explicitly** — W2's second
  checkbox is written as a verification for that reason, not as an assumption.
* **Whether zenoh 1.7.2 builds on the nano-ros pinned toolchain** — W4's blocker,
  deliberately unmeasured here because the measurement costs a full zenohd source
  build.
