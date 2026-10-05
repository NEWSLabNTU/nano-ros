---
id: 1693
title: "The census run of a contracted C++ entry records nothing since `/diagnostics`
  started publishing violations — `derived-tiers-cpp` `native_entry` exits 156 and
  the live-peer fixture build stops there"
status: open
type: bug
area: [census, cli, diagnostics, ci]
severity: medium
found: 2026-10-05
related: [1635, 1419, 1556, 1666]
---

## What was measured

Scheduled `live-peer regression` run **37263280002** (head `9967262ff`), job
**111615055870** "rows whose board is NOT this runner", step "Build the fixtures
those rows resolve". The census prepass for
`examples/workspaces/derived-tiers-cpp` `demo_bringup:zephyr` takes its census
with `demo_bringup:native` (`native_entry`), and the run ends:

```
[ERROR] nros: contract monitors installed, but the /diagnostics publisher could not be created (Backend("rmw_ret error")); violations stay in the log only (issue 1635)
[ERROR] nros: nros census: dump to `.../derived-tiers-cpp/build/nros/models/demo_bringup/system_model.census.recorded.json` failed (rc=-2)
Error: census run of `.../derived-tiers-cpp/build/posix-zenoh-native/cmake/native_entry` exited exit status: 156 and wrote nothing -- the funnel prints the reason
error: recipe `build-fixtures` failed with exit code 1
```

`rc=-2` is `nros_cpp_metadata_dump`'s "nothing was recorded"
(`packages/api/nros-cpp/src/lib.rs`, `census_write`): the recorder saw no
entity at all.

The night before (run **37180326202**, head `d914ba0b8`, job 111371626353) the
same prepass on the same entry succeeded:

```
entity-census native_entry: 4 node(s), 0 sub / 4 pub / 0 service server / 0 service client / 4 timer slot(s) / 0 parameter(s) in 20 ms -> .../derived-tiers-cpp/build/nros/models/demo_bringup/system_model.census.json
```

Between the two heads, the change that touches this path is #1656 (issue 1635:
`5e25ff4e2` "a contracted C/C++ image publishes its violations on
/diagnostics", `c2d6b22ef` "the sink is fed at detection"). The first error
line above is that change's own diagnostic (`packages/api/nros-cpp/src/diag.rs`).
The order of the two lines suggests the `/diagnostics` publisher is created
during the census run, its creation fails against the census backend, and
the recorder then holds nothing; that link is inferred from the log order and
the timing, NOT measured.

## What it is not

- Not disk (issue 1353): this job ended with ~68 GB free; its sibling board job
  is the one that hit ENOSPC.
- Not issue 1666 (`no such command: nextest`): the job never reached the test
  step.
- Not a missing contract: the prepass for this workspace reached the census
  run; the "no contract" lines in the same log are for other workspaces.

## Where it shows

Only the live-peer lane runs the census prepass over this workspace, so no
merge-gating lane saw it. `realtime-rust`'s Rust census in the same job
succeeded, so the Rust road is not affected in this run.

## What would close it

The census run of `derived-tiers-cpp`'s `native_entry` writing its census
again — for example by not creating the `/diagnostics` publisher in census
mode, or by recording it the way the census records any other publisher —
and the live-peer non-board job getting past "Build the fixtures those rows
resolve".
