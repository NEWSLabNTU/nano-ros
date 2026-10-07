---
id: 1693
title: "The census run of a contracted C++ entry records nothing since `/diagnostics`
  started publishing violations — `derived-tiers-cpp` `native_entry` exits 156 and
  the live-peer fixture build stops there"
status: resolved
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

## Resolution

Fixed 2026-10-06 (branch `fix/1693-census-arms-no-reporter`).

**Root cause, measured.** Reproduced on `origin/main` 52dcf5b5a2 by building
`derived-tiers-cpp` `demo_bringup:native` and running `nros ws entity-census
run --entry native_entry`: the issue's two log lines, then `exited exit status:
156 and wrote nothing`. The inferred link holds, and the mechanism is: the
generated setup calls `nros_cpp_install_monitors` BEFORE the first node exists
(a publisher attaches its counter cell at create time), which armed the
`/diagnostics` reporter in census mode too. The recording backend attributes
every entity to the current node, had none open, and refused the publisher
(`nros::metadata_mode::record` -> `PublisherCreationFailed`). `diag::arm` then
RETURNED that refusal (`-100`) although its own log line said "violations stay
in the log only", so the generated setup returned before creating any node, the
dump found nothing (`rc=-2`) and the process exited 156 (`-100 & 0xff`).

**Fix.**
- `nros::contract::arm_reporter` -- THE arming step, shared by the Rust
  `install_contract_monitors` (`nros::main!`, issue 1676) and nros-cpp's
  `diag::arm`, which the C AND C++ packs both reach through
  `nros_cpp_install_monitors`. It arms nothing in a census run, and a reporter
  that cannot be created is logged and never fatal -- what the C/C++ log line
  always said and what the Rust road's doc claimed the C/C++ road did.
- `nros::census_hooks::recording_run` + `RECORDER_RMW`: "is the recorder the
  selected RMW", one predicate; the three census funnels (nros-cpp, nros-c,
  nros-board-linux) select the recorder through the same constant, so
  the census funnel tests fail if the two ever name different backends.

The reporter is not RECORDED instead: it is runtime infrastructure the
inventory already counts (`EntityInventory::contract_reporters`, issue 1676),
a census never spins so it could never publish, and the recorder has no node to
file it under.

**Before / after** (`nros ws entity-census run`, same entry):
- before: `[ERROR] ... /diagnostics publisher could not be created ...`,
  `dump ... failed (rc=-2)`, `exited exit status: 156 and wrote nothing`;
- after (also re-measured after rebasing onto e7c3f9e2da):
  `entity-census native_entry: 4 node(s), 0 sub / 4 pub / 0 service server / 0
  service client / 4 timer slot(s) / 0 parameter(s) in 20 ms` -- the line the
  last green live-peer run (37180326202) printed.

**Tests.** `census_funnel_tests::a_contracted_entry_census_records_its_entities`
installs a one-row table before the node, as a generated entry does; on the old
`diag.rs` it fails `left: -100, right: 0` with the issue's two log lines.
`only_the_recorder_selector_is_a_census_run` is the predicate's negative control
(unset, empty and `zenoh` are boots).

**The class.** The C census road reaches the same `nros_cpp_install_monitors`
(both packs' `monitor_install.jinja`), so it is fixed by the same change. The
Rust road (`install_contract_monitors`) was already non-fatal, so its census
still recorded, but it attempted the same publisher and logged the same error in
every contracted Rust census; it now arms nothing there either. Sweep:
`grep -rn 'DiagSink::create\|set_var("NROS_RMW"' packages/ --include='*.rs'`
-- one `DiagSink::create` left (inside `arm_reporter`).

**Not measured.** The live-peer lane itself (the job that reported this) was not
re-run. The Rust road's census was not run on a contracted Rust image (the only
one, `realtime-rust`'s `derived_bringup`, is a Zephyr image); it is covered by
sharing `arm_reporter`, not by a measurement. A normal (non-census) boot of the
fixed `native_entry` against `rmw_zenohd` was run: it showed an intermittent
SIGSEGV that `origin/main` shows too (1 of 4 runs there), filed as
[issue 1711](1711-native-multi-tier-cpp-boot-segv-in-malloc.md).
