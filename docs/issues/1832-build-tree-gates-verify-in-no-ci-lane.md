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

- `artifact-identity-budget` reads the `mixed` workspace's fixture build dir.
  The tier-1 lane builds it (the native `linux`/zenoh workspace rows), but the
  gate ran in no job after that build: host-tests' gate job builds nothing.
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

`check fast` stays buildless, so the gates' rows in
`.config/ci-check-fast-skips.txt` stay too; their reasons now name the lane
that verifies them instead.

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
- **`artifact-identity-budget` — gate side fixed (2026-10-15).** The
  tier-1 post-build step ran it for the first time in run 38663782341, after a
  complete build, and it SKIPPED: "no build tree at
  examples/workspaces/mixed/build-workspace-fixtures". Issue 0901's fix
  resolved the newest `examples/workspaces/*/build-workspace-fixtures*` tree,
  but `nros build` now builds a workspace into `<ws>/build/<image>/…`, so no
  such dir is written. Discovery now also considers
  `examples/workspaces/mixed/build`, newest tree with rlibs first, in a
  `_resolve_tree` function the self-test exercises (case (d): the old layout
  alone, an rlib-less dir never chosen, the newer current-layout tree wins).
  Measured on a native-lane build of the mixed workspace, the gate reads
  exactly the recorded budget: `nros_core` 4/4 identities, worst crate 5/5,
  worst identity 5/5 copies. What is left is the lane reading `OK`.
- **All three gates now run after the tier-1 build.** `run-matrix.yml`'s
  tier-1 job has a `Build-tree gates (issue 1832)` step right after
  `just build tier1`. It resets the skip ledger, runs `artifact-identity-budget`,
  `sizes-header-mirrors` and the FULL `zephyr-workspace-foreign-checkout` (its
  `--manifest-only` half already ran before the build), and fails if any of
  them recorded a skip. After a real build each has its input, so a skip there
  means a lost input.
- **`zephyr-workspace-foreign-checkout`'s rule was never unverified in CI.**
  Every invocation runs its self-test first, which builds synthetic own and
  foreign checkouts and workspaces and requires the foreign ones to fail. The
  check-fast skip is only about the SUBJECT: a fresh CI checkout has no Zephyr
  workspace.

## Closes when

The tier-1 lane's `Build-tree gates` step has run green once. Until then the
step is in place but unproven: on 2026-10-11 the tier-1 lane stopped in
provisioning and never reached its build (phase-466).
