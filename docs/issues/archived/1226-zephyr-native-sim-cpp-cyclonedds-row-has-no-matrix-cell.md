---
id: 1226
title: "`just ci gate` is red on `main`: a `fixtures.toml` workspace row for zephyr-native-sim / cpp / cyclonedds has no `matrix::CELLS` cell"
status: resolved
area: testing, ci
severity: medium
found: 2026-09-08
resolved: 2026-09-08
related: [0981, 0952, 1040, 1072, phase-206]
---

## Resolution — the site was already fixed; the CLASS was the placement

**The site.** `cell(ZephyrNativeSim, Cpp, Cyclonedds, EntryPubsub, Workspace,
Runtime)` was the right answer, and it had already landed by the time this issue
was picked up: `e4940be3e` (2026-09-08 01:57, `fix(#1131)`) added it as a
side-fix inside an unrelated pull request. Measured at `7b24d8228`:
`just test-lane-contracts` is **17/17 green**, and removing that one line
reproduces this issue's report byte for byte —
`orphan (platform_idx, lang_idx, rmw_idx, is_ws): [(1, 2, 1, true)]`.

Which side was wrong was decided by reading `9ae271174`, not by guessing. Its
own acceptance is a RUNTIME measurement on `zephyr.elf` (`strings | grep -c
ddsi_` = 14283, the SSoT bytes contiguous in the ELF), and the row exists
precisely *because* the sibling compile-stage row
(`west_bringup_zephyr_cyclone_user_config`) could not produce one — the commit
message says so at length. A row whose whole purpose is a runtime verdict is
modeled, never deleted.

**The class.** The gate for "a fixtures row has a cell" existed and worked:
`fixture_rows_all_modeled_by_matrix` fired immediately and correctly. What did
not exist was anywhere for it to fire. `test-lane-contracts` — the lane that
owns it — ran in **NO workflow, on any event**, so nothing between
`9ae271174` and its merge asked the question.

`check-default-gates-run-somewhere.py` (issue 1040) is the gate for exactly that
rule — "every gate must be reached by SOME workflow event" — but its SCOPE was
`just check` names, which is narrower than the rule it enforces (CLAUDE.md's
2026-07-28 audit shape, four gates at once). `just ci gate` has two steps that
are not `check::` names, `test-unit` and `test-lane-contracts`, and one of the
two was unheard. R1's scope now covers the `just ci gate` step list, READ from
the lane's own `steps=(…)` array so it cannot go stale in the safe-looking
direction; `test-lane-contracts` is placed in `gate.yml` on
`merge_group`/`schedule`/`workflow_dispatch`, beside `test-unit` and under the
same "PR cheap, batch thorough" split.

**Not G1–G4.** Those are the interop gates (`interop::CELLS`: test coverage,
build-coord match, tier/recipe, peer declaration, lane-narrowability). They have
nothing to say about the `fixtures.toml` ↔ `matrix::CELLS` cross-check, so there
was never a reason to expect them to catch this.

**And the message.** Decoding four integers against three index tables in
`matrix.rs` — the table above — is work this issue should not have had to do, on
a red every contributor meets in turn. The assertion now names the cell to write
and the row that wants it:

```
fixtures.toml coordinates outside the matrix — model them in `matrix::CELLS` or delete the rows:
  cell(ZephyrNativeSim, Cpp, Cyclonedds, Workspace, …)  <- fixtures.toml row(s): {"workspace-zephyr-cpp-cyclonedds"}
```

Mutations, each red naming the exact site, then restored and re-run green:
the cell deleted (reproduces the original report, then the new message);
the `gate.yml` step deleted (`check-default-gates-run-somewhere` names
`just test-lane-contracts`); a new step added to the `ci gate` lane
(`fixture-staleness`, named as unheard — the step list is read, not authored).

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
