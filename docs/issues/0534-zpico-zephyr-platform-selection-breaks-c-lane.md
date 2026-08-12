---
id: 534
title: "`zpico-sys` selecting the `zephyr` platform breaks the Zephyr C zenoh leaves on a missing `version.h`"
status: open
type: bug
area: zephyr
related: [issue-0529, issue-0528, phase-348]
---

## Symptom

Zephyr C zenoh fixture leaves fail in `zpico-sys`'s build script:

```
cargo:warning=…/zenoh-pico/include/zenoh-pico/system/platform/zephyr.h:18:10:
fatal error: version.h: No such file or directory
```

Affected in one module run: `build-c-service-client-zenoh`,
`build-c-service-server-zenoh`, `build-c-action-client-zenoh`. It takes the
zephyr fixture module down, and zephyr is an order-only prerequisite of every
other platform, so it blocks `build-test-fixtures` and therefore `ci-matrix`.

## Cause — attributed at HUNK level, not by bisect

`292547dd5` (fix #529) added `zephyr` to `zpico-sys`'s platform selection:

```rust
    } else if use_zephyr {
        Some("zephyr")
```

Its commit message states the case for why this is safe:

> No behaviour change today: `build_c_shim` is skipped on Zephyr (below), so the
> config header these knobs feed has no consumer, and the C lane gets the same
> values from Kconfig via `nros_rmw_zenoh.cmake`.

**The evidence contradicts that.** Neutralising exactly that branch at current
HEAD (`else if false`) and rebuilding the leaf from a pristine build dir:

| tree | result |
| --- | --- |
| HEAD | `version.h: No such file or directory`, exit 2 |
| HEAD with that one branch neutralised | **exit 0**, zero errors |

Nothing else was changed between the two runs. Selecting the platform is
therefore not inert on Zephyr: naming it turns on the platform manifest's
include handling, and a TU that includes `zenoh-pico/system/platform/zephyr.h`
then needs the generated `version.h` — which is produced by the shim build that
Zephyr skips. So the very skip the message relies on is what makes the include
unsatisfiable.

The #529 change is still right in intent: the resolver SHOULD be total over the
platforms that have a config file, so the next knob added to that table is not
silently ignored. What is missing is making the include set that comes with it
satisfiable on a lane that does not build the shim.

## Reproduce

```
NROS_ZEPHYR_WORKSPACE=<ws> scripts/build/zephyr-fixture-make-driver.sh \
    --filter 'c/service-client.*zenoh'
```

Reproduces SOLO and from a PRISTINE build dir, so it is neither a parallel-build
race nor stale state — both were checked, because in this area they are the
usual answer.

## Direction

Either generate `version.h` (or its include path) on the Zephyr lane too, or
scope the platform manifest's `include_paths` to the lane that builds the shim.
The first keeps the resolver total, which is what #529 was for.
