# Phase 343 — The host build-dep graph: where the 240 GiB actually is

**Status (2026-08-10). MEASUREMENT COMPLETE; W1, W2, W3 LANDED. W4/W5 are
deliberately-not-doing items.** W1 (`7db7e72b5`) recovered 63.1 GiB, 33 % of the
phase; W2 confirmed by measurement that the cmake metadata probe was fixed for
free by W1 (4 private probe dirs in this tree, now 0); W3 was already widened by
phase-340 B1 and gained the standing tripwire it was missing. The header below is the
2026-08-08 measurement, unchanged because it still reproduces; what changed is
that the one thing this phase said was worth building has been built.

**W1 — the shared sizes-probe dir is now the DEFAULT.** The sharing mechanism had
existed since `82b31d08e` (2026-08-04), keyed correctly, and was reachable only by
exporting `NROS_SIZES_PROBE_TARGET_DIR`, which only `scripts/build/cargo.sh` does.
Everything else — a bare `cargo build` in a leaf, a nested cmake/corrosion probe,
an IDE — took the other branch and paid ~195 MiB for a private copy under
`$OUT_DIR`. Both branches were live in the same tree in the same week, the
wasteful one was the default, and there was no diagnostic either way. 425 leaked
probe dirs, 63.1 GiB, deduplicating 81:1. The new default is keyed IDENTICALLY to
the env branch — (rustc slug, target, features) — because phase-336 W7 keyed by
rustc slug alone, piled nine differently-featured `nros` rlibs into one directory,
and the mtime-newest fallback then handed a consumer another consumer's build.
Verified both directions with the env var explicitly unset (the case that leaked):
fix present, 0 private dirs; fix removed, 1.

That is the phase's recommendation carried out exactly as written — a wiring fix,
not a subsystem. **W2 and W3 remain**, and the decision below still stands: there
is no separate Wave-2-shaped phase to build, and the ~160 GiB of leaf + corrosion
target dirs belongs to phase-340 W2, not here.

---

**Original status (2026-08-08). MEASUREMENT COMPLETE, NOTHING IMPLEMENTED — deliberately.**
The 240.2 GiB / 91.1 % claim in [phase-340](archived/phase-340-build-artifact-reuse.md)'s
"Wave 2" reproduces (241.3 GiB / 91.1 % on a tree three days newer). **The
diagnosis attached to it does not.** Wave 2 named the population "the
feature-INVARIANT host proc-macro / build-dependency graph — one identity each"
and proposed "one shared host target-dir across leaves". Measured:

* The host graph is **not** feature-invariant and does **not** carry one identity
  each. **0 of 91** host-only crates have a single `-C metadata` identity;
  `syn` carries 45, `winnow` 32, `toml_edit` 31.
* **Cargo has no host target-dir.** `--target-dir` is documented as "Directory
  for all generated artifacts", and there is no host-scoped variant — not as a
  flag, not as a config key, not behind `-Z`. Two of the four mechanisms the
  wave proposed cannot be written.
* The single largest *addressable* block is not "the host graph across leaves"
  at all. It is **76.8 GiB inside 425 nested target dirs that build scripts
  create**, whose sharing mechanism **already exists, already has the right key,
  and is wired at exactly one site**. That is a wiring fix, not a subsystem.

So this phase does not open the program Wave 2 described. It **redirects** it,
records the decomposition that should have preceded it, and hands phase-340 back
one settled blocker (W2.a's A2). The recommendation is in "Decision" below and
the honest summary of it is: **there is no separate Wave-2-shaped phase to
build.** One wiring fix is worth 76.8 GiB and should be done; the rest is
phase-340 W2's already-decided mechanism applied to more platforms.

**Touches:** phase-340 (W2 mechanism, W2.a group key, W4 identity budget, W5
build-dep graph), phase-334 (build-cache layout), issue 0464 (the probe's
removed fallback), issue 0400 (host/box target-dir split).

## Method, so the numbers can be re-checked

Read-only walk of the provisioned checkout (the main working tree, not this
worktree — a fresh worktree has none of these dirs),
2026-08-08: every `target*` / `build*` directory under `examples/` (366 of
them), `lstat` on every regular file, recording size **and inode**. No build was
run and nothing was written into the tree that was measured.

`1 498 645` files, `480.9 GiB` by `st_size`. Phase-340 W2 measured `1 489 146` /
`478.9 GiB` on 2026-08-06, so the tree is the same tree, three days on.

**Two controls that the earlier pass did not run, and both matter.**

1. **Hardlinks.** 77 364 inodes carry more than one link, worth 94.5 GiB of
   double-counted bytes — so a naive `st_size` sum overstates the tree by ~20 %.
   Collapsed by inode the tree is **386.3 GiB**. But of that 94.5 GiB, the
   `deps/`↔`deps/` share is **0.00 GiB**: cargo hardlinks a `deps/` artifact to
   its uplifted copy in `<profile>/`, never one `deps/` entry to another.
   **The duplication this phase is about is real, distinct blocks on disk.**
2. **Age.** All 256.7 GiB of it was written within **7 days**; nothing is older.
   This is a live tree, not museum debris, so CLAUDE.md's "re-measure on cleanly
   rebuilt fixtures" caveat is satisfied rather than merely acknowledged.

## The headline reproduces

`deps/` only, deduplicated by artifact NAME — which is cargo's own
interchangeability judgement, since `-C metadata` is *in* the filename:

| | GiB | files / names |
| --- | --- | --- |
| materialised | 265.0 | 287 071 files |
| distinct | 23.7 | 17 195 names |
| **duplicate** | **241.3** | **91.1 %** |

Phase-340 W2 recorded 263.7 / 23.5 / 240.2 / 91.1 %. Reproduced.

**But 7.93 GiB of that is not evidence of anything**, because 448 of those names
carry no metadata hash at all — `libnros_c.a` (×174, **30 distinct sizes**),
`libnros_c.so` (×116, 21 sizes), `libnros_c.rlib` (×174, 97 sizes). Cargo has
made no claim that those are interchangeable, and their size spread says they
are not. Restricting to hash-suffixed compilation outputs (`rlib` / `rmeta` /
`so` / `dylib` / `a`) gives the defensible number:

**256.7 GiB materialised → 20.39 GiB distinct → 236.3 GiB duplicate (92.1 %).**

*(The `.d` dep-files are excluded too: 99 204 of them, 0.4 GiB, and 4 605 of
their hash-suffixed names disagree on size because they embed absolute paths.
They are noise at this scale but they were inflating the "names that disagree"
count into something alarming.)*

## Are the duplicates genuinely shareable, or do they only look alike?

This is the question the task hinged on, and the answer is **not** the obvious
one.

**1 154 of 10 474 hash-suffixed names have copies that differ in bytes** despite
identical `-C metadata`. Diffing two copies of `libtoml_edit-234bebb8e17a39fb.rlib`
(same size, 95 differing bytes in 7.35 MB, in 5 clusters): every cluster sits
immediately after a dependency's name in the rmeta dependency table —
`…\x11-8d48b152677233a5…serde…<16 bytes that differ>`. Those are the recorded
**stable crate hashes of the dependencies**. The extra-filename hashes match; the
content hashes underneath do not. Nondeterminism in one crate propagates a
different recorded hash into every rmeta above it.

The natural inference is that a name-keyed dedup would be unsafe — cross a tree's
`toml_edit` with another tree's `serde_core` and rustc should reject the pair.
**Measured, and it does not.** Staging a complete `deps/` directory from tree A,
swapping in tree B's `libserde_core-a15b3241c7958709.rlib` (different sha256),
and compiling a consumer that forces the metadata to load:

| stage | rustc |
| --- | --- |
| consistent (all from A) | exit 0 |
| **crossed (serde_core from B)** | **exit 0** |
| control: serde_core deleted | exit 1, `E0463: can't find crate for serde_core which toml_edit depends on` |

The control is the point — it proves the crossed run actually loaded the crossed
crate rather than never touching it. So **cargo's `-C metadata` is a sound
interchangeability key here even where the bytes differ**, and the 236.3 GiB is
genuinely redundant rather than 236.3 GiB of things that merely share a name.

The 7.93 GiB of unhashed names is the exact complement: those really do only look
alike, and they are the population where a shared directory is *destructive*
rather than merely wasteful. See "What a shared dir must not do" below.

## Where the mass actually is — the decomposition Wave 2 skipped

Wave 2 read the top of the duplicate list (`libwinnow` ×512, `libcc` ×504,
`libsyn` ×391) and concluded "the host build-dep graph, duplicated across
leaves". The crate names are right. The **location** is not, and the location is
what selects the mechanism.

Classifying every hash-suffixed `deps/` artifact by which *kind* of target
directory it sits in:

| origin | materialised GiB | distinct GiB | dup % | who creates it |
| --- | ---: | ---: | ---: | --- |
| leaf cargo target dir | 121.9 | 14.43 | 88.2 % | one per example leaf |
| **nested: sizes probe** | **63.1** | **0.78** | **98.8 %** | `nros-sizes-build`, per build-script instance |
| cmake / corrosion per-target dir | 58.0 | 5.06 | 91.3 % | one per cmake target |
| **nested: cmake metadata probe** | **13.7** | **1.35** | **90.2 %** | `metadata_probe_cmake`, a nested cmake project |
| **total** | **256.7** | | | |

**76.8 GiB — 32 % of the duplicate mass — is inside target dirs that a build
script created underneath another target dir.** Its dedup ratio is the highest
of any population by a wide margin: 63.1 GiB of sizes-probe artifacts reduce to
**0.78 GiB**, an 81:1 ratio. That block is not "a leaf's host graph"; it is one
probe's host graph, materialised 425 times.

Counted directly: **425 private `sizes-probe-target-*` directories, 80.8 GiB of
total content, mean 195 MiB each.**

| consumer / example family | dirs | GiB | newest |
| --- | ---: | ---: | ---: |
| `nros-c` / `native/c` | 97 | 15.05 | today |
| `nros-c` / `native/cpp` | 94 | 14.57 | 2.2 d |
| `nros-cpp` / `native/cpp` | 54 | 8.37 | 2.2 d |
| `nros-cpp` / `native/c` | 52 | 8.06 | 2.2 d |
| `nros-c` / `threadx-linux/c` | 24 | 3.75 | today |
| … | | | |

## The finding this phase exists for: the shared probe dir already exists, and leaks

`nros-sizes-build` resolves its probe target dir like this
(`packages/tooling/nros-sizes-build/src/lib.rs:184`):

```rust
let probe_target_dir = if let Ok(dir) = env::var("NROS_SIZES_PROBE_TARGET_DIR") {
    // keyed by (rustc slug, target, features) — everything that changes the ANSWER
    PathBuf::from(dir).join(&rustc_slug).join(probe_key(&target, &forwarded))
} else {
    PathBuf::from(env::var("OUT_DIR")?).join(format!("sizes-probe-target-{rustc_slug}"))
};
```

The shared arm is not a proposal. It landed as `82b31d08e` ("perf(sizes-probe):
one shared probe cache instead of one per build dir", **2026-08-04**), it is
keyed correctly — by everything that changes the probe's answer, which is the
lesson phase-336 W7 paid for — and `build/sizes-probe` is **8.1 GiB and actively
written today**.

And 425 private dirs were written on 2026-08-06, -07 and **-08**, i.e. *after*
the shared arm landed and *alongside* it. Both arms are live in one tree in one
week.

The reason is the wiring, and it is CLAUDE.md's "fix the CLASS, not the site"
pattern with the roles reversed — the fix was correct and landed at one site:

```sh
# scripts/build/cargo.sh:168
if [ -z "${NROS_SIZES_PROBE_TARGET_DIR:-}" ]; then
    _nros_probe_dir="$(nros_sizes_probe_dir)"
    [ -n "$_nros_probe_dir" ] && export NROS_SIZES_PROBE_TARGET_DIR="$_nros_probe_dir"
fi
```

**The shared dir is opt-in via an environment variable exported by one shell
script.** Every build that transits `scripts/build/cargo.sh` — `fixtures-build.sh`,
`workspace-fixtures-build.sh`, `zephyr-fixture-leaves.sh` — gets it. Everything
that does not, silently gets a private 195 MiB probe dir and no diagnostic:

* `nros`'s own nested cmake metadata probe
  (`packages/cli/nros-cli-core/src/orchestration/metadata_probe_cmake.rs`)
  spawns `cmake` inheriting whatever env `nros` was launched with — 13.7 GiB in
  its own right;
* a developer running `cmake --build` or `just <recipe>` in a shell that has not
  sourced the script;
* any consumer of `nano_ros` outside this repo.

**The default is the expensive branch.** That is the whole defect: a
195 MiB-per-instance cost is paid by anyone who does not know to opt out of it,
and nothing reports that it happened.

## Mechanisms, evaluated

Wave 2 listed four candidates. Two of them do not exist.

### Rejected — "CARGO_TARGET_DIR scoped to the host half" (cannot be written)

```console
$ cargo build --help | grep target-dir
      --target-dir <DIRECTORY>  Directory for all generated artifacts
```

One directory, for all artifacts. Cargo has no host-scoped target dir: not a
flag, not a `[build]` key, and the `-Z` options that mention hosts govern
*flags*, not paths — `-Z host-config` enables a `[host]` section for rustflags,
`-Z target-applies-to-host` changes which flags apply, `-Z dual-proc-macros`
changes what gets built. None of them relocates the host half.

The host/product split *is* visible on disk — with `--target <triple>`, cargo
writes host units to `<dir>/<profile>/deps` and product units to
`<dir>/<triple>/<profile>/deps`, and that is how this phase classified them:

| class | materialised GiB | distinct GiB | dup % |
| --- | ---: | ---: | ---: |
| HOST | 173.8 | 7.89 | **95.5 %** |
| product (cross) | 61.9 | 11.13 | 82.0 % |
| unsplit (no `--target`) | 21.0 | 1.55 | 92.6 % |

— but it is an *output layout*, not an input knob. You cannot point the left
column somewhere else. **The host half can only be shared by sharing the whole
target dir**, which is arm B, which phase-340 W2 has already decided.

### Rejected — "corrosion's own host half" (same reason, one level up)

Corrosion invokes `cargo` with `--target`; it has no more access to a host-only
target dir than anyone else. Nothing to build.

### Rejected for this axis — sccache

Phase-340 F1 already settled it and this measurement does not disturb it:
sccache dedups **compiles**, and every consumer still writes its own copy of the
result. A 236 GiB *byte* problem is invisible to it by construction. The task
brief's caution is also confirmed as still true — sccache did not bridge the
`--target` spelling split (0 hits / 62 misses, phase-340 W3) — but that is a
reason it does not help the CPU axis either, not the reason it fails here.

### Rejected — offline dedup (hardlink / reflink janitor)

Worth pricing, because at 91 % duplication somebody will propose it, and the
rustc experiment above says the artifacts really are interchangeable. Two
measured facts kill it:

* **No reflink.** The volume is ext4 (`stat -f` → `ext2/ext3`). `cp --reflink`
  is unavailable, so copy-on-write dedup is not on the table on this host.
* **Hardlinks are safe but self-erasing.** Tested: hardlink two rlibs, rebuild
  one with rustc, and the link count drops 2 → 1 while the peer keeps its old
  content — rustc unlinks and recreates rather than truncating in place. So a
  hardlink pass corrupts nothing, and *undoes itself* one artifact at a time on
  every subsequent build.

That makes it a janitor for reclaiming a tree you already have — genuinely
useful when the volume is at 98 %, which it is — and not a fix. Recorded as an
operational tool, not a mechanism.

### Accepted (already decided) — one shared `--target-dir` per group

Phase-340 W2 measured this at the real group size and it is not this phase's to
re-decide: 9.70 GiB → 455 MiB over 37 leaves, `deps/` dedup 27.9:1 → 1.0:1, never
slower than the status quo. It addresses the *leaf* and *corrosion* populations —
179.9 GiB materialised against 19.5 GiB distinct.

**What this phase adds to it is one settled blocker.** Phase-340 W2.a records A2
as open: "the Rust resolver cannot express a variant group, and `linux` produces
six", with the coarse platform-grained key marked "not decided here, because it
needs the lane". The measurement decides it without the lane:

* Distinct identities **coexist by construction** in one directory, because
  `-C metadata` is in the filename. This tree demonstrates it at scale — 17 195
  distinct names currently share 366 directories, and `nros_core` alone carries
  78 identities that have never collided with each other.
* Where two builds *do* land on one name with different bytes, the crossed-rlib
  experiment above shows a consumer accepts the survivor.
* Feature unification — the one real reason the key had to be variant-grained —
  applies only *within* one cargo invocation, and arm B is N invocations.

So the platform-grained key is sound, and `fixture_shared_target_dir`'s existing
`build_dir("fixtures-cargo", &[platform])` is already the right answer. The
remaining verification is the namespace one, which phase-340 already ran
(0 collisions, 7 platforms, 122 rust rows) — not a semantic one.

### What a shared dir must not do — the unhashed 7.93 GiB

Phase-340 gated artifact-name collisions for **binary** names (`talker` /
`listener`, fixed in `3ebc32110`, `KNOWN_COLLISIONS` now empty). The measurement
says the class is wider than the gate:

| name | copies | distinct sizes |
| --- | ---: | ---: |
| `libnros_c.a` | 174 | **30** |
| `libnros_c.so` | 116 | 21 |
| `libnros_c.rlib` | 174 | 97 |

These sit **in `deps/`** and carry no metadata hash, alongside properly hashed
`libnros_c-<hash>.a` siblings. In a shared directory 174 non-interchangeable
artifacts would contend for one path, last writer wins — the same
silently-wrong-binary failure the binary-name gate was created to prevent, one
directory deeper. **`check-fixture-groups` should extend its collision scan from
binary names to every unhashed artifact name a group's members can emit**, and
that is cheap to do before any path moves.

## Decision

**Do not build a "shared host target-dir" subsystem. It cannot be built, and the
part of the prize that is separable from phase-340 is a wiring fix.**

The 236.3 GiB splits into three jobs with three different owners:

| population | GiB dup | owner | status |
| --- | ---: | --- | --- |
| leaf + corrosion target dirs | ~160 | **phase-340 W2 / Wave 1** | mechanism decided, migration blocked on paths |
| nested probe dirs | **76.8** | **this phase, W1–W3** | **W1–W3 LANDED** — 63.1 GiB recovered directly, cmake-probe row measured to zero |
| unhashed-name collision risk | (hazard) | **phase-340 W2 gate** | gate narrower than the class |

## Work items

### W1 — make the shared probe dir the DEFAULT, not an opt-in — **LANDED** (`7db7e72b5`)

- [x] Invert the resolution in `nros-sizes-build`: derive the shared root when
      one is discoverable, fall back to `$OUT_DIR` only when it is not.
      `NROS_SIZES_PROBE_TARGET_DIR` stays as the explicit override and keeps
      winning — issue 0400's box-private tree depends on that.
- [x] The discoverable root must not become a **second spelling** of
      `build-root.sh`'s `nros_build_dir`. That is the failure mode phase-334
      W2.b spent a pass eliminating and the one `nros_fixture_group` /
      `NROS_FIXTURE_SHARED_PLATFORMS` divergence (phase-340 W2.a) cost most.
      Prefer: keep the shell as the single deriver and have it export the
      variable from a place every entry point transits, rather than teaching the
      Rust side to re-derive a repo root.
- [x] Out-of-tree consumers must keep working. A `nano_ros` installed outside
      this checkout has no repo root and MUST land on `$OUT_DIR` — verify, do
      not assume.

**Acceptance:** after a `lane=native` rebuild of a clean tree, zero
`sizes-probe-target-*` directories exist under `examples/`, `build/sizes-probe`
holds the whole probe population, and `just verify-size-probe` is green.

**Tripwire, both directions** (phase-340's rule, and the one this phase's
predecessor paid for): a test that asserts the private branch is taken when no
root is discoverable **and** the shared branch when one is — each arm must fail
when the other's condition is forced.

### W2 — close the same leak in the cmake metadata probe — **LANDED** (2026-08-10)

- [x] `metadata_probe_cmake` spawns a nested `cmake` that runs a full cargo
      build (13.7 GiB, 90.2 % duplicate). It inherits `nros`'s environment, so
      it is fixed for free **if and only if** W1 makes the default shared rather
      than relying on an exported variable. Confirmed by measurement.

**The measurement, and the two false readings it took to get one.**

The obvious check — "no private probe dirs newer than the fix" — returned zero
in both the main tree and the box mirror. It meant nothing: `build/sizes-probe`
had no entries newer than the fix either, so **no probe had run in either tree
since W1 landed**. Today's fixture builds were incremental and the build scripts
were cached. A passive count over a population nothing has written to is the
same green-on-nothing this repo keeps catching one layer up.

Second attempt, `nros sync` on `examples/workspaces/cpp` with
`NROS_SIZES_PROBE_TARGET_DIR` explicitly unset: `sync: source metadata — 0
rebuilt, 6 already current`. Still nothing ran; the sidecars were current.

The measurement only became real after invalidating the six sidecars and the
stale probe tree, which forced `6 rebuilt, 0 already current`:

| | before W1 (same tree) | after, cold probe |
| --- | ---: | ---: |
| private `sizes-probe-target-*` under the probe tree | **4** (2026-08-02/03) | **0** |
| shared `build/sizes-probe` written | — | yes, `<rustc-slug>/695abfac671af99c/` |
| probe tree on disk | 5.7 GiB accumulated | 843 MiB rebuilt cold |

The four private dirs are the 13.7 GiB row's live instance in this checkout, and
they are gone. The env var was unset for the run, so this is the branch that
leaked, taking the shared path.

**Note the sizes honestly:** 5.7 GiB was accumulated across runs since
2026-08-02, and 843 MiB is one cold rebuild. They are not the same measurement
and the ratio between them is not a saving. The saving this item claims is the
row going to zero, which is the line above it.

### W3 — extend the collision gate from binary names to all unhashed artifacts — **LANDED** (2026-08-10)

- [x] `check-fixture-groups` scans binary names. Widen to every artifact name a
      group member can emit without a metadata hash — staticlibs and cdylibs
      included. **Done by phase-340 B1** (`d8c46b446`), which cites this item's
      own example: `libnros_c.a` at 438 copies across ~30 distinct sizes.
- [x] Tripwire: a deliberately re-collided staticlib name must fail the gate,
      and a fixed tree must report an empty record.

**The widening was already done; the tripwire was not.** B1 ran both directions
by hand and recorded them in its commit message, which is not something a later
reader can re-run — and this gate has now been widened twice (W1's row-keyed
owners, B1's LIB artifacts), each time because it had been reporting "no
collisions" over a namespace it was not looking at. A gate with that history
needs a standing check, so `packages/testing/nros-tests/tests/
fixture_group_collision_gate.sh` now runs inside `check-fixture-groups`:

| arm | asserts |
| --- | --- |
| T3 | the real tree passes with an empty record — run FIRST, so a red baseline cannot make T1/T2 pass for the wrong reason |
| T1 | two rows claiming one BINARY name are reported by name |
| T2 | two rows claiming one STATICLIB name are reported by name (the B1 arm) |
| T3' | the perturbed manifests were restored |

**Its first draft was itself the defect it exists to catch.** T1/T2 asserted only
`rc != 0`, and T2 passed with B1 reverted — because appending a second `[lib]` to
a leaf that already declares one is invalid TOML, so the gate died in
`tomllib.load` with rc=1 and the assertion could not tell a crash from a
detection. Both halves were fixed: the perturbation EDITS the existing table, and
every expectation greps the gate's message for the artifact name it should have
found. **A non-zero exit is not evidence of detection** — that is the same shape
as issue 0445 (a STALE verdict absorbing the run) and as the passing-message tell
in this gate's own docstring, where "61 row(s)" for 85 rows was the clue.

Each arm was then confirmed to FAIL when its half of the gate is disabled —
`artifacts()`'s lib branch off makes only T2 fail; its bin branch off makes only
T1 fail. A tripwire nobody has seen fail is a tripwire nobody should trust.

### W4 — an operational reclaim, kept honest about what it is

- [ ] A read-only reporter for the origin×age decomposition in this document, so
      the number can be re-checked rather than re-derived by the next reader.
      This phase's measurement scripts are throwaway (`tmp/w2b/`) by design.
- [ ] If a reclaim pass is wanted at 98 % full: hardlink-by-name is *safe*
      (measured) and *self-erasing* (measured). Ship it as a janitor with that
      stated, or not at all. It is not a mechanism and must not be recorded as
      one.

### W5 — do not re-open the host-graph question without new information

- [ ] If cargo ever grows a host-scoped target dir, re-price: the HOST class is
      173.8 GiB at 95.5 % duplication and would become separable. Until then the
      only lever on it is the whole-dir share, which phase-340 owns.

## What this phase did not verify

Stated plainly, because the phase-340 process rule is that a worktree agent
verifies at gate level only:

* **No build was run.** This is a read-only measurement of an existing tree plus
  two contained rustc experiments. The 425 private probe dirs are evidence that
  the leak *happened*; the exact set of entry points that miss the export was
  derived by reading the call sites, not by instrumenting a build.
* **W1's acceptance is unproven.** "Zero private probe dirs after a clean
  `lane=native` rebuild" is the criterion and it needs a provisioned tree and
  ~20 GB of headroom. The volume was at 98 % (23 GB free) throughout.
* **The crossed-rlib result is one crate pair**, not a survey. It is enough to
  refute "same-name artifacts are not interchangeable"; it is not enough to
  claim every pair in the tree is.
* **The 8.1 GiB `build/sizes-probe` figure is a floor, not the steady state** —
  it accumulated under partial wiring. What it costs when it serves the whole
  tree has not been measured.
