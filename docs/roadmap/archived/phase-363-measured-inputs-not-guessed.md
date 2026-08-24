# Phase 363 — Measured inputs, not guessed ones

**Status (2026-08-24). COMPLETE — W1–W5 LANDED and each RE-SWEPT; archived. The waves are closed; the CLASS is not — four more sites turned up AFTER they were, the most recent being issue 0596. Treat this phase as a standing sweep, not a finished list.** Every freshness check in
this tree answers "was this built from the sources on disk right now?", and they
split cleanly into two kinds: the ones that ASK the tool that owns the
dependency graph, and the ones that GUESS an input set by hand. Every recurring
staleness bug this year came from the second kind. This phase converts guesses
into measurements, one site at a time.

**Owns:** the remaining half of
[issue 0466](../../issues/archived/0466-tier1-setup-contract-unstated.md) (the compile-check
signature's dependency closure) plus four sites found by surveying for the same
shape.

**Related:** [issue 0196](../../issues/archived/0196-native-rust-fixture-stale-probe-misses-generated.md) (build-side probes must watch the same
inputs as test-side gates — the rule this phase enforces mechanically),
[issue 0491](../../issues/archived/0491-leaf-relative-env-strings-thrash-shared-cargo-group.md) (a `rerun-if-env-changed` on a PATH variable — the same
class in the env dimension), [phase-319](phase-319-compile-check-lane-presence-to-truth.md) (`.inputsig`, which
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

### Re-swept 2026-08-16 — five more of the same construct, in a sibling file

W2 fixed the ten globs in `nros_generate_interfaces()`. Re-running its own sweep
found **five it did not reach**, all in `cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`
— and they are both sub-kinds W2 converted, not a new shape:

| glob | kind |
| --- | --- |
| `*/package.xml` (stub emission) | the msg-PACKAGE scan |
| `*/package.xml` (name-mismatch fallback) | same |
| `msg/*.msg`, `srv/*.srv`, `action/*.action` | the interface globs |

These matter more than the count suggests: the roots are
`NROS_INTERFACE_SEARCH_PATH`, i.e. the USER's own msg packages. So the defect
W2 describes — "adding a message file leaves the build generating the old set
until an unrelated edit forces a reconfigure" — was still live on the
user-facing path after W2 declared the class closed.

That is the issue-0196 shape inside this phase's own wave: the fix landed where
the symptom was seen. Recorded rather than quietly patched, because "the tree
already uses CONFIGURE_DEPENDS in 26 other places; these six were missed" was
the same sentence one iteration earlier.

Verified by W2's method — the MECHANISM, not a symptom:

```
control (no change)   -> 0 GLOB mismatch
add a msg package     -> 1 GLOB mismatch    (reconfigure forced)
remove it again       -> 0 GLOB mismatch
```

No `file(GLOB)` without `CONFIGURE_DEPENDS` remains in that file.

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

### Re-swept 2026-08-17 — the CLI's own freshness predicate had the same allowlist

W3 deleted the extension allowlist from both signature scripts. The tree's THIRD
freshness predicate kept one: `packages/cli/nros-cli-core/src/source_stamp.rs`
decides "is the in-tree `nros` CLI stale" from

    rel.ends_with(".rs") || rel.ends_with(".jinja")
      || rel.ends_with("Cargo.toml") || rel.ends_with("Cargo.lock")

266 tracked files under the dirs it watches fall outside that list. Almost all
are `tests/fixtures/**` data and correctly not build inputs — but one is:
`packages/cli/rosidl-bindgen/askama.toml`, which carries `dirs = ["templates"]`.
That file decides WHICH templates askama compiles into the binary, so editing it
changes what the build consumes while touching no `.rs` and no `.jinja` — the
exact argument the file's own comment already makes for `.jinja`, one file over.
The comment even states the rule it was breaking: *"Any input list here that
watches less than what the build consumes is the issue-0196 shape."*

Measured on a freshly built CLI, all three states:

```
baseline                          not stale
edit askama.toml `dirs`           not stale   <- the defect
restore                           not stale
```

and after the fix:

```
baseline                          not stale
edit askama.toml `dirs`           STALE
restore                           not stale   <- content-based, so no rebuild needed
```

Matched by BASENAME rather than adding `.toml` to the allowlist: a blanket
clause would pull in the fixture board and orchestration manifests, and a
fixture edit would then stale the CLI and force a rebuild for nothing — the cost
this predicate exists to avoid. Widening a guess is not the same as measuring.

**Also corrected here:** `compile-check-signature.sh` still told the reader that
the measured closure is "empty (and therefore inert) for rows with no cargo
dep-info — cxx-syntax, cmake-configure, west-*". The 2026-08-15 extension gave
every builder a record; the comment outlived the code and would have told the
next reader those rows still guess.

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

## W5 — the zephyr candidate list (LANDED, `d721f1771`)

`source_dir_is_stale` took a source dir and hand-listed what to watch under it —
no extension filter, deliberately over-broad, failing SAFE. It was scoped as the
least urgent item, to revisit once W3/W4 existed. That happened, and it did
become a caller of the shared helper.

The probe now reads the configure inputs the BUILD recorded rather than guessing
them: `ninja_configure_deps()` parses `build.ninja`'s `RERUN_CMAKE` edge, and
`require_prebuilt_binary_fresh_zephyr()` feeds those into the same content-aware
check the other lanes use. `ZephyrLeafSource` names the leaf's own inputs beside
them.

The first cut compared MTIMES across the ~3291 configure inputs, which reported
every image stale — a pull rewrites those files, which is the treadmill this
phase exists to stop trusting. Restructured to feed the content-aware path
instead.

Covered by `tests/zephyr_leaf_staleness.rs` — three mutation tests, each
asserting the probe FAILS when an input it claims to watch is changed.

---

## The site the sweeps found — `zpico_c_source_newer` (LANDED 2026-08-17)

Found by sweeping the W5 class the way W2 and W3 were swept. It is the same
shape three times over, in one function:

* a hand-authored ROOT — `packages/rmw/zenoh/zpico-sys/c`, a candidate list of
  size one, chosen because it is "the one purely-cargo C surface these fixtures
  link";
* an extension ALLOWLIST — `.c/.h/.cpp/.hpp/.cc/.hh`, the class W3 deleted twice
  elsewhere;
* MTIME rather than content, which W5 and `source_stamp.rs` both moved away from.

It exists for a good reason: corrosion invokes cargo as one opaque custom
command, so `ninja -t deps` cannot see anything cargo compiled, and issue 0391
is the museum binary that produced. The compensation is sound in intent and
guessed in construction.

**The measured answer is already on disk.** The `cc` crate emits
`cargo:rerun-if-changed` for everything it read, and cargo stores it:

```
build/cargo-fixtures/nuttx-*/…/build/zpico-sys-*/output   37 rerun-if-changed entries
  …/packages/platform/nros-platform-api/include
  …/config/bare-metal/nros-platform.toml
  …/config/freertos/nros-platform.toml
```

That set is BROADER than the walk. `nros-platform-api/include` is not under
`zpico-sys/c`, and `config/*/nros-platform.toml` is neither under it nor a
`.c`/`.h` file — so **editing a platform config does not stale a zpico fixture
today**, though cargo names it an input. CLAUDE.md separately records those
files as carrying the `rerun_if_env_changed` lists of issue 0491, which is the
same class in the env dimension.

**LANDED with the mutation tests it was held back for.** The probe now reads the
`cargo:rerun-if-changed` lines cargo stored, and the hand-authored walk survives
only as a BOOTSTRAP for a tree with no build-script output yet — over-broad,
therefore failing safe, which is the same reasoning W4 records for needing a
pre-build answer.

Three properties the measured path has to get right, each asserted:

* a recorded entry may be a DIRECTORY (`…/include`), and cargo means "anything
  under it" — the shape the old walk could not express;
* no extension filter on this path: cargo named it an input, so what it IS
  matters less than that it changed;
* both `cargo:` and `cargo::` prefixes — the `cc` crate still emits the legacy
  one, and a probe knowing only the modern spelling records nothing, silently.

Five mutations, all caught: drop the legacy prefix, ignore directory inputs,
drop the in-repo filter, and cap the search depth at 0 and at 4. `MAX_DEPTH` is
measured rather than chosen — the `zpico-sys-*` dir sits at recursion depth 5 in
a real corrosion layout (4 fails, 5 passes), and the shipped 8 is that bound plus
headroom. An earlier draft of that comment said "six levels", counted by eye.

### Why some leaves churned on every sync, and why a gate on it was wrong

Recorded because a tree-wide "canonical `include` spelling" gate was written,
pushed, and reverted here on the strength of guessing this mechanism.

`render_patch_config_with` evicts the sync-managed entries with `retain` and
puts the central one back with `insert(0, …)`. What happens to the array's
leading whitespace depends on whether anything SURVIVES the retain:

| leaf's include | after retain | result |
| --- | --- | --- |
| only `nros-patch.toml` | EMPTY — decor died with the last element | re-inserted tight: `["…"]` |
| plus `nros-board.toml` | survivor keeps the array's decor | preserved: `[ "…", "…"]` |

`retain` evicts only the central and managed patch entries, so `nros-board.toml`
survives. The predicate is therefore "does retain empty the array", NOT "how
many entries are there" — which is what the reverted gate assumed, and why it
failed three leaves that had never churned.

Isolated in a scratch toml_edit program before being believed, then pinned in
`config_writer_quoted_user_header_no_duplicate` so a toml_edit upgrade cannot
change it silently. Both halves mutation-tested.

The practical consequence is small and worth stating: leaves whose include holds
only sync-managed entries are rewritten by every sync unless they are stored in
the tight form. Those are the ones that churned, and normalising them is what
stopped it.

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

## Found after the waves closed (2026-08-17)

Two more, both after this doc first said the phase was complete. Recorded
because the pattern is the point: each was in a file the relevant wave never
opened.

**The zpico bootstrap now announces itself.** The hand-authored walk kept as a
fallback is unreachable for a built fixture — measured, all 76 zenoh build dirs
carry a `zpico-sys-*/output` with >= 5 in-repo entries. That makes it the
failure-mode handler rather than leftover guesswork: if the recorded set came
back empty and the probe returned `None`, it would report FRESH forever. The
defect was that the degradation was SILENT, so `probe_accounting()` now says
`INPUT SET UNMEASURED` in the line CLAUDE.md tells the reader to trust over the
verdict.

**Issue 0596 — `nros-launch-resolve`'s skew check was mtime.** The same class in
a predicate this phase never surveyed. It had already been moved from comparing
BINARIES to comparing SOURCES; the remaining half was that it compared source
MTIMES, so a rebase or stash re-armed it with identical bytes. Now a content
stamp, from ONE helper shared by all THREE copies of the comparison — the third
being an inline walk in `setup-launch-resolve` that decided whether to rebuild.

### Re-swept 2026-08-19 — W2's class survived in the Zephyr copy, and is now gated

The fifth re-sweep, and the same shape as the previous four: a sibling file the
wave never opened. `zephyr/cmake/nros_generate_interfaces.cmake` is a COPY of the
function W2 fixed, and its six interface globs — `msg/*.msg`, `srv/*.srv`,
`action/*.action`, local and ament — carried no `CONFIGURE_DEPENDS`. Same
consequence W2 names: adding a `.msg` leaves the build generating the old set
until something unrelated reconfigures.

Fixed, and this time gated: `check-interface-glob-configure-depends` (check-fast)
requires the flag on every glob whose pattern is `.msg`/`.srv`/`.action`. 21 such
globs tree-wide, all now flagged; mutation-checked by removing one and watching
the gate name the site.

**The gate is deliberately narrow.** The tree has 39 unflagged `file(GLOB)`s and
flagging them all would be noise — a glob over vendored ThreadX/CycloneDDS
sources only gains files on a submodule bump, which reconfigures anyway, and a
glob over the SDK store is one-shot discovery rather than a build input set.
Interface definitions are the case where USER content changes with nothing else
moving, which is what makes a stale capture reachable. A gate that cried wolf on
the other 33 would be bypassed, and then it would not catch the next `.msg` one
either.

This is the fifth site found after the phase declared itself complete. The
standing-sweep framing at the head of this doc is correct and should stay.

### The remaining mtime predicate, and why it stays

`just/zephyr-ci.just` re-ran `nros sync` for a package when its `package.xml`
was newer than a stamp — the last mtime predicate of this class. Converted
2026-08-19, and the cost turned out to be easy to measure:

| | syncs |
| --- | --- |
| first run (records the stamps) | 6 |
| `touch` every `package.xml`, then re-run — the rebase/stash shape | **0** |
| one package.xml genuinely edited | **1**, and the right leaf |

Before, the middle row was 6: every rebase, stash pop or branch switch re-armed a
sync for every Zephyr Rust leaf, because those rewrite tracked files with
identical bytes.

The input set is UNCHANGED — still the `package.xml`, nothing added — because the
direction of harm is what made this a deliberate exception rather than a bug:
over-regenerating is safe, and watching more would risk the opposite failure.
Only the comparison moved, from mtime to content. Two helpers in
`scripts/build/codegen-stamp.sh` beside the trait-surface stamp it already hosts,
so this is one more use of an existing mechanism rather than a second spelling.
An empty or missing stamp is a miss, so a pre-existing tree syncs once and
records the hash — no flag day.

`scripts/check-artifact-identity-budget.sh:267` also compares mtimes and is NOT
this class: it asks "was this rlib written during this run" (issue 0499), which
is genuinely a question about time.

## What remains, stated plainly

One guess survives on purpose (and after 2026-08-19 it is the ONLY one, the
zephyr-ci mtime predicate above having been converted): `zpico_c_source_newer`'s walk over
`zpico-sys/c`, reached only when no `zpico-sys-*/output` exists under the build
dir — a tree whose fixture has never been built by cargo. It is over-broad and
fails SAFE, and removing it would trade a rare false-STALE for a rare museum
binary, which is the worse direction.

### Audited 2026-08-19 — the guard is narrower than it looks, and correctly so

`zpico_c_source_newer` opens with `if !build_dir.contains("zenoh") { return None }`,
a substring test on a path. That reads like a guess this phase should have
converted, and the first pass through it here concluded exactly that — wrongly.
Recorded so the inference is not repeated.

**Provenance.** The guard arrived in `96e6d7729` (2026-08-02) and its doc comment
states its scope: *"when the fixture is a zenoh-backed CMAKE build (cyclone
fixtures don't link zpico → gate on the `build-zenoh` marker to avoid false
stales)"*. It is keyed on the cmake build-dir marker `build-<rmw>`, not on an
example layout, and that marker is still live: **76** `examples/**/build-zenoh/`
dirs, all 76 carrying a recorded `zpico-sys-*/output`.

**What looked like a hole is covered by a better measurement.** phase-340 moved
cargo fixtures into coordinate-keyed group dirs (`build/cargo-fixtures/linux-<hash>/`),
whose names contain no "zenoh" — so the guard declines them, and 237 fixture rows
declare `rmw = "zenoh"`. That is not a gap: **cargo folds a build script's
`rerun-if-changed` paths into the crate's dep-info**, so the zenoh group dir's
`talker.d` carries 9 in-repo zpico C/H paths —

```
…/zpico-sys/c/platform/errno_override.h
…/zpico-sys/c/zenoh-pico-version.h.in
…/zpico-sys/c/platform/bare-metal/platform.h
```

— and `dep_info_newer_source` reads that file. Those fixtures are therefore
covered by the strongest form this phase asks for: the tool that owns the graph,
answering for itself. The XRCE coordinate dir builds no `zpico-sys` at all, so
declining it is right too.

**So the guard's only failure mode is declining to check something already
checked better elsewhere.** It is a conservative marker for the one population
cargo cannot speak for — the cmake leaves, where ninja produces the binary and
there is no `.d` to consult. Left as it is.

Everything else the thesis names is measured: bindgen reports what it read (W1),
cmake re-globs (W2), signatures enumerate through the git index with no type
filter (W3), every builder contributes its own dependency record (W4), and the
zephyr and zpico probes read those records rather than a candidate list (W5 and
above). The CLI's own predicate keeps a filter BY DESIGN — it watches whole
crate dirs, so an unfiltered rule would stale the CLI on a fixture edit — and
that filter is now asserted rather than trusted.

The phase is complete. What it leaves behind is a habit rather than a backlog:
each of the three re-sweeps found a site the original wave had missed, always in
a sibling file, so the next person to touch a freshness check should re-run the
sweep for its class rather than trust that the class was closed.
