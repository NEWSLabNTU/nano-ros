---
id: 2831
title: "The Zephyr patch-set record refuses unrelated lanes and flips between branches sharing a workspace"
status: resolved
type: bug
severity: medium
area: [zephyr, build, tooling, ci]
related: [issue-2764, issue-2697, issue-2867]
found: 2026-11-22
resolved: 2026-11-23
resolved_in: "the change that archives this issue"
---

## What

Host 241, 2026-11-22, PR #4597's cell run. Every fixture build was refused,
including the NATIVE fixtures for the `zero_copy` cell, a lane that never
touches a Zephyr workspace:

    4.4: <store>/zephyr/4.4 has a patch record, but not for this checkout's
    4.4 patch set

An hour earlier, #4584's cell run had applied #4584's 4.4 set to the same
shared workspace, so the record held #4584's digest and not #4597's.

## Two defects

1. **The probe checks every line, for every lane that claims Zephyr.**
   `gate-lane-preflight.py` runs `check-zephyr-pin-drift.py` with no `--line`
   whenever the lane claims `zephyr` or `west`. A 4.4 record then refuses a run
   that builds only on 3.7, or none at all. The probe already takes
   `--line <l>`, so the preflight should pass the run's lines
   (`NROS_ZEPHYR_VERSION`, 3.7 by default), and refuse only those workspaces.
2. **The record keys on the scripts, not on what they did.** A record row is
   the sha256 of `patches/<line>.sh` plus every `scripts/zephyr/*.sh` it names
   (issue 2764). Two branches whose patch SCRIPTS differ in any byte have
   different digests. That includes a comment, or a helper refactor like
   #4584's in `provision-py312-venv.sh` that changes no edit in the workspace.
   Each must re-run its own set before its build is accepted, so parallel PRs
   on one host keep refusing each other. Rows accumulate rather than being
   replaced, so a re-run fixes it, but only by hand, after a refusal, every
   time a branch changes a byte of the set. The record answers "did THIS
   checkout's scripts run here", while the build needs "are the edits this
   checkout needs present".

## Direction (ruled 2026-11-22: shape (a), and (1) in the same PR)

- (1) is mechanical: pass the run's line(s) to the probe.
- (2) has two shapes:
  - (a) Digest the set's EFFECT: each patch script gets a `--check` mode that
    exits 0 when its edit is already present (they are idempotent, so each
    already knows how to tell), and the probe asks the set's scripts instead of
    comparing digests. Exact, but it touches all ~20 patch scripts.
  - (b) Digest only the patch payloads, the `patch`/`sed` bodies, not the
    comments or helpers around them. Cheaper, but it is a convention the
    scripts would have to follow, and a heuristic.

  Shape (a) is the honest one: the question is about the tree, so ask the
  tree.

## Acceptance

- A native-only or 3.7-only lane is not refused by a 4.4 record.
- Two branches whose 4.4 sets make the same edits both pass against a
  workspace that either one patched. A branch that adds an edit is refused
  until its set runs. Negative control: remove one edit from the workspace,
  and the probe names that patch.

## Fix

(1) Each probe asks only about a build that USES a workspace, and only on its
line:
- `gate-lane-preflight.py` passes `--line "${NROS_ZEPHYR_VERSION:-3.7}"`.
- `_require-owned-provisioned-roots` now takes the build's `lane`, and both
  fixture front doors pass theirs. A lane whose modules include no `zephyr`
  runs neither workspace probe, and any other lane probes only the run's line.
- `check-provisioned-root-guard-reach`'s planted edge uses the new spelling,
  and the gate is green.

(2) Shape (a), the tree is asked:
- `nros_patch_would_apply <tag> <what>` in `scripts/lib/zephyr-patchset.sh`.
  All 25 patch scripts the two sets run call it before each write. That is
  41 write sites. The three whose guard sits inside a Python heredoc exit 10
  there in check mode, and bash turns that into the helper's report.
- Under `NROS_PATCH_CHECK=1` the sets skip their non-edit steps (venv
  creation, the record) and say when every edit is present.
- `check-zephyr-pin-drift.py` runs the tree's line's set that way, failing
  with the script and edit that are missing. A set naming a script without a
  check mode is refused UNRUN, because that script would write to a shared
  workspace. The record's reader is removed. The record itself is still
  written but no longer a verdict; retiring it is a follow-up with the sync
  tests that manage it. (Done; see "The record, retired" below.)
- New gate `just check zephyr-patch-check-mode`: every script a set runs calls
  the helper.

Negative controls:
- The probe's self-test runs on a scratch checkout through the real helper:
  - an unpatched tree fails, naming its first missing edit, and stays
    unwritten;
  - a patched tree passes;
  - **one edit reverted fails, naming THAT script**, and the tree stays as it
    was;
  - a set changed only by a comment still passes;
  - a set naming a script without a check mode is refused unrun.
- Mutating the helper so check mode writes fails three of those cases.
- Per script, in scratch copies (sha256 of every file before and after each
  check):
  - pristine -> rc 1 MISSING;
  - patched -> rc 0;
  - check runs never write;
  - normal mode still applies, then reports already applied.
  - Every edit of the six multi-site scripts was removed one at a time and
    named.
- The real probe on host 139's two store workspaces left their `git diff`
  hashes unchanged. Both lack #4524's CTF `app_marker` edit, which is correct.

The record, retired (follow-up, 2026-11-23):
- `scripts/lib/zephyr-patchset.sh` loses the record's three functions
  (`nros_zephyr_patchset_stamp`, `_digest`, `_record`), and
  `nros_zephyr_apply_patchset` no longer deletes it before running a set.
- `patches/3.7.sh` and `patches/4.4.sh` record nothing as their last step;
  check mode still ends by saying every edit is present.
- `tests/zephyr-sync-workspace-manifest-tests.sh` row 11 no longer plants or
  asserts the file. It still asserts that the set runs once after an update,
  and not after a failed one. 27/27.
- The patches README, the sync script, and `check-west-update-patchset`'s
  docstring, remedy and `just` comment no longer describe a record.
- No gate expected it: `check-zephyr-pin-drift --self-test`,
  `check-west-update-patchset --self-test`/run and
  `check-zephyr-patch-check-mode` are green. A workspace provisioned earlier
  may still hold `.west/nros-patchset`. Nothing reads it.

Found on the way, not fixed here (filed as issue 2867):
- `nsos-adapt-ipproto-ip-patch.sh` (3.7) can never write. Its guard
  `grep -q "case NSOS_MID_IPPROTO_IP"` also matches `..._IPV6:`, which is in
  its own anchor, and stock 3.7 already carries the case. It is dead code.
- `cargo-features-patch.sh --self-test` fails at `main` ("patch FAILED on run
  1": its 3.7 fixture has no `CARGO_ARGS build` line, so the staticlib-only
  hunk refuses it). No lane runs it.
- `tcm-section-align-patch-4.4.sh` rewrites both files on every normal run,
  even when already applied: same content, new mtime. Check mode no longer
  does.
