---
id: 1249
title: "A command substitution whose status is meant to be INSPECTED, written as a bare assignment under `set -e` — the class behind two same-day defects, swept and gated"
status: resolved
type: tech-debt
area: ci, build, process
severity: medium
found: 2026-09-09
related: [1246, 1077, 0196, 0442]
---

# The idiom, not the site

Two independent defects landed on 2026-09-09, in different files, by different
routes, wearing the same three lines:

```sh
out="$(cmd)"        # under `set -e`, a non-zero `cmd` ENDS THE SCRIPT HERE
rc=$?               # never runs; and if it did it would read 0
if [ "$rc" -eq 2 ]; then ...
```

* **PR #780** — `scripts/build/workspace-fixtures-build.sh`. A `find` was handed
  a doubled path that has never existed on any host, exited 1 with no output,
  and `set -euo pipefail` took the script down at the assignment. The
  `if [ -n "$_sf_elf" ]` written directly beneath it — whose entire purpose was
  to tolerate an absent elf — had never executed once. The workspace fixture
  build died on its first entry with `error: recipe … failed on line 234` and
  nothing else; a stack-floor gate had therefore never run.
* **PR #798** (issue 1246) — `.githooks/pre-push`. `out="$("$reach" --changed
  "$sha" 2>&1)"`, `rc=$?`, and a three-outcome `case` whose rc=2 arm existed to
  say "could not ask the remotes" and LET THE PUSH THROUGH. Under
  `set -euo pipefail` a `$reach` exiting 2 killed the hook at the assignment
  instead: pushes refused with ZERO OUTPUT, which teaches `--no-verify` and so
  bypasses every other guard in the hook at once.

Two instances in guard/build infrastructure on one day is a fact about the
idiom. CLAUDE.md's standing rule — *fix the CLASS, not the reported site, then
prove the sweep* — is what this issue records.

## Why it is invisible on review

The handling is right there, three lines down, correct, and reads as coverage.
Nothing in the diff says the shell will never reach it. And the failure has no
signature of its own: `set -e` prints nothing, so the symptom is the caller's
generic "recipe failed" or an exit status with no output at all. Both authors
above wrote a considered comment about the failure mode they were handling,
directly above code that could not run.

Same shape as issue 1077 one lane over (a `pipefail` SIGPIPE turning a match
into a miss): a red that is not a verdict.

## The sweep

`scripts/check-set-e-bare-assignment.py` over every tracked `*.sh`, `*.just`,
`justfile`, `.githooks/*` and `.github/workflows/*` — 340 files.

**1278 command-substitution assignments examined; 16 changed.** Every one of the
16 had an explicit empty-or-failed handler, with a diagnostic, that the shell
could not reach:

| file | what could not run |
| --- | --- |
| `tests/c-msg-gen-tests.sh` | `RESULT=$?`, `echo "$OUTPUT"`, "failed with exit code" — a failing test executable produced no output at all |
| `scripts/check-kconfig-knob-forwarding.sh` | the gate's own `[FAIL] no _nros_resolve_knob() calls found` |
| `scripts/check-staleness-probe-exemptions.sh` | the gate's own `[FAIL] no entry points found` |
| `scripts/ci/issue-ids-check.sh` (×2) | `[FAIL] duplicate ids` on an empty series |
| `scripts/check-decoupling.sh` | `FAIL: no packages/*/$crate/Cargo.toml found` |
| `scripts/check-leaf-lockfiles.sh` | the `[ -z "$gv" ] && continue` skip |
| `scripts/build/host-only-members.sh` | the `[ -n "$name" ] \|\| continue` skip |
| `scripts/bump-manifest.sh` | `ERROR: could not read the git URL out of the manifests` |
| `scripts/nuttx/build-nuttx.sh` (×2) | `declares no CONFIG_ARCH`; the no-tarball branch |
| `scripts/installers/arm-fvp-installer.sh` | the whole multi-line "expected layout" hint |
| `scripts/stack-analysis-c.sh` | the clang-vs-gcc `-fstack-usage` hint |
| `scripts/qemu/build-zenoh-pico.sh` | `zenoh_manifest_die "… holds no .c files"` |
| `packages/…/alloc_free_audit.sh` | `FAIL: no libnros_rmw_cyclonedds*.rlib found` |
| `ci/nano-ros-sdk/scripts/build-arm-none-eabi-gcc.sh` | `no extracted toolchain dir` + the `ls -la` dump |

Sweep command:

```sh
python3 scripts/check-set-e-bare-assignment.py
```

## What was deliberately NOT changed

**Not every bare assignment is a bug**, and the sweep's value is in the sites it
left alone. Three groups:

1. **`awk` / `sed` / `basename` / `cut` heads.** They report "nothing matched"
   as exit 0 with empty output, so the author's `[ -n "$x" ]` is live and the
   code is right (`.github/workflows/docs.yml`'s mdbook-version read is the
   canonical one). If their INPUT FILE is missing they do exit non-zero — and
   aborting is then the right answer, because a missing input is a broken
   precondition, not a result.
2. **Pipelines without `pipefail`.** `v=$(grep -m1 '^version' "$f" | cut -d'"'
   -f2)` cannot abort however grep exits: the pipeline's status is `cut`'s.
   Eight of the tree's candidate sites are exactly that shape
   (`justfile:4587`, `just/esp_idf.just:104`, `scripts/qemu/setup-qemu-network.sh:80`
   among them). Flagging those would have been the mass-rewrite the gate exists
   to avoid — and an early draft of the gate did exactly that, because its
   `set` parser did not understand `set -euo pipefail` as a bundle.
3. **Failures that SHOULD abort.** `root="$(git rev-parse --show-toplevel)"`,
   `dir="$(mktemp -d)"` with nothing inspecting them afterwards. `set -e` is the
   correct handling, and there is no handler to strand.

Also left: `just workspace doctor`, which runs `set +e` on purpose and reads
`rustup_rc=$?` legitimately. The gate tracks `set +e` and does not flag it.

## The gate

`just check set-e-bare-assignment` — buildless, source-only, on the fast line
(so it runs on every push through the `pre-push` hook). Two rules, and each keys
on the author's OWN evidence of intent rather than on a bare assignment alone:

* **status** — a bare assignment whose next statement is a pure status capture
  (`rc=$?`), reached across block closers (`fi`, `esac`, `done`) because that is
  how PR #798 was written. Needs no judgement about the command: after a bare
  assignment under `set -e`, `$?` is either unreachable or a constant 0.
* **emptiness** — a bare assignment whose head command spells "nothing found"
  as a NON-ZERO EXIT (`find`, `grep`, `ls`, `command -v`, `readlink`,
  `pkg-config` — a closed list), followed by a test of that variable for
  emptiness. Under `pipefail`, any stage counts; without it, only the last.

A GitHub `run:` block is `bash -e {0}` by GitHub's own default, so errexit is on
there whether or not anyone wrote `set -e`; the gate treats workflows that way.

### Mutation-tested, and the first version failed

The self-test runs on the normal path, with both real cases in both shapes plus
five negative controls. Four mutations, all killed:

| mutation | result |
| --- | --- |
| `workspace-fixtures-build.sh` loses its `\|\| true` | killed |
| `pre-push` back to the full pre-fix shape (3 bare arms, `rc=$?` after `esac`) | killed |
| detector blinded to `find` | killed by the self-test |
| detector loses closer-skipping | killed |

The second one **survived the first version of the gate**, and that is the
finding worth keeping. The self-test had used a flattened two-line rendering of
PR #798 and passed; the real file puts the assignment behind a `case` PATTERN
and the `rc=$?` behind `esac`, and a detector anchored on "the literally next
line at statement position" could see neither. A self-test written from the
commit message rather than from the file is a negative control that agrees with
your model of the bug instead of with the bug.
