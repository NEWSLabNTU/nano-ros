---
id: 1627
title: "A `west-configure` fixture whose declared `output` is written BEFORE the
  generate step counts as built, so a failed configure is indistinguishable from
  a successful one — 4 FATAL errors reported as `1 of 5 FAILED`"
status: open
type: bug
area: ci, testing, zephyr
severity: medium
found: 2026-10-02
related: [issue-0700, issue-1016, issue-1536, issue-1453]
---

## Measured

`live-peer regression` run **36963917981** (schedule, 2026-10-02T04:17:37Z, head
`4c3850cb8`), job **110703711587** `rows whose board is NOT this runner`, step
`Build the fixtures those rows resolve`.

**Four** west fixtures hit a fatal configure error — log lines 7458, 7550, 7629,
7708:

```
FATAL ERROR: command exited with status 1: … -B…/west-fixtures/west_bringup_zephyr_cyclone_user_config …
FATAL ERROR: command exited with status 1: … -B…/west-fixtures/west_board_import …
FATAL ERROR: command exited with status 1: … -B…/west-fixtures/zephyr_self_pkg_rust …
FATAL ERROR: command exited with status 1: … -B…/west-fixtures/zephyr_self_pkg_sibling …
```

The harness reported **one**:

```
west fixtures: 4/5 ok (0 reused, 4 built).
west-fixtures: 1 of 5 fixture(s) FAILED to build.
   MISSING zephyr/zephyr.exe for west_bringup_zephyr_cyclone_user_config
```

## Why exactly one of the four was caught

It is the builder and the declared output, not the error:

| fixture | builder | `output` | written before the failure? | counted |
| --- | --- | --- | --- | --- |
| `west_bringup_zephyr_cyclone_user_config` | `west-build` | `zephyr/zephyr.exe` | no — needs a link | **FAILED** |
| `west_board_import` | `west-configure` | `CMakeCache.txt` | yes — CMake writes it early | ok |
| `zephyr_self_pkg_rust` | `west-configure` | `nros-system/system_config.h` | yes — **we** write it | ok |
| `zephyr_self_pkg_sibling` | `west-configure` | `nros-system/system_config.h` | yes — **we** write it | ok |

The ordering is in the log, for `zephyr_self_pkg_rust`:

```
-- nros_system_generate: baking …/zephyr_self_pkg/self/alpha_pkg → …/zephyr_self_pkg_rust/nros-system (rmw=zenoh)
-- Configuring done
CMake Error at …/extensions.cmake:428 (add_library):
  No SOURCES given to target: app
CMake Generate step failed.  Build files cannot be regenerated correctly.
FATAL ERROR: command exited with status 1: …
```

`nros_system_generate` writes the declared output, CMake finishes CONFIGURING,
and the failure is at **GENERATE**. The artifact is on disk; the build is not.

## This is a stated rule whose premise fails, not an oversight

`scripts/build/west-fixtures.sh` is explicit about it:

```sh
# The stamp gate is `output` EXISTS, for both builders — not west's exit
# code. A `west-configure` row is expected to stop before linking, and a
# `west-build` row that exits 0 without its image is not built either. One
# rule, and it is the row's own declaration.
env "${tc_env[@]}" west "${args[@]}" || true
if [ -e "$bld/$output" ]; then …   # ok
else …                             # MISSING, failed++
```

That reasoning is correct for the case it names: a `west-configure` row stops
before linking, so west exiting non-zero is not evidence of anything, and the
row's declaration is the honest test. **The premise is that the output can only
exist if the thing we wanted happened.** For these three rows it does not hold —
one output is CMake's own early bookkeeping and two are written by our module
before Zephyr's generate step runs.

Any fix has to keep what that comment protects. "Check the exit status" on its
own would start failing `west-configure` rows for stopping before a link, which
is what they are for.

## What this is NOT

- **Not 1536.** That issue is one fixture's own breakage (`zephyr_self_pkg_sibling`,
  `No SOURCES given to target: app`). This is why 1536 reads as intermittent: the
  failure recurs, and the harness surfaces it only when something else also makes
  the artifact absent. The two are separate and both real.
- **Not 0700**, which is the converse and already fixed: a fixture that produced
  nothing used to exit 0. Producing nothing while provisioned now fails. Producing
  *something* while failing still passes.
- **Not 1016.** That is about a west leaf whose build-dir NAME no lane models, so
  its verdict is a stale message. Here the row is modelled, built, and reported —
  reported OK.
- Not a disk or runner fault: this job ran 60+ minutes and the other three
  fixtures built.

## What would close this

A `west-configure` row needs an output that the **generate** step must have
produced. `build.ninja` is the natural candidate — CMake writes it at the end of
generate, so its presence means configure AND generate succeeded, and its absence
after a provisioned run is a real failure. That keeps one rule ("the row declares
what must exist") and fixes the premise rather than bolting an exit-status test
beside it.

Acceptance: re-run this lane against a deliberately broken `west-configure`
fixture and see it counted as FAILED, with the three rows above still passing
when their configure genuinely succeeds.

## The same error, counted once — the cleanest demonstration

Two of the four failures are the **identical** error, and they differ only in
what their row declares:

```
CMake Error at …/zephyr/cmake/nros_rmw_cyclonedds.cmake:375 (message):
  host Cyclone idlc not found.
    searched: SDK store hints [], host PATH, then …/build/{cyclonedds,install}/bin
    Remedies: install ROS 2 (idlc on PATH), run `nros setup <board> --rmw cyclonedds`,
    or set IDLC_EXECUTABLE.
```

- `west_bringup_zephyr_cyclone_user_config` — `west-build`, `output =
  zephyr/zephyr.exe` → **counted FAILED**.
- `west_board_import` — `west-configure`, `output = CMakeCache.txt` → **counted
  ok**.

Same cause, same lane, same run; one is a verdict and one is silence. Nothing
about the defect being detected depended on the defect — only on whether the
declared artifact happened to precede the failure.

The other two failures are `No SOURCES given to target: app` on
`zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling`, both `west-configure` with
`output = nros-system/system_config.h`, both counted ok. That widens issue
1536 beyond its `sibling` title, and is appended there.

## The provisioning gap underneath the idlc half

Separate from this issue and not filed on its own yet: this lane configures a
Cyclone fixture without ever running `nros setup <board> --rmw cyclonedds`, which
is what the error's own remedy text names. The `SDK store hints []` in the
message is the index saying it was never consulted. If the harness is fixed per
the section above, that gap becomes a second visible red rather than a new one.
