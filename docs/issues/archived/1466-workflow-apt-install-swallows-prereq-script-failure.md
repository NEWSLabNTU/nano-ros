---
id: 1466
title: "The apt idiom `check-workflow-indexed-apt` PRESCRIBED swallowed its own script's
  failure: `apt-get install $(prereq-packages.py …)` installed nothing and the step
  SUCCEEDED — measured live in live-peer, which then ran without clang"
status: resolved
type: bug
area: [ci, tooling]
severity: medium
found: 2026-09-23
related: [1249, 1364, 1359, 1452, 1465]
resolved_in: "2026-09-24 — every call site captures the status, and the gate that prescribed the idiom now refuses it"
---

## What it was

A command substitution's exit status does not reach the command it sits in, and
`apt-get install` with zero package arguments exits 0. So

```sh
apt-get install -y --no-install-recommends \
  $(python3 scripts/sdk/prereq-packages.py --manager apt clang libclang-dev)
```

installs NOTHING and SUCCEEDS when the script dies. Measured live in `live-peer
regression` run 35817736997: a `ModuleNotFoundError` traceback three lines above
`0 upgraded, 0 newly installed`, green step, lane ran without clang.

The reason it was filed rather than fixed: **`check-workflow-indexed-apt.py`
prescribed that exact line**, in its remedy text and as a PASSING self-test case.
Every site that complied inherited the defect and a new site would have too.

## Re-measured (the issue's own table was a day stale)

| site | line | errexit |
| --- | --- | --- |
| `.github/workflows/docs.yml` | 92–93 | job default `bash -e {0}` |
| `.github/workflows/live-peer.yml` | 464–465 | job `shell: bash` ⇒ `-eo pipefail` |
| `.github/workflows/nightly.yml` | 824–825 | job `shell: bash` ⇒ `-eo pipefail` |
| `.github/workflows/release-nros.yml` | 124 | `bash -e {0}` |

Errexit is armed at all four and cannot see the substitution's status, so the
mechanism is exactly as filed. The gate's own two sites re-measured: remedy text
at `:212`, passing fixture at `:157`.

Two corrections to the issue as written:

* **`just/ci.just:464` was listed as already CORRECT. It was not.** The recipe
  `provision-zenohd` opens `set -uo pipefail` with **no `-e`**, so the assignment
  is not fatal either: a failed resolve leaves `pkg` empty, execution continues,
  and line 493 runs `apt-get install -y --no-install-recommends "$pkg"` with a
  quoted empty argument — which exits 0 — and then prints `provision-zenohd:
  installed `. Same swallow, reached through a missing `-e` rather than through
  argument position. Live: `just native setup` calls it.
* **`scripts/ci/runner-container.sh:124` is the only exemplary site**, and it is
  the shape everything moved to: `if ! VAR="$(…)"; then <diagnostic>; exit 1; fi`.

`prereq-packages.py` can never legitimately print an empty list — it raises on an
unmapped key, including the deliberate `noble = []` "not packaged there" case
(verified: jammy resolves, noble exits 1) — so capturing the status is
sufficient and `test -n` would be redundant. The status was the whole signal, and
nothing consumed it.

## What landed

1. **All five call sites** moved to `if ! pkgs="$(…)"; then … exit 1; fi` —
   the one shape that is fatal whether or not `set -e` is on, which matters
   because half this tree's recipes are not.
2. **`check-workflow-indexed-apt.py` gained rule 2**: every call site of
   `prereq-packages.py` must capture its status, and the gate's remedy text and
   self-test fixture now print that shape. The reach is every call site of the
   helper — workflows, `just` recipes and shell scripts — because that is the
   rule; a workflow-only reach would have left `ci.just`, the 0196 shape.
   The marker is the tool INSIDE a `$(…)`, not its name on the line: the first
   draft flagged all four of the gate's own remedy `echo`s, a gate reporting its
   own advice as the defect.
3. **`check-release-manifest.py` R4 was narrowed** to match the rule it states.
   It read "every `exit 1` must mention codegen" while its rule is "no OTHER
   VERSION COMPARISON may block a release" — wider than the rule, which is issue
   1452's shape. It had never been measured against anything, because both
   `exit 1`s the workflow carried sat in the codegen paragraph; the zstd fix is
   the first legitimate non-version fatal path, and a release that cannot be
   BUILT must still stop. The subject is derived now (does this fatal path
   compare two values?), with `case … in` counted as a comparison so the
   crate-prefix assertion the gate already caught is still caught.

## The sweep: two more producers read as an empty answer

Same class, different construct, both found by sweeping every `$( … )` and
`< <( … )` whose producer can fail:

* **`scripts/check-rust-targets-installed.sh:43`** — `done < <(nros_rust_targets
  rustup)`. The helper returns 2 printing nothing when `config/rust-targets.txt`
  is unreadable, so the loop read zero lines and the gate printed `OK (0 rustup
  target(s) present)` and exited 0. The count in that OK line was a SECOND
  invocation of the same failing helper — issue 1025's shape. Now resolved once,
  status checked, and zero declared targets is itself a refusal.
* **`just/workspace.just:387`** — `for f in $(just workspace
  _pinned-toolchain-files)` inside `doctor`, which runs `set +e`. The producer is
  `set -e` and its `emit() { [ -f "$1" ] && echo "$1"; }` returns 1 for a file
  that is merely ABSENT, so a missing optional `rust-toolchain.toml` aborted the
  enumerator mid-list and `doctor` printed `[OK] rust-pinned-toolchains` having
  examined zero files. Both halves fixed: `emit` returns 0 (absence is its
  expected case), and `doctor` captures the status and refuses to print `[OK]`
  over a list it never obtained.

## `check-set-e-bare-assignment` has a real reach gap for this construct

Measured, not inferred. Its file set is already right — 380 files, including all
17 workflows, and `scan_text` sets `errexit = is_workflow` because a `run:` block
is `bash -e {0}`. The PREDICATE is what cannot express this: everything routes
through one `ASSIGN` regex requiring `<var>="$(` at statement position, and on no
match the scanner advances a line. A substitution in argument position has no
`var=` and is invisible at the first regex; `ASSIGN.match('  apt-get install -y
$(python3 x.py)')` is `None`, and `scan_text` over the real live-peer block
returns `[]`. It also skips `just/ci.just` for a second, independent reason:
`set -uo pipefail` leaves `just_errexit` false and the whole recipe is skipped.

So this is a genuinely new rule, not a widening of 1249's, which is what the
issue said.

**A general argument-position rule was considered and NOT written.** The sweep
counted 742 argument-position substitutions in the tracked shell/just/workflow
set, ~123 of them invoking an in-repo script or tool; a blanket rule would need
an allowlist of roughly that size, which is the authored-list drift this repo
refuses elsewhere. Scoped to the helper the prescribing gate names, rule 2 needs
no list at all. The remaining candidates were judged individually and the two
above were the only other REAL ones; the rest fail closed (a missing profile
directory makes `check-no-alloc-image.py` red), are authored tolerances
(`|| true`), or call pure `printf`/`case` helpers that cannot return non-zero.

## Controls

* **1466**: revert `docs.yml` to the swallowing spelling → the gate FAILS naming
  `docs.yml:96`; restore → OK over 17 workflows and 324 caller files. Rule 2's
  self-test carries 8 cases both directions, including a comment, a heredoc body,
  a bare mention, a plain (non-substituted) invocation, and a line-number
  assertion so the report does not send a reader to the top of a 900-line file.
* **sweep A**: with `config/rust-targets.txt` moved aside, the gate now refuses
  instead of printing `OK (0 …)`.
* **sweep B**: with the enumerator made to fail, `doctor` prints `[ERROR]
  rust-pinned-toolchains: could not enumerate the pinned toolchain files` and no
  `[OK]` line.
