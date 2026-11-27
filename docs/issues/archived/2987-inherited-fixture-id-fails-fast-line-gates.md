---
id: 2987
title: "An inherited NROS_FIXTURE_ID narrows the Zephyr leaf inventory that four fast-line gates read, so the documented narrowing fails its own check-fast edge"
status: resolved
type: bug
severity: low
area: [gates, fixtures, build]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-2987-fixture-id-gates"
related: [0406, 0986, 2835, 1325, 1536]
---

## What happened

`NROS_FIXTURE_ID=freertos-isr-wake-mps2 just build-test-fixtures lane=all`
failed at its first edge, `_check-fast-unless-green-gate`. `check fast` runs in
the caller's environment, and with the variable unset the same tree's
`check fast` was 670/670. Run alone on that tree, unset and then set, four
gates differed:

| gate | failures unset | failures set |
| --- | --- | --- |
| `fixture-filter-fields` | 0 | 3 |
| `kconfig-overridden-values` | 0 | 1 |
| `zephyr-fixture-id` | 0 | 3 |
| `fixtures-manifest` | 0 | 2 |

Each failed with the same line:

    zephyr-fixture-leaves: no records matched: NROS_FIXTURE_ID=freertos-isr-wake-mps2

As a result, the sweep-wide narrowing that `workspace-fixtures-build.sh`
documents (issue 0406) could not go through `build-test-fixtures`; only a
builder's own `--id` flag worked.

## Cause

`scripts/build/zephyr-fixture-leaves.sh` has two kinds of caller:

- the Zephyr build lane (`just/zephyr-ci.just` and the make driver it hands
  the same arguments to), which means to narrow;
- the gates, which read it as the WHOLE-manifest inventory of west leaves.

Since issue 2835 the emitter narrowed on `NROS_FIXTURE_ID` by itself, and it
had always defaulted `--filter` from `NROS_ZEPHYR_FIXTURE_FILTER`. A gate
cannot tell an inventory narrowed by its environment from a real one, so an
inherited variable reached every gate through the environment. The id that
selects one row, or one with no west leaf, left the inventory with one leaf or
none. `NROS_ZEPHYR_FIXTURE_FILTER` is the same class: measured, an inherited
filter that matches no leaf turns the emitter's whole-manifest output into
exit 3 and no records.

This is issue 0986's class (an inherited `GIT_DIR`) one variable over: a
setting meant for one command reaches every command run beneath it.

## Fix

The emitter reads neither variable from the environment. The narrowings are
flags, `--filter` and a new `--fixture-id`, and the Zephyr lane, the one
caller that means to narrow, reads both variables and passes them. So a gate
gets the whole manifest whatever its caller exported, and
`NROS_FIXTURE_ID=<id> just zephyr build-fixtures` narrows as before.

Verification:

- **Negative controls** in `tests/zephyr-fixture-id-tests.sh` (the
  `zephyr-fixture-id` gate) require byte-identical whole-manifest output with
  `NROS_FIXTURE_ID` set (to a real row, and to a row with no leaf),
  `NROS_ZEPHYR_FIXTURE_FILTER` set, and both set, against neither. On the old
  emitter all four fail; on the new one they pass. The test's narrowing cases
  use the flag.
- **The four gates** give rc=0 with `NROS_FIXTURE_ID` set, the same as unset.
  On the old emitter, `kconfig-overridden-values` with it set gives rc=1 and
  "no records matched".
- **The sweep:** the whole fast line, run with each variable inherited, passes
  670/670 with `NROS_FIXTURE_ID=workspace-zephyr-c-realtime-ctf` and 670/670
  with `NROS_ZEPHYR_FIXTURE_FILTER=no-leaf-matches-this`, the same as with
  neither. No other gate reaches an environment-narrowed inventory. The
  builders that read `NROS_FIXTURE_ID` (`compile-check-fixtures.sh`,
  `workspace-fixtures-build.sh`, `west-fixtures.sh`, `fixtures-build.sh`) do
  so on purpose: there it is the narrowing a person asked for.
