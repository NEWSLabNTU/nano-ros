---
id: 2137
title: "`nros build` never takes the census a cross image is checked against, so `nros sync && nros build --all` refuses every contracted cross image with `census missing`"
status: resolved
type: bug
area: [build, cli]
severity: medium
found: 2026-10-27
related: [1419, phase-463, phase-517, phase-452]
resolved: 2026-10-29
resolved_in: "#2820"
---

## What

`nros sync && nros build --all` on `examples/workspaces/features` (a copy-out,
and equally the in-tree workspace in a fresh checkout) built the six Rust
`linux_*` images and then stopped:

```
Error: `demo_bringup:zephyr_rust_qos`: the census and the contract disagree, or the census no longer describes the code (issue 1419)

Caused by:
    census missing: no census at `<ws>/build/nros/models/demo_bringup/rust_qos_model.census.json`
    Take a census of the code as it is now, from the workspace root:
        nros ws entity-census take --image demo_bringup:zephyr_rust_qos
```

## Why

Neither of the two suspected misfires. The image is correct: its contract
(`rust_qos.contract.yaml`) agrees with the code, and once a census exists the
check passes. The guard is correct too: the census was simply ABSENT.

Issue 1419 made `[census] on_missing` default to `refuse`, and made two
producers of the census: `nros ws entity-census take`, typed by hand, and
`scripts/build/census-prepass.sh`, which every fixture builder runs before its
stages. `nros build` itself, the user's front door, takes none. The rule it
inherited (the configure must never build the native image, issue 0641) is
about a CONFIGURE. `nros build` is the driver one level above every configure,
and it already plans the host sibling (`linux_rust_qos`) in the same run. So a
user following the README (`nros sync`, then `nros build`) hit a refusal that
only the fixture lanes could avoid. Every lane that builds the example runs
the pre-pass, which is why no gate saw it. A fresh checkout fails in-tree the
same way, so this is not specific to a copy-out.

## Fix

`nros build` takes the census of every CROSS image itself, at stage 3.4 of its
plan. That is after preflight and before the resolve phase (which reads the
census for callback slots), and before both checks: stage 4 for the cargo and
west-Rust roads, the configure for cmake. It calls the same `take` a person
would type, so it does nothing when the model folds in no contract or the census
is already fresh. It never fails the build. A take that cannot run is printed,
and the check then decides under the bringup's `[census]` policy, as before.
Under the fixture pre-pass (`NROS_CENSUS_PREPASS=done`) it takes nothing,
because those stages run in parallel and the pre-pass has taken every census.

Rule: `census_take_is_ours` in `packages/cli/nros-cli-core/src/cmd/build.rs`.
Test: `stage4_census_tests::nros_build_takes_the_census_of_every_cross_image`.
