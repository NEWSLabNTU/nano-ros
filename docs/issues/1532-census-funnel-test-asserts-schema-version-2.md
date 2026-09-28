---
id: 1532
title: "`census_out_makes_the_funnel_dump_and_exit` asserts schema `version:2`
  while the emitter has been on 3 since phase-457 W3 — red on `main`, and the
  gate that would catch it filters this test out"
status: open
type: bug
area: [api, testing]
severity: medium
found: 2026-09-28
related: [phase-457, phase-463, 1226]
---

## Symptom

```
packages/api/nros-cpp/src/lib.rs:5658
    assert!(json.contains("\"version\":2"), "schema v2: {json}");
```

The producer emits 3:

```
packages/api/nros/src/node_metadata.rs:49
pub const SOURCE_METADATA_SCHEMA_VERSION: u32 = 3;
```

So `nros-cpp`'s lib test `census_funnel_tests::census_out_makes_the_funnel_dump_and_exit`
fails on `main`. Reproduce with:

```sh
cargo test -p nros-cpp --lib --no-default-features \
  --features std,rmw-cffi,metadata-mode,param-services
```

## Cause

`7376f12f7` (phase-457 W3, 2026-09-28 00:25 UTC) moved the constant to 3 and did
not move the test's literal. `git merge-base --is-ancestor 7376f12f7 origin/main`
confirms it is on `main`, so this is not a branch artifact.

## Why nothing caught it

`check-census-hooks` filters on `-- census_fixture`, so the funnel test is not in
its set. That is issue 1226's shape: a test that works but that no lane runs.

## Fix

Assert against the constant rather than a literal — a test that hardcodes a
schema version is a second producer of that number, which is what drifted. If
the literal is deliberate (pinning the wire format a consumer reads), it needs a
comment saying which consumer and a paired update rule.

Then decide whether `check-census-hooks`'s filter should widen to cover the
funnel, since the gate exists to answer exactly this question.

## Found by

A phase-456 ledger-gap change, which ran the crate's lib tests and got
39 passed / 1 failed. The failure is unrelated to that diff.
