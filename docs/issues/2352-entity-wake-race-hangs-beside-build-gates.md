---
id: 2352
title: "entity_wake's arrival-vs-drain race test hangs to its 60 s timeout when it runs beside the build-tier gates"
status: open
type: bug
area: [testing, executor, rmw-zenoh]
severity: medium
found: 2026-11-07
related: [2221, 1980, 1997, phase-543]
---

## Measured

phase-543's order-independence check (`just check gate-order build`) runs the
build tier twice, starting the tests in opposite name orders. On 2026-11-07
(lane `dev`, host newslab-118, 48 cores), `build::check::required-features-tests`
failed in the ascending run and passed in the descending one. Every other build
gate had the same verdict in both orders.

The red inside that gate was one test, in the `nros-tests` feature lane
(`--features trigger-test,component-runtime-test,phase216-substrate,signal-fd-wake-test`):

    TIMEOUT [  60.026s] nros-tests::entity_wake
        an_arrival_racing_a_drain_is_never_left_undispatched::case_2_one_sample_per_dispatch

The same test, from the same build:

- alone, 8 runs: 8 passes, about 0.48 s each;
- 48 copies run at once, both cases: 48 of 48 pass, twice.

So plain CPU load is not enough to trigger it. The failing run had the other
build gates beside it, compiling and running their own test binaries and
routers, and which gates ran beside it is exactly what changes between the
two orders.

## What it is (not yet known)

The test opens its own router (`zenohd_unique`), its own ROS domain, and an
executor that races an arrival against a drain. A 60 s hang, not a wrong
count, suggests either a router that never becomes ready under that load, or
a wake that is lost so the executor parks forever. It is not a retry
candidate (no retries, maintainer ruling 2026-11-02): the next step is to make
it reproduce. Run the build tier with this test's feature lane beside it, or
`just check gate-order build`, and capture the router's and the executor's
state at the timeout.

## What would close it

The cause found and fixed, and `just check gate-order build` showing the same
verdict for `required-features-tests` in both orders on a loaded host.
