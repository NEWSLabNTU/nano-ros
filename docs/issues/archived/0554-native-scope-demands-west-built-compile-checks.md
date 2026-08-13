---
id: 554
title: "`NROS_FIXTURE_SCOPE=native` demands four west-built compile-checks the native lane cannot produce, so tier 1 cannot reach a single test"
status: resolved
resolved_in: phase-350
type: bug
severity: high
area: build
related: [issue-0536, issue-0482, issue-0351, phase-350]
---

## Symptom

`just ci` (tier 1) dies at the staleness gate, before any test runs:

```
ERROR: 4 compile-check fixture(s) are missing or stale:
  west_bringup_zephyr     (missing build/compile-check/west_bringup_zephyr/.inputsig)
  zephyr_self_pkg_rust    (missing …/.inputsig)
  zephyr_self_pkg_sibling (missing …/.inputsig)
  west_board_import       (missing …/.inputsig)
  Run `just build-test-fixtures` before test-all.
error: recipe `_check-fixtures-stale` failed with exit code 1
```

Running `just build-test-fixtures lane=native` does not help, and cannot: the
native lane never builds these.

## Cause

`a12e2c3e4 feat(#536 / phase-350 W2): west fixtures declare their shape` added
four west `[[compile_check_fixture]]` rows. `check-fixtures-stale.sh` lists
compile-checks with no builder or lane filter:

```sh
done < <(python3 scripts/build/fixtures-manifest.py list-compile-checks 2>/dev/null)
```

so every scope demands every row. The manifest says outright who builds them:

> Built by the WEST lane (west-fixtures.sh), never by compile-check-fixtures.sh:
> west needs a provisioned Zephyr workspace, so the lane that owns one runs them.

That is issue 0482's distinction, missed for the compile-check inventory:
**which fixtures must be FRESH is the lane's cell cover, not every row in the
manifest.**

## Fix

Drop `west-*` rows when `SCOPE=native`. `all` (tier 3) and `coords` (tier 2)
keep demanding them — those lanes either build west or select by coordinate, and
silently dropping a west row there would hide a real staleness, which is the
failure mode this gate exists to prevent.

Verified in both directions, by counting what each branch actually passes to the
probe:

| scope | west rows demanded | total |
| --- | --- | --- |
| `native` | 0 | 36 |
| `all` | 4 | 40 |
| `coords` | 4 | 40 |

and end to end: `NROS_FIXTURE_SCOPE=native bash scripts/check-fixtures-stale.sh`
now exits 0 where it exited 1.

### The predicate is a PREFIX, and that was not obvious

There are TWO west builders, not one:

```
west_bringup_zephyr      west-build
west_board_import        west-configure
zephyr_self_pkg_rust     west-configure
zephyr_self_pkg_sibling  west-configure
```

My first version matched the literal `west-build`. It would have fixed ONE of
the four and left the other three failing identically — a fix that looks right,
passes review, and moves the error message by one line. Counting the rows rather
than reading the ids is what caught it.

Worse, the correction initially edited only the COMMENT: the `awk` kept the
literal while the prose above it claimed a prefix. The gate run caught that too.
Both are the same mistake in different clothes — verifying the change I meant to
make rather than the one on disk.

## Reproduce

```sh
source ./activate.sh
NROS_FIXTURE_SCOPE=native bash scripts/check-fixtures-stale.sh
```

Before: exit 1, naming the four. After: exit 0.

## Note

`SCOPE=all` cannot be verified end to end on a host without the embedded
fixtures built — it fails earlier, at the rust probe ("6 rust fixture(s) could
NOT be built"), and never reaches the compile-check section. The table above
verifies that branch by counting the records it passes, which does not depend on
having built them.
