---
id: 1226
title: "`just ci gate` is red on `main`: a `fixtures.toml` workspace row for zephyr-native-sim / cpp / cyclonedds has no `matrix::CELLS` cell"
status: open
area: testing, ci
severity: medium
found: 2026-09-08
related: [0981, 0952, phase-206]
---

# The last step of the local push lane fails, and it is nobody's diff

`just ci gate` step 6 of 6, `test-lane-contracts`:

```
FAIL nros-tests::matrix_fixture_coverage fixture_rows_all_modeled_by_matrix

fixtures.toml coordinates outside the matrix (model them or delete the rows):
orphan (platform_idx, lang_idx, rmw_idx, is_ws): [(1, 2, 1, true)]
unmapped rows: []
```

Decoded against `packages/testing/nros-tests/src/matrix.rs`:

| field | value | source |
| --- | --- | --- |
| `platform_idx` 1 | `PlatformId::ZephyrNativeSim` | `:93` |
| `lang_idx` 2 | `Lang::Cpp` | `:307` |
| `rmw_idx` 1 | `Rmw::Cyclonedds` | `:282` |
| `is_ws` true | `Kind::Workspace` | |

So `examples/fixtures.toml` carries a WORKSPACE row at
(zephyr-native-sim, cpp, cyclonedds) that `matrix::CELLS` does not model.
`fixture_rows_all_modeled_by_matrix` is the REVERSE direction the matrix gate
added at W3-end — "an unmodeled row is either debt the table must model or an
orphan to delete" — and it is the one firing.

## Not from the branch that found it

Bisected by inputs rather than by revision, which is sound here because the
test is a pure function of three files:

* `examples/fixtures.toml`
* `packages/testing/nros-tests/src/matrix.rs` (`CELLS` + the index tables)
* `scripts/build/fixtures-manifest.py` (the `coords` subcommand)

The most recent commit touching any of them is
`9ae271174 feat(phase-206 W2): the RTOS image that links Cyclone AND carries a
bringup config` (2026-09-06), and it is an ancestor of every branch that has
since reported this. Reproduced on `docs/rfc-0094-resolve-before-configure`
with and without `NROS_REPO_ROOT` set, on a tree whose diff touches none of the
three.

## Why it survived

`test-lane-contracts` is not in the required `CI` context on a pull request
(that is `check-fast` + `check-submodule-commits-reachable` +
`check-compile-smoke` + `check-cli-tests`), so nothing between the commit and a
merge asks this question. It is asked by `just ci gate`, which is the lane
contributors are told to run before every push — and because that lane stops at
the FIRST failure, a red here also withdraws nothing, being last. What it does
cost is the signal: every contributor who runs the documented lane now sees a
red they must triage as not-theirs before they can trust the green above it,
which is issue 0952's warning read from the other end.

This is the fourth pre-existing red on `main` this week, after the two
`docs/rfc-0094-resolve-before-configure` fixes and the `cli-clippy` dead-code
one.

## Fix direction (not applied)

Two candidates, and the choice belongs to whoever owns phase-206 W2:

* **model the cell** — add
  `cell(ZephyrNativeSim, Cpp, Cyclonedds, …, Workspace, …)` to `matrix::CELLS`,
  if the image the row builds is meant to produce a runtime verdict;
* **delete the row** — if the fixture is a build-only artifact that was never
  meant to carry a cell.

Not guessed here: the gate's own message says "model them or delete the rows",
and picking wrong turns a missing cell into a permanently-skipped one, which is
the shape issue 1127 is already about.
