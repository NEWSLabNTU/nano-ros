---
id: 1832
title: "`artifact-identity-budget` and `sizes-header-mirrors` give a verdict in no CI lane, not even the ones that build fixtures"
status: open
type: test-gap
severity: low
area: [ci, testing]
related: [0650, 1226]
found: 2026-10-12
---

## Measured

Both gates read a BUILD tree, so in `check fast` (buildless by contract) they
report NOT VERIFIED, and CI's `check` job allowlists them for that reason
(`.config/ci-check-fast-skips.txt`). The allowlist assumes a lane with a build
tree verifies them instead. None does. The latest `host-tests` run, which builds
fixtures, skipped both:

```
[SKIPPED] artifact-identity-budget: NOT VERIFIED — no build tree at examples/workspaces/mixed/build-workspace-fixtures
[SKIPPED] sizes-header-mirrors: NOT VERIFIED — 0 built mirror(s) across 0 build dir(s); 15 Makefile-generator build dir(s) not compared
```

- `artifact-identity-budget` reads the `mixed` workspace's fixture build dir,
  which the tier-1 lane does not build.
- `sizes-header-mirrors` compares Ninja build dirs only. The CI lanes configure
  with the Makefile generator, so it finds 15 build dirs and compares none of
  them.

So each verifies only on a developer host that happens to hold the right build
tree. That is issue 1226's shape: a gate that works is not a gate that runs.

## Fix

For each gate, either:

- make a lane that already builds the input run the gate after the build: the
  `mixed` workspace fixture for the first, Ninja build dirs (or Makefile support
  in the gate) for the second; or
- record why no lane can, and drop the gate's claim to be a merge guard.

Then delete the gate's row from `.config/ci-check-fast-skips.txt`; the
allowlist script fails the job until it is gone.

## Progress

- **`sizes-header-mirrors` — gate side fixed.** It now derives the
  `(mirror, source)` pairs from a Makefile build dir too: the same
  `mirror-generated-header.sh` command line, read from every
  `CMakeFiles/<target>.dir/build.make` (nested subproject dirs included), fed
  to the one comparison. Self-test has a synthetic Makefile dir (current pair
  compared, drifted pair fails, `--fix` re-mirrors, mutation of the make
  derivation fails it). Measured on real `build/cmake-fixtures/*` dirs (the
  layout the CI log's 15 dirs are): a 16-dir host tree compared 29 pairs, 0
  drift; a hand-drifted `nros_cpp_config_generated.h` failed the gate. Its
  `ci-check-fast-skips.txt` row stays: `check fast` is still buildless. What
  is left is confirming the fixture-building lane's log now reads `OK — N
  pair(s)` rather than NOT VERIFIED.
- `artifact-identity-budget` — unchanged, still open.
