---
id: 1532
title: "`census_out_makes_the_funnel_dump_and_exit` asserts schema `version:2`
  while the emitter has been on 3 since phase-457 W3 — red on `main`, and the
  gate that would catch it filters this test out"
status: resolved
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

## Fix — 2026-09-28

**1. The test asserts against the constant.** It read `"version":2`, which made
it a second producer of a number only the emitter gets to decide, and it drifted
the moment the emitter moved. It now formats
`nros::node_metadata::SOURCE_METADATA_SCHEMA_VERSION` into the expected string,
so a future bump cannot make this test wrong — it can only make it fail for a
reason that is about the document.

**2. The gate's filter widened, `census_fixture` -> `census`**, so
`census_funnel_tests` is inside the only lane that builds this feature set. The
funnel is squarely this gate's subject: the gate exists to check "whether the
comparison is ever reached with a document worth comparing", and the funnel is
what decides that.

**3. And the widened group is SERIALISED, which the widening itself forced.**
Running the funnel beside the fixture in one process failed immediately:

```
nros metadata mode: recorder rejected node `census_fixture` —
  raise the MetadataRecorder capacity
```

That is not a capacity bug. The `MetadataRecorder` is process-global by design
(`nros::metadata_mode` says so, and `nros-rmw-metadata`'s own tests carry a
`Mutex` for exactly this), and the funnel tests drive the switch through
`env::set_var`, which is process-global too. Each module had been internally
consistent; nothing serialised them against *each other*, because until now they
were never in one process. `--test-threads=1` is this group's contract, not a
flake workaround, and it is commented as such at the recipe.

Worth noting for whoever adds the next census test: the constraint is a property
of the RECORDER, so it applies to any test that touches it, in any module.

## Verified

`just check census-hooks-complete` — rc=0. The final cargo invocation went from
**1 test** (the fixture alone) to **3 passed, 37 filtered out**: the fixture plus
the two funnel tests, which had never run in this lane.
