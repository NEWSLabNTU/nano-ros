---
id: 2180
title: "Tier-2 nightly: `workspace-system-required` west road reads no image facts on the runner"
status: open
type: bug
severity: medium
area: [ci, cmake, zephyr]
related: [1261, 2160]
found: 2026-10-31
---

## Symptom

Nightly run 41171284748 (2026-10-31 05:07 UTC, `7787c0c347`), job
124449578453 `tier 2 nightly (pairwise cover)`, runner
`nano-ros-runner-newslab-118-server`. Step `just build tier2-nightly` failed in
its `check fast` prelude, so `just ci matrix-nightly` was skipped and no cell
ran:

```
check-fast (parallel): 1 of 492 gate(s) FAILED
===== FAIL (workspace-system-required, rc=1, 4392ms) =====
[FAIL] D: an unset NROS_FEATURES did not become the image's axes (rc=0)
-- RESULT=<undefined>
[FAIL] E: NROS_FEATURES omitting the declared `lifecycle` was accepted
[FAIL] E: the refusal does not name the missing axis and issue 1261:
-- RESULT=[param_services]
```

Cases A–C (the `nano_ros_workspace()` road) passed. The synthetic workspace's
`nros sync` succeeded, because the `D-G: nros sync … failed` message is absent.
So `nros_resolve_image_features()` returned without facts, and D's log has no
`image facts NOT read` line. Either `_nros_image_facts_file` found no CLI, or
`nros image-facts --if-present` answered empty ("not in a workspace").

The two previous tier-2 nightlies (41006780767 on `093456b7e2`, 41018247377 on
`1c81f56b75`) got past this gate and reached `test-all`. The regression window
is therefore `093456b7e2..7787c0c347`. Among the commits in that window,
`d361877f83` (#2160, `export PWD := justfile_directory()`) changes what every
recipe sees as its cwd. It is a suspect only, not a finding.

## What it is NOT

- It does not reproduce off the runner. `just check workspace-system-required`
  passes on `1b47c88f01` in a linked worktree of a clean clone, both with the
  inherited `NROS_*`/SDK variables cleared (issue 1280) and with them pointing
  at another checkout.
- It is not out-of-disk (1353), and it is not the earlier tier-2 nightly
  failures 2112/2113, which were in `test-all`.

## What would close it

Re-run the gate on the runner's `nros-fixture-tree` and keep
`$TEST_TMPDIR/d.log` plus the `nros image-facts --for-entry zephyr_entry
--workspace <app> --cmake --if-present` output. The fix belongs wherever that
query diverges: CLI resolution in `_nros_image_facts_file`, or the CLI's
workspace walk-up from the gate-scratch path. The acceptance test is a tier-2
nightly reaching `ci matrix-nightly`.
