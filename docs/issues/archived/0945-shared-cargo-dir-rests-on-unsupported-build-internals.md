---
id: 945
title: "The shared-cargo-dir campaign rests on unsupported build-system
  internals — a Corrosion path formula, an unstable cargo flag, cargo's private
  `.fingerprint` format, a side channel inside cargo's target dir, an
  undocumented depfile location, and (found here) cargo's target-dir output
  layout"
status: resolved
type: tech-debt
area: build
related: [issue-0805, issue-0616, issue-0499, issue-0834, issue-0835, issue-0112,
  phase-424]
---

## Symptom

Nothing is failing. This is a register of what the build-performance campaign
(phase-400, and issue 0805 before it) DEPENDS ON that no one has promised to keep
working. Each item breaks on somebody else's release, not on a change of ours,
and most break QUIETLY — the build keeps going and stops sharing, or keeps
sharing and reads the wrong file.

Filed because the exposure was reviewed once, deliberately, and that review
should not have to be reconstructed from commit messages.

## RESOLVED 2026-09-05 (phase-424)

The phase's acceptance was: *"0945's five assumptions are either supported by
something we can point at, or written down as accepted risk with what would break
if each fails."* Every item below now carries a VERDICT, an EVIDENCE line, and a
DETECTION line. Three things changed on the way:

1. **One claimed mitigation was false and is now real.** Item 3 said both
   fingerprint tools have a `--self-test` that would surface a schema change.
   `nros-leaf-graph`'s never touched the parser at all, and
   `nros-shared-dir-churn`'s wrote the same key names it read, so both stayed
   green through a rename while answering wrongly. Both now REFUSE, both
   self-tests mutate the key, and both run that self-test on their NORMAL path
   so the control is exercised at the moment a number is about to be quoted.
2. **A SIXTH assumption was found**, in the layer this issue is about but not on
   the list: cargo's `[<triple>/]<profile>/<bin>` output layout inside a
   `--target-dir`, which the two DIFFERENTIAL staleness probes glob for. Item 6.
3. **The classification is now per PROBE FAMILY**, because "which of these does
   the freshness machinery actually rest on" turned out to have a different
   answer than the register implied — see the map below.

Nothing is retired: items 1, 2, 4, 5 and 6 remain live exposure, by design, with
their detection stories written down. What remains as WORK is item 4's consumer
migration, which is a wave and is recorded in phase-424.

## Which probe family depends on which assumption

Measured on `origin/main`, 2026-09-05, by running the manifest:

| family | rows | decides by |
| --- | ---: | --- |
| cmake cells (`--lang c` 61, `--lang cpp` 59) | **120** | DIFFERENTIAL — md5 of the top-level executables in the cell's own build dir, before and after an incremental `cmake --build` |
| rust fixtures (`--builder cargo`) | **117** | DIFFERENTIAL — md5 of the ROW's own binaries under the phase-340 group `--target-dir` |
| workspace fixtures | **94** | `.inputsig` — a signature over sources + codegen fingerprint + measured dep closure |
| compile-checks | **40** | `.inputsig` — same shape |
| | **371** | 237 differential, 134 signature |

The two kinds fail in OPPOSITE directions, and that is what decides which
assumption hurts which:

* A **differential** family answers "did these bytes move?". It needs to FIND the
  artifact. If cargo's directory identity or output layout moves under it, it
  finds nothing, falls back, and the fallback is the pre-0835 signal that is
  permanently "stale" for these rows — a false STALE dressed as a self-healing
  WARNING. It is also structurally blind to an input that is stale in a STABLE
  way: a wrong generated header produces the same wrong bytes twice, so the
  verdict is FRESH.
* A **signature** family answers "does the recorded stamp still match a recomputed
  one?". It never hashes an ARTIFACT, so a directory-identity or layout change
  cannot corrupt its verdict — it can only make the stamp unfindable, and that
  reads as MISSING and fails loud. (`workspace-fixture-stale.sh` puts the stamp
  under the row's own `target_dir`/`build_subdir`; `compile-check-stale.sh` puts
  it under `nros_build_dir`, our root. Neither is Corrosion's.) The one place a
  signature family reaches into a build tree is
  `nros_dep_closure_manifest` -> `dep-closure.py`, and what it reads there is
  Make-syntax `*.d` dep-info plus cmake/ninja re-configure edges — item 5's
  class, and it degrades to an empty closure rather than a wrong one.

Per assumption:

| # | assumption | cmake cells (120, diff) | rust fixtures (117, diff) | workspace (94, sig) | compile-check (40, sig) |
| --- | --- | --- | --- | --- | --- |
| 1 | Corrosion symlink | **cost only** | no | no | no |
| 2 | `--artifact-dir` | 13 nuttx rows, loud | no | no | no |
| 3 | `.fingerprint` parse | no | no | no | no |
| 4 | header side channel | **yes, and invisible** | leaves that link nros-c/cpp | partly | partly |
| 5 | depfile location | **yes** (short watch list) | yes | no | no |
| 6 | target-dir layout | fallback exists, unused | **yes, silent** | no | no |

Item 3 touches no family: both tools are hand-invoked diagnostics, nothing in
`check-fast` or the probe path parses `.fingerprint`, and nothing should.
(`scripts/build/dep-closure.py`, which the two signature families DO use, reads
Make-syntax `*.d` dep-info and cmake/ninja re-configure edges — that is item 5's
class, not item 3's.)

## The six

### 1. The Corrosion symlink redirects a path Corrosion computes privately

`nros_share_corrosion_cargo_dir()` (cmake/NanoRosSharedCargoDir.cmake) works by
symlinking over `${CMAKE_BINARY_DIR}/cargo`, because Corrosion derives its
`--target-dir` as `${CMAKE_BINARY_DIR}/cargo/<workspace-folder>_<hash-of-manifest-path>`
and, in its own words there, *"Corrosion 0.6.1 exposes no knob for the directory
(it is a plain local)"*.

**VERDICT: accepted risk, witnessed.** Read against the pinned v0.6.1 AND against
upstream `master`: the same
`cmake_path(APPEND CMAKE_BINARY_DIR ${build_dir} cargo "<folder>_<hash5>")`,
still a plain local, no cache variable, no `corrosion_import_crate()` argument,
no target property. The symlink is not a workaround for a version we are behind
on — it is the only override point that exists.

**WHAT BREAKS: not correctness — AFFORDABILITY, and that is the correction this
review makes.** The intuition is that a dead redirect corrupts the cmake cells'
freshness verdict. It cannot: `cmake-fixture-stale.sh::_arts()` hashes the
executables in `$dir/$sub`, the CELL's own build dir, and never reads the cargo
tree, so where Corrosion writes cannot change the bytes being compared. What a
dead redirect does instead is make each of the 120 cells rebuild nros-c /
nros-cpp from scratch — inside a probe that builds all 120. The gate stops being
runnable rather than starting to lie, which is a different failure needing a
different alarm.

**DETECTION: `nros_assert_shared_cargo_dir_used()`** (cmake/NanoRosCorrosion.cmake)
driving `scripts/check-shared-cargo-dir-used.sh` as a build-time check on every
Corrosion leaf that shares. It asserts the RESULT, never the formula — a second
copy of Corrosion's path rule would drift from it silently, which is the defect
rather than the fix:

  1. `${CMAKE_BINARY_DIR}/cargo` is still a symlink at the directory THIS
     configure chose;
  2. an artifact with the built target's name exists under that directory and
     its size matches the copy Corrosion produced.

Everything it consumes is documented or ours — `$<TARGET_FILE:...>` and stat(2).
It parses nothing. Measured on `examples/native/c/talker`: the healthy build
prints `shared-cargo-dir OK (nros_c-static)`, and all four dead-redirect states
fail with the artifact paths named. `ninja -t query` puts `libnros_c.a` above the
`||` line, so the witness re-runs on a real archive change and not on an
order-only edge (issue 0268's rule); a no-op rebuild does not re-run it.

**WHAT IT STILL CANNOT CATCH, precisely:** a long-lived build dir that shared
successfully BEFORE a Corrosion upgrade keeps a same-named artifact in place, so
with no code change the sizes can still match and this passes. It cannot pass for
long — any edit moves the size — and it cannot pass at all for a new key, which
is what a reconfigure after an upgrade produces. Byte-comparing would close the
hole and cost a full read of a multi-hundred-MB archive on every leaf build; not
worth it for a performance-regression detector.

Escape hatch: `NROS_ALLOW_UNSHARED_CARGO_DIR=1` downgrades the failure to a
warning, for someone mid-upgrade who wants the build to finish. The witness's own
negative control (five fixtures, including the state only the symlink arm can
catch) runs on the NORMAL path, not behind the flag — a witness that had quietly
stopped witnessing would be this very defect one level up. `just check
shared-cargo-dir-witness` (fast line) runs it standalone.

BLAST RADIUS: six platforms (freertos, native, nuttx, qemu-baremetal,
threadx-linux, threadx-riscv64). This is the campaign's single largest dependency
on someone else's internals, and it PREDATES phase-400.

### 2. `--artifact-dir` is an explicitly unstable cargo flag

The NuttX FFI driver (packages/api/nros-c/cmake/nros-nuttx.cmake:334) passes
`-Z unstable-options --artifact-dir` to evict per-leaf artifacts from a shared
target dir. It survives only because that crate is pinned to a nightly toolchain
(`packages/boards/nros-board-nuttx-qemu/nros-nuttx*-ffi/rust-toolchain.toml`),
and the flag has already been renamed once (`--out-dir`).

**VERDICT: accepted risk, loud in every direction.** A rename, a removal or a
stabilisation all make cargo exit non-zero, and every consumer of that artifact
is downstream of the same cargo command. There is no arm where the flag silently
stops copying: the CMake byproduct `${_artifact_dir}/nros-nuttx-ffi` is what the
kernel link consumes, so an uncopied artifact is a missing file, not a stale one.

**WHAT BREAKS:** the thirteen nuttx cells in the cmake differential family
(measured: 6 `examples/qemu-arm-nuttx/c`, 1 `examples/qemu-riscv-nuttx/c`, 6
`examples/qemu-arm-nuttx/cpp`) plus the nuttx compile-check and workspace rows.

**HOW YOU NOTICE, and the one weak spot:** `cmake-fixture-stale.sh` reports a
cell whose build FAILS by printing its build dir — so it surfaces as STALE, and
the cell then fails the fixture gate. But that probe captures cmake's output into
`$out` and discards it on the failure branch, and `check-fixtures-stale.sh` sends
probe stderr to a file it only prints on a probe CRASH. So the verdict is loud and
the CAUSE is not: the reader sees "stale", not "cargo rejected `--artifact-dir`".
That is an attribution gap, not a detection gap, and it is shared with every other
way a cell can fail to build.

NOTE: this is also why the Zephyr C/C++ lane cannot use the same eviction —
native_sim builds on STABLE, so the flag is unavailable there. That constraint is
what forces features into the shared-dir key and caps the Zephyr collapse at
70 -> 28 build dirs instead of 70 -> 14 (phase-400 W5).

### 3. `just leaf-graph` and `just shared-dir-churn` parse cargo's private `.fingerprint` format

Both tools read `<target-dir>/**/.fingerprint/<unit>/*.json` — an on-disk format
with no stability guarantee and no documentation. `deps`, `features`,
`compile_kind` and the `local` array of `RerunIfEnvChanged` / `RerunIfChanged`
entries are all internal.

**VERDICT: accepted risk — and the mitigation this issue claimed did not exist.
It does now.**

The claim was: *"both have `--self-test`, so a schema change that breaks parsing
surfaces as a failing self-test rather than a plausible wrong number."* Checked,
2026-09-05, and FALSE on both counts:

* `nros-leaf-graph --self-test` fed `requirer_map` a hand-written `units` list.
  It never opened a `.json` file, so it could not see a schema change at all.
  Every key is read with `.get(..., default)`, so a renamed `deps` does not
  raise — every crate reads as a build root and `--exclusive-to X` answers
  **"nothing leaves"**. That is plausible, optimistic, and wrong in exactly the
  direction of the three phase-400 estimates this tool exists to correct.
  A renamed `compile_kind` labels every unit "host" and conflates the two graphs
  — the same class as the path-based guess the tool already replaced once.
* `nros-shared-dir-churn --self-test` DID exercise the parser, but by WRITING the
  same key names it reads. A rename moves both ends together: every unit parses
  to an empty env dict and an empty path set, every unit therefore agrees with
  every other, and the tool prints *"These trees are safe to collapse onto one
  --target-dir"* — the most dangerous sentence it can say, on evidence it did not
  read. Its existing vacuity guard does not catch this: it fires on "no unit is
  common to two trees", and here every unit is common.

**DETECTION, landed here.** Both tools now REFUSE (`INCONCLUSIVE`, rc 2) when
records were read and not one carried the key the answer depends on:

* `nros-leaf-graph`: `schema_complaint()` on `deps` and on `compile_kind`.
  Measured: every record cargo writes today carries both — `lib-*`,
  `build-script-build*` and `run-build-script-*` in a populated tree all have the
  identical key set — so "records read, none carried it" is a rename, not a legal
  shape.
* `nros-shared-dir-churn`: refuses when no record in any tree yielded a
  `RerunIfEnvChanged` or `RerunIfChanged` entry. `Precalculated` entries are
  normal and carry neither (measured: 14 of 125 records in the freertos tree, 16
  of 72 in qemu-arm-baremetal), so the predicate is "not ONE record in ANY tree",
  never "some record yielded none".

Both self-tests now MUTATE the key and assert the refusal, and assert that the
refusal NAMES the key. Mutation-checked in both directions: disable
`schema_complaint` and `nros-leaf-graph --self-test` reports 3 FAILs (rc 1) while
printing the wrong graph; disable the churn guard and its self-test dies on
`a renamed .fingerprint schema still certified sharing`.

**And the self-tests are RUN — from `main`, not from a flag.** The first cut of
this added a dedicated `check` gate for the pair; `check-gate-selftests` rejected
it, and it was right to: *"a negative control nobody runs decays into a comment.
Call it from main."* For a hand-invoked DIAGNOSTIC that is the stronger placement
anyway — the moment the guard has to be wired is the moment someone is about to
quote a number out of the tool, not a CI lane nobody reads. Both `main()`s now run
`self_test(quiet=True)` before doing any work and refuse to report if it fails
(`run_selftest=False` on the self-test's own re-entrant calls). Measured overhead:
~10 ms for `leaf-graph`, ~50 ms for `shared-dir-churn`, on tools that then walk a
target dir.

**WHY THEY ARE STILL WORTH HAVING:** the question they answer ("what did THIS
build compile, and who required it?") has no supported interface. `cargo tree`
re-resolves the workspace and answers a different question, which is exactly the
mistake these tools exist to prevent. They remain DIAGNOSTIC tools: nothing in
`check-fast` depends on their OUTPUT, and nothing should. The gate checks that
they still parse, not that a tree is healthy.

### 4. The generated headers are a side channel inside cargo's target dir

Build scripts write `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/*.h` — a path
INSIDE cargo's tree that cargo does not manage. It works because nothing cleans
it, not because it is supported.

**VERDICT: accepted risk, and the highest-incidence one in the register — but not
for the reason first written.** The stated risk was a future cargo target-dir GC
treating those files as unowned. That has never fired. What HAS fired, six times,
is the consequence of the side channel having no owner, so every consumer grows
its own rule for which copy is authoritative:

| issue | what went wrong |
| --- | --- |
| 0360 | written to a FLAT path, so two feature sets overwrite one header |
| 0834 | the mirror reaches a state no re-run repairs (stamp present, header absent) |
| 0978 | the mirror prefers a leaf's OWN header whenever present |
| 0985 | a configure-time heal wrote a museum sizes header over a correct one |
| 0987 | the mirror's `cargo/*/` glob took the FIRST candidate, not the newest |
| 1031 | no RMW selected, both build scripts decline to write, three snippets fail |

**WHAT BREAKS, and why the differential probes cannot see it.** The cmake cells
compile against `-Itarget/nros-{c,cpp}-generated`. A header that is stale, absent
or from the wrong variant produces the same wrong bytes on the "before" build and
on the "after" build, so `md5(before) == md5(after)` and the cell reads FRESH. A
differential probe is structurally blind to an input that is stale in a STABLE
way. This is the one place in the register where an assumption failing makes a
freshness verdict WRONG rather than merely expensive.

**DETECTION:** `check-orphan-generated-stamp` (fast line) catches 0834's state —
a `.stamp` with no `.h` beside it. `nros_config_generated.h`'s variant slug and
`integrations/px4/NanoRosArchivePairing.cmake` (phase-424) catch a header paired
with the wrong archive, at configure time. Nothing catches "header present,
correct variant, older than the sources", which is why the migration below is the
real remedy rather than another guard.

**BEING ADDRESSED:** phase-400 W5.c emits the same headers to `$OUT_DIR` — cargo's
sanctioned location, per-unit and hashed BY CARGO, so two feature sets cannot
collide without us keying anything. Measured: default features and
`rmw-cffi,platform-posix,std,ros-humble` land in different `OUT_DIR`s. The path is
discoverable on the stable JSON stream (`{"reason":"build-script-executed", …
"out_dir": …}`), which cargo emits even on a FULLY CACHED run (measured: 13 events
with nothing to rebuild) — the property the side channel lacks.

**REMAINING WORK, counted not estimated** (`git grep -n
'nros-c-generated\|nros-cpp-generated' -- ':!docs'`, 2026-09-05): **128 hits
across 26 files**, including `cmake/NanoRosNodeRegister.cmake`,
`cmake/NanoRosVerbs.cmake`, `integrations/nuttx/Make.defs`,
`integrations/px4/NanoRosPx4Module.cmake`, `zephyr/CMakeLists.txt`, and 49 hits in
`just/check.just` alone. Deleting `write_header_to_target_dir` needs all of them
moved first, which is a wave, not an afternoon. Tracked in phase-424.

### 5. The probe depfile's location is an undocumented layout convention

`nros-sizes-build::probe_depfile` knows that cargo writes a rustc depfile beside
the UPLIFTED artifact and never beside the hashed `deps/` copy. Measured in this
repo's probe store: 182 uplifted rlibs with 182 depfiles, 269 `deps/` rlibs with
none.

**VERDICT: accepted risk, loud in the direction that matters, quiet in one that
does not have a cheap guard.**

**WHAT BREAKS:** a layout change removes the watch list, which is issue 0563's
defect — a probe that measures a crate and then does not watch it. The consumer's
build script then stops rebuilding when that crate changes, and both differential
families report FRESH on an artifact built from a stale measurement (the sizes
header reaches the C/C++ cells through `nros-build-helpers`).

**DETECTION:** `probe_depfile` tries both spellings and PANICS if neither exists,
naming the file and the issue. Pinned by
`probe_depfile_found_beside_and_uplifted`. Be exact about what that pins: the
test constructs the directory shape itself, so it fixes the RESOLVER's behaviour
against a shape we assert, not cargo's actual behaviour. If cargo moved the
depfile somewhere neither spelling names, the panic fires and the build stops
loudly — which is the case that matters. If cargo instead kept the file where it
is but wrote a SHORTER list into it, nothing here notices; that is 0563's
original defect and it has no cheap guard, because the correct list is exactly
what we do not independently know.

### 6. Cargo's output layout inside a `--target-dir` (NEW — found reviewing this register)

`scripts/test/rust-fixture-stale.sh::_row_artifacts` locates a row's binaries by
globbing `<target-dir>/*/*/<bin>` and `<target-dir>/*/<bin>` — cargo's
`[<triple>/]<profile>/<bin>` convention. Its own comment says why it globs rather
than deriving ("the triple comes from the leaf's own .cargo/config.toml"), which
is the honest reason and also an admission that the LAYOUT is being assumed.
`cmake-fixture-stale.sh` makes the milder assumption that a cell has exactly one
top-level executable in its build dir.

This belongs in this register: it is the same kind of dependency as item 5 (an
undocumented on-disk layout), it is load-bearing for 237 of the 371 probe rows,
and it was not on the list.

**VERDICT: accepted risk with no detection story. This is the register's one
genuine gap — and it is NOT covered by the gate that landed for #0835 on the same
day.** `check-fixture-staleness-probes`
(`tests/fixture-staleness-probe-tests.sh`, commit `4746f93d5`) runs the REAL probe
scripts against a two-file cmake cell and a one-binary cargo leaf, with a negative
control that re-applies the pre-`2fa1ed09f` decision rules. Every one of its cases
— A (activity vs bytes), B (a re-run unit with identical output), C (cross-family
re-staling), D (the negative control) — has the artifact PRESENT. What is
unguarded is the branch taken when the glob finds NOTHING, which is the one a
layout change produces.

**WHAT BREAKS:** the glob matches nothing, `_row_artifacts` is empty before and
after, and the probe silently falls back to cargo's `"fresh":false` — the
pre-0835 signal, which for these rows is PERMANENTLY true, because rows sharing
one phase-340 group evict each other. Issue 0835 measured that exact state: ~22
rows reporting stale on every run with byte-identical binaries, the gate never
reaching a fixed point, and ~190 fixture-stale test failures on every
`just ci-matrix`. The failure is reported as a self-healing WARNING, which reads
as the gate working. The cmake probe has the same shape: no artifact means it
falls back to the output grep, which is permanently "stale" for the 17 Corrosion
cells 0835 identified.

**HOW CLOSE IS IT TODAY:** measured read-only against the shared checkout,
2026-09-05, by replicating both locators without building — **116 of 117** rust
rows find an artifact, and **120 of 120** cmake cells do. Exactly one row is on
the silent fallback right now: `packages/testing/qemu-smoltcp-bridge`, whose
`Cargo.toml` names no binary the glob can find. Its verdict comes from
`"fresh":false` and nothing says so.

**REMEDY, specified but not landed:** both fallback branches should emit a
`DEGRADED\t<row>\t<reason>` line — the same convention `rust-fixture-stale.sh`
already uses for `FAILED\t` — and `check-fixtures-stale.sh` should bucket and
count them the way it buckets `rust_failed`, as a WARNING beside the stale list.
That widens no watch set, so it cannot make 0835 worse (phase-424's constraint).
Not done here for one reason, stated rather than hidden: verifying it needs a
built fixture tree, this worktree has none, and shipping unverified shell into
the gate that decides whether every test result is trustworthy is the defect this
phase exists to remove. It is a small, well-specified change for whoever next has
fixtures built.

## Not on this list, deliberately

`nros_shared_cargo_dir()` itself (a directory plus a SHA1), the fixture stamp and
its `.started` sidecar (our file, our format), the `nros-sizes-build` nested cargo
using `--message-format=json` (stable, documented), and `rerun-if-env-changed` /
`rerun-if-changed` (documented). These are ours or supported, and carry no version
exposure.

## Order for whoever picks up what remains

1. **#4's consumer migration** — the only item with real work left, and a wave.
   128 hits, 26 files. Tracked in phase-424.
2. **#6's `DEGRADED` marker** — an afternoon, on a machine with fixtures built.
3. **#2** only if the NuttX lane's nightly pin is ever revisited.
4. **#1, #3, #5** are as closed as they can be: #1 is witnessed and Corrosion
   offers no knob to retire it with, #3 now refuses instead of guessing, #5 fails
   loudly in the direction that ships.

## CLOSED 2026-09-05 — accepted risk, each item re-verified against the tree

Closed under phase-424's acceptance: *"0945's five assumptions are either
supported by something we can point at, or written down as accepted risk with
what would break if each fails."* Nothing here is fixed by closing it; what
changes is that the register has been CHECKED rather than remembered, and each
item now carries how its failure would be DETECTED, which is the property
phase-424 is actually about.

**The table below verifies FIVE, and the register above lists six.** Two
sessions worked this issue at once and each found something the other did not:
this verification pass (with its detection column and the 1031 evidence) came
from one, and item 6 — cargo's output layout inside a `--target-dir` — from the
other, found while reviewing the register itself. Item 6 has NOT been through
this pass; it carries its own analysis above and is the second entry in the
order below.

| # | assumption | verified 2026-09-05 | how a failure surfaces |
| --- | --- | --- | --- |
| 1 | Corrosion path formula | `scripts/check-shared-cargo-dir-used.sh` present; 5 call sites of `nros_assert_shared_cargo_dir_used` in `NanoRosCorrosion.cmake` | **WITNESSED** — build-time assert on the RESULT, not the formula |
| 2 | `--artifact-dir` unstable flag | still passed by `packages/api/nros-c/cmake/nros-nuttx.cmake` | **LOUD** — cargo rejects an unknown flag; the NuttX lane fails at the copy |
| 3 | cargo `.fingerprint` parsing | both parsers carry `--self-test`; **0** references from `just/check.just` | **CONTAINED** — a schema change fails the self-test, and no gate consumes them, so a wrong number can mislead a person but cannot make CI lie |
| 4 | headers as a side channel in cargo's target dir | OUT_DIR emission present (`cpp.rs`, `scripts/build/cargo-out-dir-headers.py`), but **13 readers still on the side channel** | **SILENT — and it has now happened**, see below |
| 5 | probe depfile location | `probe_depfile` tries both spellings; `emit_probe_watches` PANICS naming the file when neither exists; pinned by `probe_depfile_found_beside_and_uplifted` | **LOUD** |

Item 5's mitigation is stated in this issue as "the lookup … PANICS", which reads
as if `probe_depfile` panics. It does not — it returns `Option`; the panic is one
frame up in `emit_probe_watches`. The behaviour is what was claimed, the location
is not, and following the caller is what settled it.

### Item 4 is no longer hypothetical

This register predicted the shape: *"share the target dir and the second image
takes a cache hit, the build script never re-runs, and the directory the
consumers were pointed at is never written."* Issue **1031** (2026-09-04) is that
outcome arriving by a different route — the size probe returned `EXECUTOR_SIZE =
0`, both build scripts took their documented early return, and neither header was
written. Every consequence the register names followed: the build exited 0, the
consumers reached the committed stub, and three `cxx-syntax` snippets failed
every scheduled run against `#error "must be supplied per-build"` while the step
that was supposed to produce the headers reported nothing wrong.

Two things that fell out of it belong here:

* The side channel had **no dependency edge at all**. Deleting a header did not
  bring it back, because cargo held the crate fresh and never re-ran the script —
  the artifact is inside cargo's tree and unmanaged by it, which is exactly this
  item. 1031 added `cargo:rerun-if-changed` on the header and its stamp, so
  cargo now reports `Dirty … the file … is missing` and regenerates. That is a
  partial mitigation of item 4, not a migration.
* On a developer tree the side channel is normally populated by some OTHER build,
  so the lane passes on residue and the failure looks CI-only. That is the
  property that makes this item's risk silent rather than loud.

### Why closing rather than keeping it open

Its purpose was to record an exposure reviewed once, so the review would not have
to be reconstructed from commit messages. That purpose is served, and it is now
served with a verification date and a detection column. A register left open
accumulates the appearance of unfinished work without anyone owning a next step.

**Re-review trigger is a version bump, not a calendar**: a Corrosion upgrade
(item 1, and it PREDATES phase-400), a cargo release that touches `.fingerprint`
or target-dir GC (items 3 and 4), or the NuttX crate leaving its nightly pin
(item 2). Item 4 also closes properly on its own terms the day the 13 readers
above reach zero — that is a migration with a countable finish line, and it is
the only one of the five that has one.

## Item 6's gap is CLOSED 2026-09-05 — the fallback branches announce themselves

The register's one entry with no detection story now has one.

**What was unguarded.** Both staleness probes decide from the artifact's bytes
(issue 0835). When the locator finds NOTHING they fall back to the older rule —
cargo's `"fresh":false` for the rust probe, the build-chatter grep for the cmake
one — and say nothing about having done so. That branch is what an on-disk
LAYOUT change produces: `_row_artifacts` globs
`<target-dir>/[<triple>/]<profile>/<bin>`, which is cargo's convention rather
than anything cargo promises, and `cmake-fixture-stale.sh` assumes a cell has
exactly one top-level executable.

The consequence is not a missing warning, it is a WRONG VERDICT that reads as a
right one: for rows sharing a phase-340 cargo group the old rule is
*permanently* stale, because those rows evict each other. Issue 0835 measured
that state — ~22 rows reporting stale on every run with byte-identical binaries,
and ~190 fixture-stale failures per `just ci-matrix` — and it surfaces as a
self-healing WARNING, which reads as the gate working.

**What landed.** Both fallback branches emit
`DEGRADED\t<row>\t<reason>`, the convention `rust-fixture-stale.sh` already used
for `FAILED\t`. `check-fixtures-stale.sh` buckets and counts them beside the
stale list, as a WARNING: a degraded row is not known to be stale or fresh, it
is known to have been decided by a rule this gate does not stand behind, so
escalating would redden a lane over something nobody can act on. **The COUNT is
the signal** — measured 2026-09-05, 116 of 117 rust rows and 120 of 120 cmake
cells locate their artifact, so the expected reading is ONE
(`packages/testing/qemu-smoltcp-bridge`, which names no `[[bin]]` the glob can
find). More than one means a layout moved.

This widens no watch set, so it cannot make 0835 worse — phase-424's constraint.
It is a fact ABOUT a verdict, never a new input to one.

**Gated, with the case that was missing.** `check-fixture-staleness-probes` had
cases A–D and every one of them has the artifact PRESENT, so none reached this
branch. Case **E** builds a LIBRARY-ONLY cargo leaf — the real shape, not a
contrived one, since that is exactly why `qemu-smoltcp-bridge` is on the
fallback today — and asserts both that `DEGRADED` is emitted and that it carries
a reason. Mutation-tested: removing the `printf` makes E fail, and the leaf then
appears in the output as an ordinary stale row, which is the silent state
itself.

**One defect found while writing it, worth recording because it would not have
failed loudly.** A degrading probe prints its `DEGRADED` line *and*, if the
fallback rule fires, the stale line. `check-fixtures-stale.sh` reads probe
output two different ways — `mapfile` when GNU `parallel` is installed, one
`$( )` capture per record when it is not — so the pair arrives as two array
elements or as one, and bucketing without re-splitting would have classified
them by whichever line came first, **differently depending on whether a tool is
installed on the machine**. The caller re-splits on newlines before bucketing;
five shapes are checked, including that divergence.

The rest of item 6 stands as written: the layout assumption itself is still an
assumption. What changed is that it can no longer fail in silence.
