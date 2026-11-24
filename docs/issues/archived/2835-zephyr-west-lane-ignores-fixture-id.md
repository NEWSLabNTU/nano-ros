---
id: 2835
title: "`NROS_FIXTURE_ID=<id> just zephyr build-fixtures` builds the whole Zephyr family: the west lane never reads the id, though the refusal hint says to use it"
status: resolved
type: bug
severity: low
area: [zephyr, fixtures, ux]
related: [issue-0406, issue-1325]
found: 2026-11-22
resolved: 2026-11-24
resolved_in: "the west lane honours NROS_FIXTURE_ID (#4614): one leaf per id through the manifest's `west-leaf-id`, an id with no west leaf refused"
---

## What happens

A workspace fixture builder that is handed a row it does not build refuses
with a copy-paste hint (`nros_fixture_id_builder_hint`,
`scripts/build/fixture-id-guard.sh`):

    NROS_FIXTURE_ID=<id> just <platform> build-fixtures

That header documents `NROS_FIXTURE_ID` as the sweep-wide narrowing "read by
every stage a platform recipe runs". The Zephyr west stage does not read it.
`just zephyr build-fixtures` narrows only on `NROS_ZEPHYR_FIXTURE_FILTER`
(`just/zephyr-ci.just`, matched per field by `scripts/lib/fixture-filter.sh`),
so the hint builds every Zephyr leaf.

Measured on host 241, 2026-11-22: for the 2766 acceptance cell, the
`workspace-zephyr-c-realtime-ctf` row was asked for, and the whole family was
built. The family's run failed on an unrelated leaf
(`build-ws-cpp-params-entry-zenoh`) before the CTF leaf was built, so the cell
read UNMET. Building only the leaf that was wanted would have answered it.

## Workaround

`NROS_ZEPHYR_FIXTURE_FILTER='build-ws-c-realtime-ctf-zenoh$' just zephyr build-fixtures`,
i.e. the row's `west_build_name`.

## Fix direction

The west lane honours `NROS_FIXTURE_ID` the way the other stages do. It
resolves the id to its row's `west_build_name` through the manifest (one
reader, never a second TOML parse), and narrows to exactly that leaf when
`NROS_ZEPHYR_FIXTURE_FILTER` is unset. It refuses an id that names no
Zephyr west row, rather than building everything. The hint then stays true
for every platform, and the narrowing contract in `fixture-id-guard.sh`'s
header holds for the stage that broke it.

The record `west-leaves` emits is 16 fields wide and has many readers, so
the id → build-name answer gets its own manifest command, not a new field.


## Fix (2026-11-22, merged in #4614)

- `scripts/lib/fixture-filter.sh` gains `nros_fixture_id_selects` and
  `nros_fixture_narrowing`, one spelling for both Zephyr builders.
- The leaf emitter (`zephyr-fixture-leaves.sh`) resolves the row id once,
  through a new manifest command `west-leaf-id --id <id>`, and keeps only that
  leaf. A record carries the leaf id (`west_id`), never the row id, so the
  mapping is its own command, not a 17th field. An id with no west leaf
  selects none and exits 3, the existing "matched nothing" answer.
- `west-fixtures.sh` (the west compile-check rows, whose ids ARE row ids) keeps
  only that row and exits 3 when none matches.
- `just zephyr build-fixtures` treats an id like the filter: exit 3 from one
  half is benign, and from both halves the lane fails, naming
  `NROS_FIXTURE_ID`. It also prints the id it narrowed to.

Only `[[workspace_fixture]]` rows carry an `id`. The nine `[[fixture]]` west
rows have none, so `NROS_FIXTURE_ID` cannot name them; the filter still can.

Test, `just check zephyr-fixture-id` (`tests/zephyr-fixture-id-tests.sh`,
records mode, nothing built):

- unset selects every leaf (106);
- two workspace rows of different shapes each select exactly their one leaf;
- a bogus id selects none, exits 3 and names the id;
- `west-leaf-id` refuses an id no west row has.

### The sweep: other platform stages

Each platform's `build-fixtures` dependency chain was read:
- **FreeRTOS, NuttX, ThreadX (linux and riscv64), QEMU bare-metal, Linux:**
  every fixture build goes through `fixtures-build.sh`,
  `workspace-fixtures-build.sh` or `compile-check-fixtures.sh`, and all three
  honour `NROS_FIXTURE_ID`. The direct leaf builds in the ThreadX justfiles
  are in their `_run*` and `doctor` recipes, not `build-fixtures`.
- **PX4 is the one remaining gap.** `just px4 build-fixtures` builds its
  three XRCE companions in its own loop. They have no manifest row and so no
  id, which means `NROS_FIXTURE_ID` names nothing there. Honouring it would
  need rows for them first. Not changed here.

## Resolution (2026-11-24)

The fix above merged in #4614 (047f909d20, 2026-11-22), with its test
`just check zephyr-fixture-id` (`tests/zephyr-fixture-id-tests.sh`). That
test runs on main's checks, so the hint `NROS_FIXTURE_ID=<id> just zephyr
build-fixtures` now builds exactly the leaf it names. The issue was never
archived when the fix landed; archived now, with no live run needed (manager,
2026-11-24). PX4's companions, which have no manifest rows, stay as noted
above.

