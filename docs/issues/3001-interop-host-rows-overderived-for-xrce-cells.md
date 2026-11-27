---
id: 3001
title: "The live-peer host lane builds 97 workspace rows and its last green run used 39; the coordinate rule alone adds 47, of which 6 are used"
status: open
type: bug
severity: medium
area: [ci, testing, interop]
found: 2026-11-27
related: [2904, 1127]
---

## What happens

Live-peer's host job builds only the workspace rows derived from the
recorded cells' tests (`check-interop-verdicts.py --list-passing --runner host
--host-workspace-fixtures`). Release blocker B16 capped that set at 100 rows,
because each row is a from-scratch build inside a 350-minute job. It is 97
today.

Recording the six lifecycle-pair cells, which pass live (issue 2904), would
take it to 116. Twelve of the 19 new rows come from the two XRCE cells and are
`workspace-{c,cpp}-native-{talker,listener,service-client,service-server,action-client,action-server}-xrce`.
`lifecycle_pair_interop` launches none of them.

## Measured

The evidence ledger of the last green live-peer run (45868139877, main
`52fcb58126`, artifact `live-peer-junit-host`) names every workspace row a
test resolved. Against the set derived from the same ledger:

| rule | derives | used |
| --- | --- | --- |
| a literal row id in the test or a helper | 14 | 14 |
| by coordinate: every Linux pair image of the cell's (lang, rmw) | 53 | 12 |
| a migrated leaf's directory, named | 36 | 19 |
| `linux_pair_image(...)` calls, named | 6 | 6 |
| **all** | **97** | **39** |

No resolved row was outside the derived set, so the lane is safe. It is
2.5 times larger than what it runs.

- **Coordinate rule:** 47 rows come from it alone, and 6 of those were used.
- **Leaf-name rule:** 30 rows come from it alone, and 13 were used.

## Why the coordinate rule is there

It stands in for the resolvers the name rule cannot read. The six used rows
it alone supplies are reached as:

- `linux_pair_image("c", case, FixtureRmw::Cyclonedds)` in `interop_e2e`.
  `_PAIR_CALL` accepts only an `Rmw::` path in the third argument, so a
  `FixtureRmw::` spelling falls through.
- `build_native_rust_example_rmw("action-client", "action-client", Rmw::Cyclonedds)`
  in `ros2_action_e2e`.
- rstest fixture arguments (`xrce_listener_binary`,
  `xrce_service_server_binary`, ...) in `xrce_ros2_interop`, defined in
  `nros_tests::fixtures`.

## What a fix needs

- Read those three resolver shapes by name, then retire the coordinate rule,
  as B16 already did for migrated leaves.
- A test that, for each recorded host test, the derived rows are a superset
  of the rows a committed evidence snapshot says it resolved, and that what
  each lifecycle-pair cell derives is exactly the six pair rows it launches.
- The ceiling then drops to what is used. Recording the lifecycle-pair cells
  adds 6 rows, not 19.

`--check-resolved` stays the backstop: a row a run resolves that was not
derived is refused after the run, never dropped silently.

## Fix (draft, held until after the nros12 dispatch)

`check-interop-verdicts.py`:

- The coordinate rule is gone. Pair images are derived only from resolver
  calls: `linux_pair_image(...)`, `build_native_<lang>_example_rmw(...)` and
  `build_example[_cmake]_rmw("linux/<lang>/<role>", ...)`. A qualified RMW path
  (`fixtures::Rmw::X`, `FixtureRmw::X`) and a trailing comma are accepted.
- The pair rule follows helpers transitively, as the leaf rule already did.
- The call graph is now call sites only: `name(` and not `.name(`, plus
  rstest `name: PathBuf` fixture parameters. It used every identifier, so
  helpers named `stop`, `create`, `value` or `main` linked every test to the
  whole fixture resolver.
- Whole-line comments are dropped before scanning, so an example directory
  named in a doc comment is not a resolve.

Against the evidence of three green runs (45868139877, 45820902343,
45753606904):

| | before | after |
| --- | --- | --- |
| rows derived | 97 | 58 |
| rows resolved that are derived | 39 of 39 | 39 of 39 |
| rows dropped | | 40, none of them resolved in any of the three runs |
| rows added | | 1, `workspace-c-native-talker-xrce`: `interop_e2e` calls `build_example_cmake_rmw("linux/c/talker", "c_talker", rmw)` with a variable RMW, which is a wildcard |
| each lifecycle-pair cell derives | 19 rows | its 6 rows |
| all 8 never-recorded cells recorded | 116 | 65 |

`--host-builds` (5) and `--host-compile-checks` (3) are unchanged.

The self-test now holds the derivation to
`.config/live-peer-resolved-rows.toml`: what each host test binary resolved in
those three runs. Every row must still be derived, per binary for a cell's
own test. Each lifecycle-pair cell must derive exactly its six rows. The
ceiling drops from 100 to 70.

Still to measure after it lands: the live-peer fixture step's time on its
first scheduled run.
