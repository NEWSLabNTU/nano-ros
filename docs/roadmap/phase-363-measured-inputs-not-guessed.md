# Phase 363 — Measured inputs, not guessed ones

**Status (2026-08-15). W1–W4 LANDED, each acceptance-verified; W5 planned.** Every freshness check in
this tree answers "was this built from the sources on disk right now?", and they
split cleanly into two kinds: the ones that ASK the tool that owns the
dependency graph, and the ones that GUESS an input set by hand. Every recurring
staleness bug this year came from the second kind. This phase converts guesses
into measurements, one site at a time.

**Owns:** the remaining half of
[issue 0466](../issues/0466-tier1-setup-contract-unstated.md) (the compile-check
signature's dependency closure) plus four sites found by surveying for the same
shape.

**Related:** [issue 0196](archived/) (build-side probes must watch the same
inputs as test-side gates — the rule this phase enforces mechanically),
[issue 0491](../issues/) (a `rerun-if-env-changed` on a PATH variable — the same
class in the env dimension), [phase-319](archived/) (`.inputsig`, which
introduced the signature scripts), [phase-354](phase-354-build-correctness-lane-seams.md)
W2 (#466's owner; this phase takes its last item).

## The thesis

A freshness check needs the set of files a build consumed. There are only two
ways to get it:

* **Measure it.** The compiler/linker/build system already computed it. GCC and
  clang emit it with `-MD`; cargo writes `*.d` dep-info; bindgen will emit
  `cargo:rerun-if-changed` for every header it read; cmake/ninja keep it in
  `.ninja_deps`.
* **Guess it.** Write down the paths you think matter, in a script, by hand.

The industry settled this. Ninja folds every `-MD` depfile into a binary deps
log and reloads it next invocation. ccache stores a MANIFEST of the include
paths *and their content hashes at store time*, then re-hashes to validate.
Bazel goes further and uses the `.d` to VERIFY that the declared inputs were
complete, treating an undeclared input as an error rather than a guess to
tolerate. Go's module `dirhash` hashes every file in a tree with **no type
filter at all**. Nix's `lib.fileset` exists because the previous hand-filtering
API was too error-prone, and offers git-tracked-ness as a first-class source of
truth.

Nothing mature filters source inputs by file extension. We do it in two places.

## What the survey found (2026-08-15)

Measured, and therefore NOT in scope — recorded so nobody "fixes" them:

* `scripts/test/rust-fixture-stale.sh` runs `cargo build --message-format=json`
  and reads `"fresh":false`. Cargo owns the graph; the probe asks it.
* `scripts/test/cmake-fixture-stale.sh` runs the incremental `cmake --build` and
  decides from its output. Same shape.

Guessed, and in scope:

| # | site | what it guesses | evidence |
| --- | --- | --- | --- |
| W1 | 4 bindgen `build.rs` | `wrapper.h` + an include DIR, instead of the headers bindgen actually read | 0 of 4 call `CargoCallbacks` |
| W2 | `NanoRosGenerateInterfaces.cmake` | `file(GLOB)` with no `CONFIGURE_DEPENDS` | a NEW `.msg` is invisible until something else reconfigures |
| W3 | `compile-check-signature.sh`, `workspace-fixture-signature.sh` | an extension ALLOWLIST over `git ls-files` | drops 45 `.conf`, 4 `.msg`, 9 `.yaml`, 1 `.json` (a TARGET SPEC) |
| W4 | `compile-check-signature.sh` | `sig_paths=("$dir")` — the row's own dir, not its path-dep closure | #466's last item |
| W5 | `zephyr.rs::source_dir_is_stale` | a hand-authored candidate list | deliberately over-broad; fails safe |

## W1 — bindgen tells you what it read (LANDED)

`nuttx-sys`, `threadx-netx-sys`, `zephyr-posix-sys` and `freertos-lwip-sys` each
watched `wrapper.h` plus a guessed include directory. bindgen has emitted
`cargo:rerun-if-changed` for every header it opened since 0.53, via
`parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))`. None of the four
used it.

The guess is wrong in the direction that produces museum artifacts: a transitive
RTOS header two levels below `wrapper.h` could change with the bindings never
regenerating. The directory watches partly cover this by accident — a
`rerun-if-changed` on a DIR watches its immediate entries — but not
transitively, and not across the several include roots each of these adds with
`clang_arg("-I…")`.

**Acceptance — MET.** Measured on `nuttx-sys`, built for
`riscv32imac-unknown-none-elf` against the configured NuttX tree:

| | watches |
| --- | --- |
| before (hand-written) | 3 |
| after (emitted by the callback) | **29 unique** |

The 26 new ones are transitive headers the guess could not reach —
`arch/types.h`, `arch/inttypes.h`, `sys/socket.h`, `endian.h`, `fcntl.h` … A
`rerun-if-changed` on a DIRECTORY watches its immediate entries, so nothing
under `include/arch/` or `include/sys/` was watched at all before this.

Existing hand-written lines stay. bindgen can only report files that EXIST, so a
header added LATER to an `-I` root is invisible to it — ccache documents the
same hole for its direct mode — and the directory watches cover that case.

Bonus, not planned: `CargoCallbacks` also emits `rerun-if-env-changed` for every
env var bindgen reads, which is the issue-0491 dimension arriving measured
rather than authored.

## W2 — `file(GLOB)` without `CONFIGURE_DEPENDS` (LANDED)

`nros_generate_interfaces()` auto-discovers `msg/*.msg`, `srv/*.srv`,
`action/*.action` when the caller names no files. Plain `file(GLOB)` runs at
CONFIGURE time only, so adding a message file leaves the build generating the
old set until an unrelated edit forces a reconfigure. The tree already uses
`CONFIGURE_DEPENDS` in 26 other places; these six were missed.

**Acceptance — MET.** 10 globs converted (9 interface globs + the `package.xml`
scan that discovers msg PACKAGES). Verified against cmake's own machinery rather
than against a build log: `CONFIGURE_DEPENDS` makes cmake generate
`CMakeFiles/VerifyGlobs.cmake`, which re-runs each glob and touches
`cmake.verify_globs` on a mismatch, forcing the reconfigure.

```
no change        -> 0 mismatches
add package.xml  -> 1 mismatch     (reconfigure forced)
remove it again  -> 0 mismatches
```

Proved for the glob that actually executed during that configure (the
`package.xml` scan). The other nine are the same construct in the same file and
run only in the auto-discovery branch, which the probe package does not take.

Method note: the first attempt at this compared "Configuring done" between
builds and appeared to pass. It was meaningless — that build was failing for an
unrelated reason and reconfigured every time, so the line appeared in the
control too. Testing the MECHANISM (VerifyGlobs) rather than a symptom of it is
what made the result trustworthy.

## W3 — delete both extension allowlists (LANDED)

The two signature scripts enumerate with `git ls-files --cached --others
--exclude-standard` — correct, and the reason the filter is redundant: git's
ignore rules already exclude build output. Then each applies a `case` allowlist,
and the two lists DISAGREE with each other (`*.yaml` is in one, not the other).

Measured over the dirs each signature ACTUALLY covers (an earlier count in this
doc was taken over every `[[fixture]]` dir, most of which no signature hashes —
they use the measured cargo/cmake probes; corrected here):

| lane | dirs | dropped |
| --- | --- | --- |
| workspace | 14 | 37 `.conf`, 9 `.yaml`, 1 `.msg`, 1 `.json` |
| compile-check | 24 | 8 `.conf`, 3 `.msg` |

`.conf` is the third sighting of one bug — issue 0466's `prj.conf` hole, and
#167's before it. The single `.json` is
`realtime-rust/riscv32imac-unknown-nuttx-elf.json`, the custom RISC-V **target
spec**: edit it and the ABI moves. Nobody decided a target spec was not a build
input — it simply was not on the list, which is the argument against allowlists
in one file.

The fix is deletion, not extension: hash everything git tracks under the paths,
optionally minus a tiny denylist (`.md`, `.gitignore`) whose only cost is a
false-stale. While there, adopt Go `dirhash`'s manifest form — per-file
`<hash>  <path>`, sorted, then hash THAT — so a stale verdict is diffable
instead of an opaque mismatch, and make read errors fail loudly instead of
`2>/dev/null`.

The two scripts are 77 and 90 lines of near-identical bash sharing no code. Do
this once, in a helper both call, or W3 lands twice and drifts again.

**Acceptance — MET.** `scripts/build/source-manifest.sh` is the one helper; both
signature scripts call it and neither retains a `case` filter. Mutation on the
real records:

```
workspace row examples/workspaces/c, edit src/zephyr_entry/prj-zenoh.conf
  before 32cd843f…   edited ff2272fb…   restored 32cd843f…      PASS
```

Also adopted from Go `dirhash`: the manifest form (`<sha256>  <path>`, sorted,
then hash that), so a mismatch is diffable. `sha256sum` emits that format
natively, so it costs one process rather than one per file.

**A bug this found in its own author's code.** The first version enumerated with
`done < <(git ls-files …)`, and a process substitution DISCARDS its command's
exit status — so a failed enumeration produced an empty list and a perfectly
valid-looking signature. That is the precise failure the helper was written to
remove, reintroduced while removing it. The self-test caught it before the
commit; enumeration now goes through a temp file, which also preserves the NUL
separators command substitution strips.

## W4 — the compile-check row's dependency closure (LANDED — #466's last item)

`sig_paths=("$dir")` covers the row's own directory. A compile-check row exists
precisely to compile AGAINST workspace crates, which are not in `$dir`, so an
edit to a dependency crate leaves the signature unchanged and the gate silent
while the tests' mtime check catches it. That is issue 0196's rule violated in
the direction that produces museum binaries.

Two designs, and they are alternatives rather than a sequence:

* **Pre-build (Bazel-shaped):** `cargo metadata` per row, filtered to path deps
  under the repo, widening `sig_paths`. Available before any build; needs a
  caching decision.
* **Post-build (ccache/ninja-shaped):** after a successful row build, store the
  row's dep-info closure WITH per-file content hashes, and have the probe
  validate against that. Strictly better — it stops guessing entirely — but
  needs the pre-build answer anyway to bootstrap a row that has never built.

Do NOT "fix" this by aligning the tests onto the current signature. The tests'
mtime check is the ONLY thing covering dependency-crate staleness today; moving
them onto a signature that cannot see those crates would delete the coverage and
look like a cleanup.

**Chosen: post-build.** The pre-build option is not merely more expensive here,
it does not work: a row's `Cargo.toml` carries `@NANO_ROS_ROOT@` placeholders
until the builder stages it, so `cargo metadata` fails outright on the source
dir. The staged tree, however, holds cargo's dep-info — 143 `.d` files for
`orch_tiers_multi` — and their union of in-repo entries IS what the build read.

`scripts/build/dep-closure.py` extracts it, parsing Make syntax including
escaped spaces (a naive `.split()` truncates a path with a space and silently
SHRINKS the closure into a valid-looking hash).

**Acceptance — MET**, on the exact case the issue names:

```
row orch_tiers_multi, edit packages/boards/nros-board-common/src/platform_config.rs
  before 4ca107a8…   edited 1537c369…   restored 4ca107a8…      PASS
```

196 in-repo paths now watched for that row, across `packages/core`,
`platform`, `api`, `rmw` and `cli` — none of them reachable from `$dir`.

### Extended to every builder (2026-08-15)

The first cut covered only rows with cargo dep-info. The other builders each
keep the same record under a different name, so the extractor now reads all
three:

| source | file | covers |
| --- | --- | --- |
| compiler / cargo dep-info | `*.d` (Make syntax) | cargo + `cxx-syntax` rows |
| CMake configure inputs | `CMakeFiles/Makefile.cmake` | `cmake-configure` rows |
| Ninja re-configure edge | `build.ninja` `RERUN_CMAKE` | `west-*` rows |

`cxx-syntax` had no dep-info at all, so it now compiles with `-MD -MF` —
which composes with `-fsyntax-only`: no object is produced, the dep list still
is. `cmake-configure` rows compile NOTHING, so `CMAKE_MAKEFILE_DEPENDS` is
their whole dependency set; one row lists 18 in-repo `cmake/NanoRos*.cmake`
modules, none of which any signature saw before.

```
row cmake_add_subdir, edit cmake/NanoRosCapabilities.cmake
  before … edited … restored …      PASS
row platform_hdr_nuttx (cxx-syntax)
  closure now names nros-platform-api/include/nros/platform.h — an include
  root `sig_paths` does not list
```

**Two defects this found in its own extractor**, both discovered by a closure
coming back empty when it plainly should not have, and both now asserted by
`check-source-manifest.sh`:

* a compiler's `-MD` output wraps with backslash continuations while cargo's
  does not. A per-line parser sees the continuation lines as colon-less and
  skips them, returning only the FIRST dependency — which hashes perfectly
  well.
* a depfile may record paths relative to the compiler's cwd. The extractor
  accepted only absolute paths, so it dropped the entire list. It now tries the
  repo root and the depfile's own directory and accepts only a base that
  actually resolves — guessing one would have re-created the same silent drop.

## W5 — the zephyr candidate list

`source_dir_is_stale` takes a source dir and hand-lists what to watch under it.
It has no extension filter and is deliberately over-broad, so it fails SAFE and
is the least urgent item here. Revisit only after W3/W4 exist: if the shared
helper is good, this becomes a caller of it.

---

## Deliberately not doing

* **Not adopting a new build system.** Bazel/Buck2/Nix solve this by making
  inputs declared rather than discovered. That is the right answer and the wrong
  size.
* **Not converting the measured probes.** `rust-fixture-stale.sh` and
  `cmake-fixture-stale.sh` already ask the owning tool. They are the model, not
  the work.
* **Not touching `rerun-if-env-changed`.** Issue 0491 is the same class in the
  ENV dimension and has its own gate (`check-path-env-fingerprints`); folding it
  in here would blur two acceptance criteria.
