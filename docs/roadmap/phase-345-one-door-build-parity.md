# Phase 345 — One door: the build behaves the same however you enter it

**Status (2026-08-10). COMPLETE: W1, W3 and W4 LANDED; W2 RETRACTED (its cause
was fixed elsewhere — see W1's section). Archive once a tier-2 sweep confirms
the activation change across the matrix. The measurements below are done
and reproduce on this tree.** This phase is not a build-cache phase and moves no
path, so it never collided with [phase-340](phase-340-build-artifact-reuse.md)
item 5 / P4. The one item that would have — W2, which proposed editing leaf
`.cargo/config.toml` files that item 5's grouping work reads — is retracted, so
the fence described under "Sequencing" is moot.

**Closes:** issue 0451, issue 0452. **Does NOT close** issue 0491: another
session fixed it first, by content-fingerprinting rather than by this phase's
proposed edit (§2.2). **Advances (does not close):**
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

> **CORRECTION 2026-08-10 — the table below is WRONG and is kept only because
> the conclusion it drove was still right.** "`activate.sh` carries zero of
> them" came from grepping `activate.sh` for literal `export FREERTOS_*` lines.
> It does not carry them literally: it *sources* `scripts/sdk-env.sh`
> (`activate.sh:253`), which evaluates the just SSoT. Measured properly — clean
> environment, source, count — activation delivered **14 of 23 under bash**.
> A grep answered a question about a file; the question was about a shell.
> The real defect and the real numbers are in §2.1a.

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

### 2.1a What was actually broken (measured 2026-08-10, clean environment)

`activate.sh` sources `scripts/sdk-env.sh`, which evaluates the SSoT — so the
mechanism was right and the *coverage* was not. Three independent lists decided
which variables survived activation, and each dropped a different set:

| shell | delivered | why the rest were missing |
| --- | --- | --- |
| bash | **14 / 23** | `sdk-env.sh` carried a HAND-WRITTEN array of 14 names beside a 23-name SSoT. The nine omitted are exactly the first-party ones (`NROS_PLATFORM_*_SRC`, `NROS_C_INCLUDE`, `NROS_CPP_INCLUDE`, `NROS_LAN9118_LWIP_DIR`, `NROS_VIRTIO_NET_NETX_DIR`, `TBAND_DIR`) |
| fish | **2 / 23** | `activate.fish` dumped the subshell's whole `env` and imported only names matching `NROS_*` — dropping every third-party SDK root (`FREERTOS_DIR`, `NUTTX_DIR`, `THREADX_DIR`, `IDF_PATH`, …) |
| zsh | **0 / 23** | `sdk-env.sh` used bash-only indirect expansion (`${!name}`); zsh answers `bad substitution` and carries on. Its sourced-vs-executed test was bash-only too, so a sourced zsh took the "print the exports to stdout" branch and set nothing |

So 0451's report — "these are set only by the `just` recipes" — is right about
the *symptom* and wrong about the *cause*: activation had a delivery mechanism,
and three separate copies of "which variables" quietly disagreed with the SSoT.
Which is the mirror-drift class again, three times over, in one file each.

**`check-activate-shells.sh` passed the whole time.** It asserted the files run
to COMPLETION — and they did, in every shell. Completion is not delivery: the
zsh arm printed a `bad substitution` diagnostic, reached the last line, and the
gate said `ok: zsh`. This is issue 0196's rule (a gate whose coverage is
narrower than the rule it enforces) applied to activation.

### 2.2 One variable, three spellings — 0491

> **SUPERSEDED 2026-08-10 — 0491 was fixed by a different session, with a better
> mechanism, while this phase was open.** Path-valued build inputs are now
> fingerprinted by their CONTENT (`packages/tooling/nros-build-paths`); every
> `cargo:rerun-if-env-changed` on a path-shaped name is gone (16 Rust files / 57
> sites plus three platform manifests), gated by
> `scripts/check-path-env-fingerprints.py`. Their write-up is worth reading: the
> spelling mismatch was between the `just` (absolute) and leaf-relative forms,
> so it fired even for a SINGLE row — build under `just`, probe without it — and
> no generator-side normalisation could have fixed it.
>
> Two consequences for this phase, both load-bearing: the thrash W2 existed to
> stop is gone, and **§2.3's precedence hazard is defused** — a differing
> spelling no longer reaches any fingerprint, so it can no longer cause a
> rebuild. W2 is retracted below on that evidence.

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

### W1 LANDED 2026-08-10 — and W2 is RETRACTED

**W1 shipped as "stop having three lists", not "move the defaults".** The
defaults never needed to move: `activate.sh` already sourced `scripts/sdk-env.sh`,
which already read the `just/sdk-env.just` SSoT. What it did not do was cover
the SSoT. Three fixes, one per list (§2.1a):

* `sdk-env.sh` **derives** the variable names from `just/sdk-env.just` instead of
  restating 14 of them, and takes their values from a single `just --evaluate`
  (previously one `just` subprocess per variable — 14 justfile parses on every
  activation, now one);
* `activate.fish` consumes `sdk-env.sh --fish`, whose list comes from the same
  SSoT, instead of dumping `env` and filtering on `NROS_*`;
* `sdk-env.sh` works under zsh: portable `eval`-based "is this set" in place of
  bash-only `${!name}`, its own path resolved via `${(%):-%N}` when
  `BASH_SOURCE` is absent, and a sourced-vs-executed test that knows about
  `ZSH_EVAL_CONTEXT`.

| shell | before | after |
| --- | --- | --- |
| bash | 14 / 23 | **23 / 23** |
| fish | 2 / 23 | **23 / 23** |
| zsh | 0 / 23 | **23 / 23** |

**Gate: `check-activate-shells.sh` extended, not duplicated.** It already owned
the shell matrix, so it grew a second sentinel line (`PROBEVARS`) listing any
SSoT variable left unset. Two things had to change for that assertion to mean
anything:

* the variable list is **read from the SSoT**, and the gate fails loudly if it
  parses to empty — a gate that checks nothing must not report OK;
* every probe now runs under **`env -i`** (keeping only `HOME` and `PATH`).
  A maintainer host almost always has direnv active, so the probe inherited the
  very variables it was checking — **22 of 23 arrived that way**, and the first
  version of this assertion passed for that reason. Caught by tripwiring it.

Tripwired live, both arms, after the isolation fix: restoring the fish `NROS_*`
filter fails 2 fish cases; restoring the bash-only indirect expansion fails 2 zsh
cases; green with both reverted.

**Acceptance.** In a clean environment with only `source ./activate.sh`, a bare
`cargo build` in `examples/qemu-arm-freertos/rust/talker` — a copied-out-shaped
embedded leaf — now **completes** (exit 0), rather than merely getting past the
env stage. Under zsh the same build also exits 0, though that run was cached, so
the load-bearing zsh evidence is the 0→23 variable measurement, not the build.

**W2 is RETRACTED, on two independent grounds.**

1. **Its purpose is gone.** W2 existed to stop 0491's thrash and to defuse
   §2.3's precedence hazard. Both were dissolved by 0491's content-fingerprinting
   fix (§2.2), which explicitly leaves the leaf `[env]` blocks in place as the
   authored half of RFC-0048 W9.
2. **Deleting them would now cost something.** Twelve rows across six FreeRTOS
   leaves carry `NROS_PLATFORM_CFFI_INCLUDE` and `NROS_PLATFORM_FREERTOS_SRC`.
   Both are in the SSoT and both are now exported by activation — but the leaf
   rows are what make those two resolve **without** activation, which is exactly
   0451's scenario (an IDE, a forgotten `source`, an agent narrowing a failure).
   Removing them would make two more variables depend on a sourced shell for no
   gain beyond tidiness.

So the "three spellings" framing was right about the *lists* and wrong about the
*values*: the duplication worth removing was in `sdk-env.sh`, `activate.fish` and
the zsh path, and W1 removed it. Issue 0491 is closed by its own fix; this phase
should not reopen a decision that session made with better measurements.

### W1 (original plan) — the SDK env has one definition, reachable from both doors

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

### W3 LANDED 2026-08-10 — with two deviations from the plan below

**Deviation 1: the pin is an exact cargo requirement, not a `.cbindgen-version`
file plus a provisioning recipe.** §2.4 proposed mirroring `.clang-format-version`
/ `bindgen-cli`. That analogy does not transfer: those are PATH binaries with no
resolver, so they need a version file and a `just setup-*`. **cbindgen is a
cargo dependency**, so cargo's resolver IS the pinning mechanism —
`cbindgen = "=0.29.3"` in `[workspace.dependencies]`, inherited by both consumers
via `{ workspace = true }`. An exact requirement binds every graph including the
lockless leaves, which is exactly the population a lock could not reach, and a
separate pin file would have been a second spelling of the same fact.

Pin value **verified, not assumed**: built `nros-c` on the root lock's 0.29.3 and
the committed headers came back byte-identical. §6's caveat is discharged.

**Deviation 2: there are THREE committed cbindgen headers, not two.** The sweep
for the class (CLAUDE.md's rule) found `packages/rmw/zenoh/zpico-sys/c/include/
zpico.h`, tracked and rewritten in place by `nros-zpico-build`'s own
`generate_header` — same defect, different crate, unnamed in issue 0452. It is
not raw cbindgen output: it goes through `post_process_header` and a
plausibility guard, so the regenerator carries a per-header post-pass rather
than assuming all three are alike.

**What shipped**

| piece | where |
| --- | --- |
| exact pin, inherited | `Cargo.toml` `[workspace.dependencies]`, both consumers on `{ workspace = true }` |
| builds COMPARE, never write | `nros_build_helpers::generate_cbindgen_header` + `nros-zpico-build`'s `generate_header` — a mismatch is a `cargo::warning`, and the fresh copy is stashed in `OUT_DIR` for diffing |
| the single writer | `just regen-c-headers` → the `nros-cbindgen-headers` **crate** (its own member, not a `[[bin]]` in `nros-build-helpers` — see "What the acceptance caught") |
| gates | `check-cbindgen-pin` (exact req / inherited / lock agrees) and `check-cbindgen-headers` (all three match a fresh generation), both in `check-fast` |

`Cargo.lock` moved by exactly one line (the new optional dep edge), via
`just lock-update` — the only sanctioned mover.

**Verified**

* all three gate arms of `check-cbindgen-pin` fail before being trusted (caret
  regression, a crate re-spelling its own version, pin ahead of the lock);
* `check-cbindgen-headers` fails on a drifted header and `just regen-c-headers`
  restores it;
* **the write path is gone**: appended drift to `nros_generated.h`, rebuilt
  `nros-c`, and the drift SURVIVED with a STALE warning — previously the build
  would have overwritten it;
* `cargo clippy -D warnings` clean on both changed crates; `cargo +nightly fmt`
  applied.

**What the acceptance caught — and it was a real defect, not a flake.**

The first shape put the regenerator binary inside `nros-build-helpers`, taking
`nros-zpico-build` as an *optional* dependency behind `required-features`. That
looked free: default builds never enable it. It is not free. **A dependency EDGE
is recorded in every tracked lockfile that contains the crate**, feature-gated or
not, and `nros-build-helpers` appears in three tracked locks (the root plus both
`nros-nuttx-ffi` leaves). The NuttX lane died on:

```
error: cannot update the lock file .../nros-nuttx-ffi/Cargo.lock
       because --locked was passed to prevent this
```

The `--locked` PATH shim (issues 0359/0378) did exactly its job — it refused to
silently rewrite a lock nobody had reviewed. Fixed structurally rather than by
churning three locks: **the regenerator is its own crate**
(`packages/tooling/nros-cbindgen-headers`), so a developer tool's dependencies
stay out of every build script's graph. The root lock gains one member; the two
leaf locks do not move at all. That is also the general rule this uncovered —
*adding any dependency to a crate that build scripts link is a lockfile change
across the tree*, which is worth remembering the next time something looks like
it belongs in `nros-build-helpers`.

**Two lane attempts before that were not results at all**, and both are the
absorbing-STALE pattern (issue 0445) rather than evidence:

1. `nros: unrecognized subcommand 'profile'` — the in-tree CLI was stale, so the
   lane exited before building anything. `git status` was clean afterwards and
   that cleanliness meant nothing. This is issue 0466's ordering trap: the rebase
   earlier in the session refreshed source mtimes, which re-arms the CLI stamp,
   and `just setup-cli` must come first.
2. The lockfile refusal above — again a precondition, again before any header
   could be written.

**The smoking gun, found by the third attempt.** With the dep problem fixed, the
lane still refused to build — and this time the pin was working as designed:

```
error: cannot update the lock file .../nros-nuttx-ffi/Cargo.lock
```

Both tracked NuttX FFI leaf locks pinned **`cbindgen 0.29.4`** while the root
pinned 0.29.3. Those are the graphs that were rewriting the headers, and the
drift was not hypothetical — **it was committed, in two lockfiles**. §2.4 inferred
this population from an OUT_DIR artifact in an unrelated bench leaf; here it is
in the tracked tree. The exact requirement refuses to build them until they
agree, which is the entire point. Moved with
`just lock-update cbindgen 0.29.3 <leaf>`; the diff is the version and its
checksum, nothing else.

**Acceptance DISCHARGED 2026-08-10, fourth attempt.** `just nuttx build-examples`
→ exit 0, zero errors, "NuttX QEMU examples built!", and:

* the three committed headers are **untouched** — `git status` clean on all of
  them after a lane that previously dirtied two;
* **zero** `is STALE against this crate` warnings, i.e. the committed headers
  also match what the embedded graph generates, not merely what the host does.

The first three attempts are kept above deliberately. Each produced a clean
worktree while proving nothing, and the difference between those and this one is
only that the lane actually compiled.

**W3 FOLLOW-UP 2026-08-10 — the pin left 14 tracked locks stale, and my gate
could not see them.** Introducing the exact requirement made every tracked lock
that predates it stale: `check-leaf-lockfiles` went red and **ci-matrix was
blocked** until another session moved them (`55db8934b`, 14 leaf locks, cbindgen
only — 14 version lines and 14 checksums, nothing else re-resolved).

The miss has a precise shape worth keeping. Before landing W3 I swept for locks
that would be affected and found **3** — because I grepped for locks containing
`nros-build-helpers`, reasoning about the dependency edge I had just added. The
invariant is not about that edge: it is about locks containing **cbindgen**, of
which there are **17**. Right question, wrong predicate, and the answer looked
authoritative.

`check-cbindgen-pin`'s third arm read the ROOT lock alone, so it could not have
caught them either — a gate whose coverage is narrower than the rule it enforces,
which is issue 0196's shape and which I had quoted at *another* gate earlier the
same day. It now checks every tracked lock carrying cbindgen, reports how many it
checked, and fails rather than reporting OK if that number is zero. Tripwired by
knocking a single leaf off the pin.

**Consequence worth noting:** the cross-process advisory lock in `shared.rs`
exists because N parallel build trees regenerated one source-tree path
(known-issues #15). Builds no longer write, so that race cannot occur; the lock
is kept only because the regenerator is still a writer, and it is no longer
load-bearing for #15.

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

### W4 LANDED 2026-08-10 — measured first, and the measurement said yes

W4's own text made the fix conditional: *"zenoh 1.7.2 may not build on the
nano-ros pin. If it does not, the deliverable is the diagnostic."* The gap is
wide enough that this was a real question — zenoh pins `channel = "1.85.0"`,
nano-ros pins `stable`, today **1.97.1**, twelve minor versions apart.

**It builds.** Escalating, cheapest first, on the store's own checkout:

| probe | result |
| --- | --- |
| `cargo metadata --locked` | ok — manifest and lock resolve under the newer cargo |
| `cargo check -p zenohd --locked` | 0 errors |
| the same plus `--features zenoh/transport_serial` (what the recipe passes) | 0 errors |
| `cargo install --path zenohd --root <tmp> --locked --features zenoh/transport_serial` | **exit 0**, 15 MB binary |
| `zenohd --version` | `zenohd v790faad built with rustc 1.97.1` |

The full `cargo install` was run rather than stopping at `check`, because a
type-check is neither a release build nor a link — and the binary reporting
`rustc 1.97.1` is direct evidence the override reached the compiler rather than
an inference from a green check.

**Shipped.** The executor (`sdk_store.rs`) sets `RUSTUP_TOOLCHAIN` for a source
recipe's configure and install steps, read from the workspace
`rust-toolchain.toml`. Two escape hatches, both deliberate:

* `respect_toolchain = true` on a `[tool.*.source]` keeps the checkout's own
  pin — a nightly-only crate cannot be built by a stable channel, and forcing
  one would turn a working recipe into a compile error;
* an unreadable or channel-less `rust-toolchain.toml` means **no override**,
  because guessing a toolchain is worse than the download it avoids.

Of the four `cargo install` recipes in the index (zenohd, sccache,
play_launch_parser, espflash), zenohd is the only checkout that pins a
toolchain today — but the fix is at the executor, so it covers the next one too.

The heads-up is corrected in the same change: it told the user a pinning recipe
"also makes rustup fetch that toolchain", which is now true only for a recipe
that opted out.

**Not fixed, and not pretended:** this host still carries the `1.85.0` toolchain
the old behaviour installed. W4 prevents the next download; it does not clean up
the last one. **Issue 0374 stays open** — its direction 1 (seeding the
`1.7.2-nros2` prebuilt on `NEWSLabNTU/nano-ros-sdk`) is out-of-repo, and that is
the direction that would stop the source build happening at all.

### W4 (original plan) — a source recipe stops pulling a second Rust toolchain (issue 0374, direction 4)

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
