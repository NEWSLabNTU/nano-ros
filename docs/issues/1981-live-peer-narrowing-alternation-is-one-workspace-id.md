---
id: 1981
title: "live-peer joins two NROS_FIXTURE_ID narrowings with `|`, and the workspace builder looks the alternation up as ONE id"
status: open
type: bug
area: [ci, testing, build]
severity: medium
found: 2026-10-19
related: [0997, 1758, 1656, 1961, phase-441]
---

## Measured

- Run: live-peer 39198739349 (schedule, 2026-10-19 04:17 UTC, head `d5fe58a51`).
- Job: 117887475135, `rows whose board is NOT this runner (ci image)`. Step
  `just build freertos` (`recipe build-examples failed with exit code 2`).
- Narrowing the job was handed, and what the workspace builder made of it:

```
narrowing: NROS_FIXTURE_ID=workspace-c-freertos|workspace-cpp-mps3-an536-freertos-slirp
fixtures: no row anywhere carries id 'workspace-c-freertos|workspace-cpp-mps3-an536-freertos-slirp'.
          Searched every table in examples/fixtures.toml:
          [[fixture]], [[workspace_fixture]], [[compile_check_fixture]].
```

- Yesterday's run, 39096469491 (job 117584129206), was green. Its narrowing
  held one id: `narrowing: NROS_FIXTURE_ID=workspace-c-freertos`.

## Cause

`narrowing_for` in `scripts/check-interop-verdicts.py` merges every recorded
board cell that shares a narrowing key and joins the values with `|`. Its own
docstring gives the reason, and the condition under which it is wrong:

> the one narrowing hook in the tree (`NROS_ZEPHYR_FIXTURE_FILTER`) is a REGEX,
> so an alternation is its union. A future key whose union is not an
> alternation must say so here rather than inherit this one's meaning.

`NROS_FIXTURE_ID` is that future key. Its first row
(`freertos-mps2-pubsub-c-zenoh-n2r`) never had a partner until
phase-441 W6 added `freertos-an536-pubsub-cpp-cyclone-n2r`, and #2386 (issue 0997) recorded its PASS,
which narrows the same key. The two values then merged into
`workspace-c-freertos|workspace-cpp-mps3-an536-freertos-slirp`.
`scripts/build/workspace-fixtures-build.sh` reads `NROS_FIXTURE_ID` as one
exact id (`id_filter="${NROS_FIXTURE_ID:-}"`), and `fixture-id-guard.sh` then
fails it as a typo, which is its exit 2.

The single-node stage of the same job already prints that it does not narrow
on this variable and builds its 9 rows, so only the workspace stage refuses.

## What it is NOT

- Not issue 1961's errexit change (#2393). The refusal comes from the id
  guard, before any row runs.
- Not a missing fixture: both ids exist in `examples/fixtures.toml`.
- Not this runner's image: the step fails before anything is built.

## What would close it

Two sites are defensible, and the fix has to pick one.

1. **The consumer takes a set.** `workspace-fixtures-build.sh`, and the guard
   it calls, accept several ids. `compile-check-fixtures.sh` already does
   this as `NROS_FIXTURE_IDS=<id>[,<id>…]` (issue 1656), so reuse that
   spelling. `narrowing_for` then emits that key, comma-joined, for every row
   it now writes as `NROS_FIXTURE_ID`.
2. **The producer refuses to merge a non-regex key.** `narrowing_for` keeps
   a per-key join rule and raises `LedgerError` when two cells need different
   values of a key that names exactly one row. The job would then run once
   per id.

Either way, a check should cover the case that broke: two recorded cells
that share an `NROS_FIXTURE_ID` narrowing. The ledger's self-test currently
holds at most one such cell per key.
