---
id: 1349
title: "a feature-bearing fixture selector aimed at a crate with no `[features]`
  table matches nothing, and the cell reads as live failures"
status: resolved
type: bug
area: testing
related: [issue-0743, phase-433, phase-455]
---

## The defect, measured twice

`FixtureVariant::rmw(x)` and `FixtureVariant::features([..])` select a
`[[fixture]]` row by its cargo arguments — `features = ["rmw-<x>"]`,
`no_default_features = true`. A crate with no `[features]` table in its
`Cargo.toml` can never have such a row: cargo refuses the build outright (`the
package 'X' does not contain this feature`), so the row is never written, so the
selector matches nothing.

**What that costs is not a red.** `select_row` reports

```
no [[fixture]] row for <dir> with Selector { rmw: "zenoh",
features: "rmw-zenoh", no_default_features: true }
```

which reads as a MISSING ROW. So the next reader adds the row, cargo refuses it,
and the reader concludes the fixture is unbuildable. Meanwhile every case that
resolver feeds resolves to `[SKIPPED] fixture not built`, and a `skip!` is a
panic that bare `cargo nextest` renders FAILED — so the cell reads as live
failures against a peer it never reached.

Two occurrences, in one file, twenty lines apart:

| site | found | cell | after the fix |
| --- | --- | --- | --- |
| `build_advertised_state_probe` | phase-433, 2026-09-08, on its first run | `native-advertised-state-rust-cyclone-bidir`, never a verdict | passed 4/4 live the same day |
| `build_qos_event_probe` | phase-455, 2026-09-12, on its first run | `native-qos-event-rust-zenoh-r2n`, never a verdict | passed live the same day |

Both were corrected to a selector that authors no `features` key —
`select_sole_row` for the first, `FixtureVariant::platform_rmw` for the second.

## Why it is filed after both fixes

Two occurrences of one mistake is a class, and CLAUDE.md's rule is to fix the
class rather than the second site. The first fix landed with a careful comment
explaining itself, in the same file, twenty lines above where the second one was
then written — which is the evidence that a comment is not a gate.

## The sweep

Run by hand 2026-09-12 over the nine crates a feature-bearing selector names.
Exactly two lack the table, and both are the sites above:

```bash
for d in packages/testing/nros-tests/bins/*/; do
    grep -q '^\[features\]' "$d/Cargo.toml" || echo "$d has no [features]"
done
```

## Fix

`scripts/check-fixture-variant-features.py`, `just check fixture-variant-features`,
on the fast line (buildless, source-only: two Rust call sites and a TOML table).
It refuses a `select_row` whose `FixtureVariant` carries cargo features when the
named crate has no `[features]` table, and a `FixtureVariant::features` naming a
feature the crate does not declare.

Negative control, both directions, on the real tree: against `main` before
PR #1003 it reports `build_qos_event_probe` and exits 1; against PR #1003's
resolver it reports `OK (9 feature-bearing selector(s) checked)`. The self-test
plants five cases, including the two legal shapes (`platform_rmw`,
`select_sole_row`) that must NOT be matched.

**The vacuity guard is the part that matters most.** A regex over Rust is
exactly the thing that stops matching silently, and a gate that scans nothing
prints the same `OK` as a gate that scanned everything. Zero feature-bearing
selectors in the resolver is therefore a FAILURE — the pattern drifted, not the
tree.
