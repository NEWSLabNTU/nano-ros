---
id: 2604
title: "run-bootstrap-probe.sh reads the pinned version from the working tree and the published version from PROBE_BRANCH, so a PROBE_BRANCH other than HEAD mixes two releases"
status: open
type: bug
severity: medium
area: [testing, release]
related: [issue-2168]
found: 2026-11-15
---

# One probe, two sources for its versions

## Symptom

After the index moved to `0.6.0-nros10` (#3929), `just probe pinned-newer` run
with `PROBE_BRANCH=main` from a worktree detached at `origin/main` failed:

```
probe: installed track — using the published release 0.6.0-nros8
PROBE FAIL: ~/.nros/bin/nros fronts /root/.nros/sdk/nros/0.6.0-nros9/bin/nros, not the newest installed release (0.6.0-nros8)
```

nros9 IS newer than nros8, so the message is self-contradictory. The same
probe on a branch at `origin/main` with no `PROBE_BRANCH` passed
(`installed 0.6.0-nros10 -> project pinned to 0.6.0-nros9`).

## Cause

`scripts/probe/run-bootstrap-probe.sh` derives the track's two versions from
two different trees:

- the PINNED version (`PROBE_PINNED_VERSION`) from the WORKING TREE, through
  `scripts/lib/pinned_example.py`, which reads `nros-sdk-index.toml` on disk;
- the PUBLISHED version (`nros_published_release_version`, which also becomes
  `PROBE_EXPECT_FRONT` and picks the release asset to download) from
  `git show "$under_test:nros-sdk-index.toml"`, where `under_test` is
  `${PROBE_BRANCH:-HEAD}`.

`main` resolved to the shared git dir's LOCAL `main` (0d0972be7b, the main
checkout's stale branch, index `0.6.0-nros8`), while the working tree said
`0.6.0-nros10`. So the probe installed nros8, pinned the project to nros9
(N-1 of the working tree's nros10), and expected the nros8 it installed to be
the front: two releases' facts in one run. Nothing compares them.

## Fix (agreed shape)

Both versions come from ONE ref, `under_test`, and a `PROBE_BRANCH` whose
`[tool.nros] version` disagrees with the working tree's is refused, naming
both, before anything is installed. `pinned_example.py` already takes the
index text as an argument (`derive(index_text)`), so the pinned version can be
derived from the same `git show` the published one reads.

Test: the probe's preamble, with an index at `PROBE_BRANCH` that differs from
the working tree's, refuses and names both versions.
