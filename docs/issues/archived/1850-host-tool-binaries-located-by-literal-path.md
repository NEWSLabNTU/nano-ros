---
id: 1850
title: "The in-tree `nros` and `nros-launch-resolve` binaries are located by literal path at ~50 sites, beside the resolvers that exist for them"
status: resolved
resolved: 2026-10-13
type: tech-debt
area: build
severity: low
found: 2026-10-13
related: [0285, 0409, 1280, 1454, 1641, 1767, RFC-0103, phase-484]
---

## Measured (2026-10-13, tree at `a310e03054` + the 1767 close-out)

Residue of issue 1767 items 8 and 9: two first-party host tools, each with a
resolver, each still spelled as a literal path by most of its shell consumers.

**`nros`.** The shell resolver is `nros_cli_bin` (`scripts/build/cargo.sh:454`:
`$NROS_CLI` → this checkout's `packages/cli/target/release/nros` → PATH →
`$NROS_HOME/bin/nros`); cmake's is `nros_resolve_cli`
(`cmake/NanoRosCli.cmake:36`: `$NROS_CLI` → cached tool var → PATH → store).
Outside `cargo.sh`, the literal `packages/cli/target/release/nros` appears on
49 non-comment lines in 23 files under `just/`, `justfile` and `scripts/`
(`grep -rn "packages/cli/target/release/nros\b" just justfile scripts | grep -v
nros-launch | grep -v ':\s*#'`), the largest being `justfile` (11),
`just/check/cmake.just` (6: `:336`, `:346`, `:393`, `:403`, `:423`, `:432`),
`just/ros-editions.just` (3), `scripts/check-template-copy-out.sh` (3). Some
are `${NROS_CLI:-<literal>}` (`justfile:634`, `just/check/fixtures.just:235`,
`just/check/rmw.just:101`, `just/check/tools.just:228`), i.e. rungs 1–2 of
`nros_cli_bin` restated without rungs 3–4; most are the bare literal.

**`nros-launch-resolve`.** Three ladders:

- `nros_orchestration_ir::model_location::launch_resolver_bin_near`
  (`packages/core/nros-orchestration-ir/src/model_location.rs:333`):
  `$NROS_LAUNCH_RESOLVE` → the checkout owning `near` (or this crate's) →
  `<store>/bin/nros-launch-resolve`.
- `nros-cli-core` `cmd/ws.rs:2614` `launch_resolver_path` / `resolver_from`:
  `$NROS_LAUNCH_RESOLVE` → sibling of the running `nros` → the running CLI's
  checkout → (own SDK root).
- `scripts/build/launch-resolver-identity.sh:59`: `$NROS_LAUNCH_RESOLVE` →
  `${CARGO_TARGET_DIR:-<crate>/target}/release` → `packages/cli/target/release`;
  no store rung, no sibling rung. Plus the literals at `justfile:896`
  and `justfile:4391`.

## Why it is not one-line work

Most `nros` sites are PRECONDITION probes of a gate ("is this checkout's CLI
built?"), and `nros_cli_bin` deliberately falls to PATH and the store — so
routing them through it unchanged would turn a correct skip into a run against
another tree's binary (issue 1280's class). The fix is a mode on the ONE
resolver (checkout-only), not a second function, then a sweep with a gate
holding the literal out of code (negative control: a re-added literal).

The launch resolver's three ladders want one owner — most naturally `nros
locate tool nros-launch-resolve` (RFC-0103 D4's `nros locate`, extended to
`[tool.*]`-like first-party binaries) read by the shell, with the two Rust
ladders folded onto `launch_resolver_bin_near`.

## Direction

RFC-0103 D4 (one resolver, every road reads its answer).

## Fix (2026-10-13)

**`nros` — a mode on the one resolver, then the sweep.** `nros_cli_bin
--checkout-only` (`scripts/build/cargo.sh`) stops after rung 2: `$NROS_CLI`
(explicit, never exported by `activate.*`), else this checkout's build, else
NOTHING — exit 1, empty stdout. Never PATH, never the store. Python reaches
the same function through `scripts/lib/source_locate.py`'s `nros_cli_bin()`
(which shells it, rather than restating the ladder a third time). An
`export -f` copy of `nros_cli_bin` (a make leaf reaches it through
`nros_codegen_fingerprint`) sees `BASH_SOURCE` as the word `environment`, so
`cargo.sh` records its checkout in `_NROS_CLI_BIN_ROOT` when sourced — measured:
without it the leaf copy answered nothing. The sites,
by what they ask:

- **Gate preconditions → `--checkout-only`**, so a gate that skipped while this
  checkout's CLI was unbuilt still skips: the three cmake probes and
  `bounds-fragment-configure` (each now passes the resolved CLI to its test as
  `NROS_CLI`), `repr-memory-agreement` (via `nros_cli_usable`, whose default is
  now the resolver), the `${NROS_CLI:-<path>}` probes in `tools.just`,
  `fixtures.just`, `rmw.just`, `board-new`, `lock-update`, and the test bodies
  (`tests/cmake-*-tests.sh`, `nros-installer-tests.sh`, the two
  `nros-tests/tests/*_gate.sh`).
- **Strictly this checkout's → `NROS_CLI= nros_cli_bin --checkout-only`**, where
  the question is about the tree itself, not about a CLI: `doctor`'s
  PATH-shadow comparison (both copies), `setup-worktree`, the
  worktree-provisioning preflight, `format-cli`, `install-nros-cli`,
  `check-cli-fresh.sh`, and the tool-identity stamps (`codegen-fingerprint.sh`,
  `west-fixtures.sh`), which name the checkout they fingerprint with
  `--root <dir>` — `nros_codegen_fingerprint <root>`'s test builds a SYNTHETIC
  checkout, and anchoring on the file's own checkout read the real CLI there.
- **Consumers that just need a CLI → the full ladder**: `ros-editions.just`
  (which ran PATH FIRST — the D6 order fixed), `check-scaffold-builds.sh`,
  `check-template-copy-out.sh`, `dep-chain-check.sh`,
  `codegen-version-refusal-tests.sh`, `source_locate.py`.
- **Producer**: `setup-cli` keeps the path it builds, marked
  `host-tool-path-ok:` beside `nros_cli_bin`'s own rung 2.

**`nros-launch-resolve` — one owner.** `model_location::launch_resolver_from`
is the ladder, pure in its inputs: `$NROS_LAUNCH_RESOLVE` → this checkout's
build (the checkout owning `near`, else the one the crate was compiled from) →
beside the running binary → `<store>/bin`. `launch_resolver_bin_near` (build
scripts, `nros::main!`) and the new `launch_resolver_bin_for_exe` (the CLI)
both call it; `cmd/ws.rs`'s private `resolver_from` is gone and its tests now
drive the owner. `nros locate tool nros-launch-resolve` prints the answer
(anchored on the index's checkout), and the shell's `nros_launch_resolve_bin`
(`launch-resolver-identity.sh`) reads it, so the identity fixtures record and
`resolve-fingerprint.sh` name the binary `nros sync` actually runs.

Rung decisions:

- **Sibling of the running `nros`** (was only in `ws.rs`) — KEPT, placed after
  the checkout: it is the installed layout's identity when `nros` is unpacked
  somewhere other than the store, and an in-tree `nros` has no sibling, so the
  order changes nothing there. Not added to `launch_resolver_bin_near`, whose
  callers include proc macros running inside rustc — "beside the running
  binary" there means beside the compiler.
- **Store** (was missing from `ws.rs` and the shell) — ADDED to both by the
  fold. A store resolver from another play_launch pin is still refused by
  `verify_resolver_pin` (issue 0409), so the rung cannot skew silently.
- **`$CARGO_TARGET_DIR/release`** (was only in the shell) — NOT in the owner.
  An ambient target dir is a property of a shell, not of a checkout (issue
  0616's class), the CLI that spawns the resolver never read it, and its only
  setter (`ros2-box-env.sh`) retired it in issue 1248 — so an identity computed
  from it described a binary nobody ran. It stays on the PRODUCER side, where
  cargo really does write there: `nros_launch_resolve_built_bin`
  (`launch-resolve-stale.sh`) is the one spelling for `setup-launch-resolve`,
  the staleness predicate, `check-launch-resolve-fresh.sh` (whose self-test now
  injects through it), `doctor` and the provisioning preflight; and
  `setup-launch-resolve` now says so when a redirect moved the build out of the
  checkout, naming `NROS_LAUNCH_RESOLVE`.

**Chicken-and-egg.** `nros_cli_bin --checkout-only` needs no CLI — it answers
"none" before `setup-cli`. `nros_launch_resolve_bin` falls back, when no CLI
answers `locate tool` (none built, or one predating the verb), to
`$NROS_LAUNCH_RESOLVE` then `nros_launch_resolve_built_bin` — the first two
rungs of the same ladder, in one helper.

**Gate.** `just check host-tool-path-literals`
(`scripts/check/check-host-tool-path-literals.py`, derived into the fast lane)
refuses `packages/cli/target/release/nros` and `…release/nros-launch-resolve`
in code under `just/`, `justfile`, `scripts/`, `tests/` and
`packages/testing/**/*.sh`. Comments are prose; a definition carries an inline
`host-tool-path-ok: <why>` marker (four lines: `nros_cli_bin` rung 2,
`nros_launch_resolve_built_bin`, `setup-cli`); five files are allowlisted with
reasons (matcher inputs and workflow text other gates compare); a stale
allowlist entry fails. Its 11 self-test cases run on every invocation and
include the re-added literal in the spellings this issue counted.

Out of the gate's reach, on purpose: `.github/` (the producer: it builds the
binary and caches its directory before any resolver can run), `activate.*`
(puts the in-tree CLI on PATH, so it cannot ask one), Rust (the CLI's crates
are the ladders; `nros-tests::launch_resolver_bin` remains a checkout-only
`is_file` probe of this checkout's build), and the cmake message text in
`NanoRosCodegenCore.cmake`.

Sweep: `grep -rnE "packages/cli/target/release/nros\b|release/nros-launch-resolve\b"
just justfile scripts tests packages/testing --include='*' | grep -v ':\s*#'`, now
answered by `python3 scripts/check/check-host-tool-path-literals.py`.

## Verified (2026-10-13)

- The gate: `check-host-tool-path-literals: OK (11 self-test cases; 5
  allowlisted)`; over the tree before the sweep it lists 70 lines in 35 files.
  Negative control on the real tree: re-adding
  `[ -x packages/cli/target/release/nros ]` to a recipe fails it, naming the
  line.
- Still finds the CLI: `just check workspace-system-required` PASS,
  `just check bounds-fragment-configure` 6/6, `check-cli-fresh.sh` and
  `check-launch-resolve-fresh.sh` OK.
- The skip still fires: with `NROS_CLI=/nonexistent`,
  `workspace-system-required`, `bounds-fragment-configure` and
  `declared-qos-header` each print `[SKIPPED] … just setup-cli`. In a checkout
  copy with no built CLI and an `nros` on PATH, `nros_cli_bin --checkout-only`
  exits 1 while the full ladder returns the PATH binary — the 1280-class run
  this mode exists to refuse.
- One resolver answer: `nros locate tool nros-launch-resolve` (from the
  checkout, and from `/tmp` with `--index`) and `nros_launch_resolve_bin`
  print the same path; `launch-resolver-identity.sh` still reports the
  play_launch pin. `nros-cli-core` `launch_resolver_tests`: 10 passed,
  including a new case that an unbuilt worktree takes the STORE, never its
  parent's resolver.

## Follow-up (2026-10-13)

The Rust sites, the NuttX Makefile and the gate's reach — what the Fix left.

**One owner per binary for "where does THIS checkout's build put it".** The
launch resolver already had one, `model_location::launch_resolver_in_checkout`
(the ladder's own checkout rung). `nros` had none in Rust, so
`nros_build_paths::nros_cli_in_checkout(root)` is new — in `nros-build-paths`
because both workspaces' consumers (`nros-cli-core`, `nros-tests`) already
depend on it, so no edge and no lockfile moved. It is the PRODUCER's path; a
caller that wants a CLI to run asks a ladder.

Routed, by what each site asks:

- **This checkout's build, nothing else → the owner.**
  `nros-tests` `fixtures/binaries/mod.rs` (the west fixture's `tool:nros=`
  hash — the shell stamps it with `NROS_CLI= nros_cli_bin --checkout-only
  --root`, and the STALE message now prints the path it hashed rather than a
  literal), `fixtures/cache_key.rs` (`nros source-stamp` of this tree), and
  `nros-cli-core/tests/common/mod.rs::pinned_launch_resolver` (deliberately
  checkout-only, as its doc already argued), which
  `sizing_descriptor_model_road.rs` now calls instead of a seventh copy.
- **A CLI to run → `nros_tests::nros_cli_bin_path`**, which was itself a
  stale ladder: it claimed to mirror `nros_cli_bin` and still had the pre-W2d
  order with NO checkout rung (`$NROS_CLI` → PATH → store), so in a worktree it
  ran the parent's CLI (issue 1280's class). It now has the shell's order
  (`$NROS_CLI` → `nros_cli_in_checkout` → PATH → store), and three
  restatements fold onto it: `ros_env::host_nros_bin` (in-tree → PATH),
  `tests/non_goals_grep.rs` (in-tree → `~/.nros/bin`) and
  `process::nros_cli_path` (a private copy of the old order).
- **A resolver to run → `launch_resolver_from`.** `nros_tests::launch_resolver_bin`
  (the 4th ladder: a literal `is_file`) is now rungs 1-2 of the one ladder —
  `$NROS_LAUNCH_RESOLVE`, this checkout's build — with `beside` and `store`
  OFF on purpose. Its two callers (`launch_py_resolves_as_shipped`,
  `multihost_partition_bake`) assert about THIS checkout's layer-2 pin; a store
  resolver from another pin writes a model missing data rather than failing
  (issue 0409), and the CLI's guard for that rung (`verify_resolver_pin`) is
  not on a test's path. Same shape as `nros_cli_bin --checkout-only`.

Marked, not routed — each BUILDS a synthetic checkout in a tempdir, so the
literal is the fixture under test, and routing it through the owner would make
the test agree with the code by construction:
`stale_guard.rs` (`the_checkouts_own_binary_passes`), `cmd/ws.rs` (the
worktree-never-borrows test), `orchestration/nano_ros_root.rs` (two tests),
`nros-launcher/src/checkout.rs` (`an_in_tree_binary_answers_with_its_checkout`).
Each carries `// host-tool-path-ok: synthetic checkout under test`.
(`doctor.rs`'s `checkout/target/release/nros` never matched the pattern.)

**NuttX `apps-external-template/Makefile` — ROUTED**: `NROS_BIN` is used (it
runs `codegen-system`), and its restatement had drifted — no `$NROS_CLI`, the
store ahead of PATH. It now asks `nros_cli_bin` through `$(shell bash -c '.
"$1/scripts/build/cargo.sh" && nros_cli_bin' _ $(NROS_REPO_DIR))`, bare `nros`
only when the resolver answers nothing. Measured with the fragment in a scratch
makefile: this checkout's build; `/bin/true` under `NROS_CLI=/bin/true`; `nros`
for a nonexistent root. Not measured in a NuttX build (no merge lane builds
one).

**`cmake/NanoRosCodegenCore.cmake` — left**: the literal is prose in a
`FATAL_ERROR` remedy (`<this tree>/packages/cli/target/release/nros`), a path
for a human to type, and computes nothing.

**`.github/` — re-checked per site, all left.** Every literal sits in the step
that BUILDS the binary: the `actions/cache` `path:` lists (YAML data, no shell
to ask), the `source-stamp` assertion right after `cargo build` (which
`check-ci-cli-from-source` requires VERBATIM — substituting it fails that
gate), the `$GITHUB_PATH` line, `post-submit.yml`'s `nros setup` in the same
build step, and `release-nros.yml`'s staging (`cp …/nros "$stage/bin/nros"`)
and record/notes steps, whose subject is exactly the binary this job built —
a resolver honouring `$NROS_CLI` would be a wrong answer there, and none of it
is verifiable off CI. **`activate.*` — left**: they put the in-tree CLI on
PATH, run before any CLI can be asked, and `activate.fish` cannot source the
bash resolver.

**Gate.** `check-host-tool-path-literals` now reads every `*.rs` under
`packages/` (comments stripped by `scripts/lib/comments.py`; the marker read
from the raw line or the one above). 18 self-test cases (7 new Rust: three
positives incl. a string-literal message and a two-line join, a doc comment, a
trailing comment, a marked synthetic checkout, an owner call). Negative
control on the real tree: restoring the literal in `cache_key.rs` fails it
naming `cache_key.rs:520`; deleting one synthetic-checkout marker fails it
naming `checkout.rs:201`. Not matched, documented in the gate: split spellings
(`.join("target/release")` beside a name constant), the shape both owners use.

Sweep: `git grep -n 'target/release/nros\b\|target/release/nros-launch-resolve' -- 'packages/**/*.rs'`,
now answered by `python3 scripts/check/check-host-tool-path-literals.py`.
