---
id: 1446
title: "`check-scaffold-builds` looks for a scaffolded Rust PROJECT's binary in
  two places RFC-0098 moved it out of, so the one variant that builds fine is
  the one it calls a vacuous pass"
status: resolved
type: bug
area: ci, tooling
severity: high
found: 2026-09-22
resolved: 2026-09-22
related: [rfc-0098, 1439, 1357, 1058, 1310, 1226, 1448, 1381, 1077]
---

## Problem

`just check scaffold-builds` reports

```
  rust_component: OK — 1 own artifact(s), e.g. target/debug/librust_component.rlib
  rust_project:   FAIL — the build exited 0 but compiled none of the emitted source
  c_component:    OK — 5 own artifact(s)
  c_project:      OK — 4 own artifact(s)
  cpp_component:  OK — 2 own artifact(s)
  cpp_project:    OK — 4 own artifact(s)
```

and the `rust_project` build log ends `Compiling rust_project v0.1.0
(/tmp/nros-scaffold-builds.xhNcLF/rust_project)` / `Finished dev profile`. The
binary is there:

```
/tmp/nros-scaffold-builds.xhNcLF/rust_project/build/native/target/debug/rust_project
```

`own_artifacts()` in `scripts/check-scaffold-builds.sh` looks in exactly two
places for a cargo artifact:

```sh
find "$dir/build"  -maxdepth 2 \( -name "lib${name}*.a" -o -name "$name" \) -not -path '*CMakeFiles*'
find "$dir/target" -maxdepth 3 \( -name "$name" -o -name "lib${name}.rlib" \) -type f
```

`$dir/target` does not exist for this variant, and the binary sits FOUR levels
under `$dir/build`, so neither find can reach it at any depth the script allows.
`rust_component` passes because a component leaf still writes `target/debug/`;
the PROJECT variant writes `build/<image>/target/`, which is where RFC-0098 D1
(phase-445 W6) put a leaf's generated cargo output.

## Why the verdict is worse than a plain red

The message says "compiled none of the emitted source", which is a statement
about the BUILD, and the build is fine. A reader lands on the scaffold or on
`nros new` and finds nothing wrong with either. It also makes the gate's own
anti-vacuity argument backwards: the predicate exists to refuse "an artifact
exists somewhere" as a pass, and here it refuses the one artifact that IS the
emitted source.

## Why it costs more than one gate — merged from issue 1448

`just ci gate` stops at the first failing step, so a red here **withdraws**
`check::api-parity`, `test-unit` and `test-lane-contracts` on every branch that
runs the lane — the lane CLAUDE.md tells every contributor to run before every
push. That is issue 1226's shape: a lane with no signal capacity, where the
next regression looks exactly like yesterday's failure. It is also why a
contributor who reads `CI GATE FAILED` here cannot tell their own defect from
this one, which is how three separate sessions hit it inside a week and two of
them filed it.

MEASURED across two checkouts, two branches and two sessions on 2026-09-22,
each rebased onto `main`, which is the evidence that it is `main`'s and not any
branch's:

| checkout | branch | result |
|---|---|---|
| `nano-ros-box2` | `fix/1437-c-cpp-granted-qos` | `rust_project: FAIL`, same wording |
| `nano-ros-box-box` | `feat/444-name-accessors` | `rust_project: FAIL`, same wording |
| `nano-ros-box2` | `docs/1447-1448-dedup` (DOCS ONLY) | `rust_project: FAIL`, same wording; `check::build` 1 of 22 |

The third row settles it. That branch's entire diff against `main` is three
files under `docs/issues/`, not one line of code, and `check::build` still
fails — on this gate and on nothing else. A branch that changes only markdown
cannot break a build, so what it reproduces is `main`'s. The first two rows
also touch neither `packages/cli`, the `nros new` templates, nor anything a
scaffold compiles against except by ADDING inherent methods; and the root cause
above says why none of them could matter: no diff can move a file from depth 4
to depth 2.

## What this is NOT — merged from issue 1448

- **Not issue 1439.** That one is a RUNTIME failure of the scaffolded Rust
  entry — it builds, links, opens its session and exits without publishing.
  Here the gate's verdict is about the BUILD, and the build is fine, so 1439's
  subject is a different stage entirely.
- **Not a disk-space or load artifact.** The runs above were on different
  checkouts with different build state, and every other variant in the same
  invocation passed.
- **Not the flake beside it.** `a_stalled_timer_reports_a_timer_overrun_violation`
  also failed in the same sweep and passes solo (0.13 s); that is the known
  in-sweep timing flake, not this.
- **Not the `nros new` Rust PROJECT template.** Issue 1448 proposed starting
  there — at #1150 (issue 1409) and #1155 / `a0f00adad` (issue 1435), the two
  template changes of that week. That lead is SUPERSEDED by the root cause
  above: the template emits a project that compiles, and the locator is what
  cannot see the result. Recorded because a wrong lead that is merely deleted
  gets re-derived by the next reader.

## Not caused by the change that found it

Found while re-running `check build` under issue 1434. That change touches board
crates, nros-node, the nros-c/nros-cpp headers, the entry emitters and their
goldens — no cmake, no build script, no path logic — and no content of it can
move a file from depth 4 to depth 2. It failed identically on both `ci gate`
runs, before and after an unrelated CLI rebuild, and in both provisioning states
of the worktree.

## Fix shape

Widen the cargo arm to the RFC-0098 layout — `$dir/build/*/target/{debug,release}`
— rather than raising `-maxdepth`, which would readmit the `CMakeFiles` vacuity
the comment above it describes. Assert the two layouts by NAME so the next move
is a failing test rather than a silent miss, and check `rust_component` still
resolves through the old one.

## Resolution

FIXED 2026-09-22 in `scripts/check-scaffold-builds.sh`. The root cause is exactly
as filed and nothing else was wrong: the build is fine, the locator could not see
its output.

### Reproduced, verbatim

Two failures, textually distinct, and only the second is this issue. Before
provisioning `packages/rmw/zenoh/zpico-sys/zenoh-pico` in the worktree:

```
  rust_project: FAIL — the emitted project does not build
      124:error: failed to run custom build command for `zpico-sys v0.5.0 (...)`
```

```
thread 'main' panicked at .../nros-zpico-build/src/runner.rs:1628:9:
zenoh-pico source not provisioned at ".../zpico-sys/zenoh-pico".
```

After `git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico`, the
issue's own failure, unchanged:

```
check-scaffold-builds: compiling 1 scaffold variant(s) outside the checkout
  rust_project: FAIL — the build exited 0 but compiled none of the emitted source
      (a successful build here leaves ~9 libraries, 8 of them nano-ros's
       own deps, so "an artifact exists" is the vacuous pass this
       predicate refuses; full log: /tmp/nros-scaffold-builds.ikkJgN/build.log)
```

while that log ends

```
   Compiling rust_project v0.1.0 (/tmp/nros-scaffold-builds.ikkJgN/rust_project)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.22s
```

and the whole scaffold holds exactly ONE path named after the package, four
levels under `$dir/build`, with no `$dir/target` at all:

```
$ find .../rust_project \( -name rust_project -o -name 'librust_project*' \) \
       -not -path '*/deps/*' -not -path '*/.fingerprint/*'
  build/native/target/debug/rust_project
$ ls -d .../rust_project/target
ls: cannot access '.../rust_project/target': No such file or directory
```

The generated settings file says where it went, and it is not a guess:

```
$ grep target-dir .../rust_project/build/native/nros-cargo.toml
target-dir = "native/target"
$ cat .../rust_project/.cargo/config.toml
include = ["../build/native/nros-cargo.toml"]
```

`target-dir` is relative to the file's GRANDPARENT (`<leaf>/build`, RFC-0098 D1),
so it resolves to `<leaf>/build/native/target` — and issue 1381's `include` is
what makes a bare `cargo build` inside the leaf honour it.

### Where every scaffold kind's artifact actually lands

Measured on all six, 2026-09-22, paths relative to the scaffold root. RFC-0098
moved ONE of them; the four cmake variants are untouched, which was checked
rather than assumed (the 0196 reach rule):

| variant | road | own artifact(s), measured |
| --- | --- | --- |
| `rust_component` | cargo | `target/debug/librust_component.rlib` — no `system.toml`, so no settings file, so cargo's default |
| `rust_project` | cargo | `build/native/target/debug/rust_project` — RFC-0098 D1, `[image.native]` |
| `c_component` | cmake | `build/c_component`, `build/libc_component_talker_component.a`, `build/CMakeFiles/*.dir/src/Talker.c.o` |
| `c_project` | cmake | `build/c_project`, `build/CMakeFiles/c_project.dir/src/main.c.o` |
| `cpp_component` | cmake | `build/libcpp_component_talker_component.a`, `build/CMakeFiles/*.dir/src/Talker.cpp.o` |
| `cpp_project` | cmake | `build/cpp_project`, `build/CMakeFiles/cpp_project.dir/src/main.cpp.o` |

So the split is not language, it is MODE-plus-`system.toml`: a leaf that states a
board gets a generated per-image `target-dir`, and `--component` emits no
`system.toml`. Both cargo shapes are now asserted by name.

### The locator, and the argument against the naive fixes

Three changes, and the second and third are what make the first safe:

1. **The target directory is READ, not derived.** `cargo_target_dirs()` parses
   `[build] target-dir` out of every `build/*/nros-cargo.toml` and resolves a
   relative value against the file's grandparent — cargo's own rule for a
   `--config` file, the same one `cargo_config::base_dir` encodes on the
   producing side. Plus `$dir/target` for a scaffold with no image. There is
   therefore no second derivation of the layout in this script to drift from the
   CLI's: if `nros sync` moves the output, the gate follows, and its verdict
   stays about the scaffold.
2. **The candidates are enumerated, never walked** — `<td>/<profile>/<name>`,
   `<td>/<profile>/lib<name>.rlib`, and the same two under a triple directory
   for a board with a rustc target. A bare recursive `find` or a `-maxdepth`
   bump readmits every dependency binary and every generated-message
   `CMakeFiles/*.o`, i.e. exactly the vacuity this predicate was written in
   issue 1058 to refuse.
3. **Every artifact must be NEWER than a stamp taken immediately before the
   build** (all arms, cmake included). `run_one` scaffolds into a fresh
   `mktemp -d`, so nothing stale can be there — the requirement is what makes
   that structural fact CHECKED instead of assumed, and it is what turns "I
   widened a search" from a hope into a property. The stamp is backdated one
   second so a coarse-mtime filesystem cannot tie; the slack can readmit
   nothing, because the only writer before it is `nros new` itself, moments
   earlier, into that same fresh directory.

The freshness filter costs no legitimate artifact: the four cmake variants report
the same counts as before the change (5 / 4 / 2 / 4).

**One more vacuous pass, found while auditing the same predicate.** The count
was `n="$(printf '%s' "$arts" | grep -c .)"`. A grep that ERRORS prints nothing
and exits 2, so `n` comes back empty, `[ "$n" -eq 0 ]` returns 2, bash SKIPS the
FAIL branch and falls through to `OK —  own artifact(s)`. That is issue 0726's
class one layer up from this one: a tool failure reported as a verdict, in the
predicate whose whole job is refusing vacuous passes. Now `nros_grep_count n .
<<<"$arts"`, which exits 2 rather than returning an empty number. Verified to
agree with the old spelling on 0 / 1 / 2 / 3 artifacts.

**Why not the shapes the issue's own "Fix shape" left open.** Four mutants, each
caught by a different control (all measured):

| mutant | control that refused it |
| --- | --- |
| the pre-fix `find "$dir/target" -maxdepth 3` | 1, "RFC-0098 project layout": found NOTHING — this is the bug |
| the NAIVE fix: recursive `find` over `build/` + `target/`, no freshness | 4, "stale artifact only": ACCEPTED a 2020-dated binary — a false green, strictly worse than the false red |
| GUESSING the layout as `build/*/target` instead of reading the settings file | 7, "settings file is read": missed a non-default `target-dir` |
| matching any file in the profile dir rather than the package's name | 5, "another package's artifact only": accepted `libnros_core.rlib` and `some_other_pkg` |

### Negative controls

`--self-test` now runs two groups, and the first needs no compiler, no CLI and no
network (`--locator-self-test` runs it alone, in milliseconds):

```
  locator control OK (RFC-0098 project layout): build/native/target/debug/rust_project
  locator control OK (component target/ layout): target/debug/librust_project.rlib
  locator control OK (nothing built): refused, as it must
  locator control OK (stale artifact only): refused, as it must
  locator control OK (another package's artifact only): refused, as it must
  locator control OK (no pre-build stamp): refused, as it must
  locator control OK (settings file is read, not guessed)
self-test OK: an undeclared type in the emitted source is caught.
```

The three refusals are the issue's three hazards: a scaffold that builds nothing,
a missing artifact, and — the one the widening introduces — a STALE artifact at
the right path. The fourth refusal is the precondition itself: with no pre-build
stamp the locator reports nothing rather than falling back to a lenient arm,
because a lenient arm here IS the vacuous pass.

**What the fix guarantees, precisely.** An artifact counts only if it is at a
path the scaffold's own generated settings name, is named after the emitted
package, and was written after the build started. It does NOT distinguish "this
build compiled the source" from "this build relinked an unchanged object" —
cargo/cmake freshness can in principle rewrite nothing — but in a fresh
`mktemp -d` there is no prior state to be fresh against, so within this harness
the two coincide. A future harness that reused a work directory would need to ask
the compiler instead (`cargo build --message-format=json`, `fresh: false`).

### Acceptance

All six variants green, for the right reason, `exit 0`:

```
check-scaffold-builds: compiling 6 scaffold variant(s) outside the checkout
  rust_component: OK — 1 own artifact(s), e.g. target/debug/librust_component.rlib
  rust_project: OK — 1 own artifact(s), e.g. build/native/target/debug/rust_project
  c_component: OK — 5 own artifact(s), e.g. build/c_component
  c_project: OK — 4 own artifact(s), e.g. build/CMakeFiles/c_project.dir/src/main.c.o
  cpp_component: OK — 2 own artifact(s), e.g. build/CMakeFiles/cpp_component_talker_component.dir/src/Talker.cpp.o
  cpp_project: OK — 4 own artifact(s), e.g. build/CMakeFiles/cpp_project.dir/src/main.cpp.o
check-scaffold-builds: OK
```

**Which greens depended on provisioning rather than on this fix — MEASURED, not
inferred.** NONE of them. Re-run with the submodule deliberately removed
(`git submodule deinit -f packages/rmw/zenoh/zpico-sys/zenoh-pico`):

```
  rust_component: OK — 1 own artifact(s), e.g. target/debug/librust_component.rlib
  rust_project:   FAIL — the emitted project does not build
      125:error: failed to run custom build command for `zpico-sys v0.5.0 (...)`
  c_component:    OK — 5 own artifact(s), e.g. build/c_component
  c_project:      OK — 4 own artifact(s)
  cpp_component:  OK — 2 own artifact(s)
  cpp_project:    OK — 4 own artifact(s)
```

So five of the six are green with NO provisioning at all: `rust_component` is a
library and the four cmake variants do not compile `zpico-sys`. Exactly ONE
variant, `rust_project`, needs `zenoh-pico` — and it is the same one this issue
is about, so its green needs BOTH the submodule and this fix. The two failures
are textually distinct and never confusable: `FAIL — the emitted project does
not build`, naming `zpico-sys`, versus this issue's `FAIL — the build exited 0
but compiled none of the emitted source`.

(An earlier draft of this section asserted that all six needed the submodule.
That was inferred from "every scaffold defaults to the zenoh RMW" and it is
wrong; the deinit above is why it is not still written here.)

The fix neither helped nor hurt the other five — identical artifact counts
before and after (5 / 4 / 2 / 4 and 1).

### Verdict text

The zero-artifact failure no longer asserts something about the build it cannot
know. It now says `the build exited 0 and left no artifact of the emitted
source` and PRINTS WHERE IT LOOKED, each target dir marked `(exists)` or
`(absent)` — because "the locator cannot see it" and "the build produced
nothing" read identically otherwise, which is what this issue cost.

### Gates

`check-scaffold-builds` (+ `--self-test`, + `--locator-self-test`),
`check-gate-lists`, `check-default-gates-run-somewhere`,
`check-set-e-bare-assignment`, `check-pipefail-sigpipe-assertions` — all OK. The
`just check/fixtures.just` recipe is unchanged, so the new controls run wherever
the gate already did.
