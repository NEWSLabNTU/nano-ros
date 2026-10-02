---
id: 1536
title: "The `zephyr_self_pkg_sibling` compile-check fixture fails its
  west-configure, reported but NOT reproduced in a main checkout"
status: open
type: bug
area: testing, zephyr
severity: medium
found: 2026-09-28
related: [1501, 1521, phase-470]
---

## What this is

`examples/fixtures.toml`:

```toml
[[compile_check_fixture]]
id = "zephyr_self_pkg_sibling"
builder = "west-configure"
dir = "packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling"
west_subdir = "caller"
west_board = "native_sim/native/64"
west_extra = "-DCONF_FILE=prj.conf"
output = "nros-system/system_config.h"
```

A phase-470 W5.b1 run reported this record failing during
`just zephyr build-fixtures` with:

```
No SOURCES given to target: app
```

and attributed it to `fix(#1501): the sibling self-pkg routes as cargo, and 1501
files where resolved issues live` (2026-09-25) — whose thesis is that the sibling
`alpha_pkg` declares `nros_cargo` and carries no `CMakeLists.txt`, while
"the sibling `caller/` app is what" consumes it. That commit does touch this
fixture (`fixtures/zephyr_self_pkg/sibling/alpha_pkg/package.xml`), so the
mechanism is at least coherent: if `alpha_pkg` no longer contributes C sources,
the `caller/` Zephyr `app` target can legitimately end up with none.

## Evidence status — read this before acting

**The `No SOURCES` failure is a second-hand observation from an agent worktree
and was NOT reproduced in a main checkout.** It is recorded here because the
report was specific and the blamed commit checks out, not because it has been
confirmed. Four attempts in the main checkout each stopped on a *different*
precondition before reaching the fixture, and a fifth reached a configure but
not a faithful one:

1. in-tree `nros` CLI stale (branch switch) — `just setup-cli`;
2. same again after the next branch switch;
3. `nros-launch-resolve` built from a different `play_launch` commit than `nros`
   (issue 0409's guard), tripped by restoring that submodule's recorded pin
   earlier the same day;
4. `NROS_ZEPHYR_FIXTURE_FILTER=zephyr_self_pkg_sibling` → `no records matched
   filter`. **That filter selects `[[fixture]]` west leaves; this is a
   `[[compile_check_fixture]]`, a different record type in a different lane** —
   so the filter can never match it, and the "no records matched" is correct
   rather than a symptom;
5. invoking `west build` directly on `sibling/caller` fails earlier, at
   `codegen-system: … alpha_pkg/system.toml declares system semantics but no
   SystemModel was found`, because the lane runs `nros sync` first and a direct
   invocation does not. That is the harness's prep step missing, not the defect.

So the reproduction recipe is still unknown, and **step 4 is worth keeping
whatever the outcome**: there is no obvious way to build one compile-check
fixture by name, which is why five attempts went to the environment instead of
the question.

## Why it went unnoticed

Same class as issue 1521. `[[compile_check_fixture]]` records run in the
compile-check lane, which lives in `check-build` — `schedule` /
`workflow_dispatch` only. No `pull_request` and no `merge_group` event runs it,
so a break here is invisible to everything a contributor sees before merging,
and stays invisible afterwards.

## What to do

1. **Reproduce it, or show it is already gone.** Someone with a warm Zephyr
   workspace should run the compile-check lane and say which. If it does not
   reproduce, close this and leave the note about the filter (below), which is
   independently true.
2. If it reproduces: the fix is presumably to give `caller/` its own sources, or
   to restore whatever `alpha_pkg` contributed before it was routed as
   `nros_cargo` — decide from `#1501`'s intent, not from this issue's guess.
3. **A way to run ONE compile-check fixture by id.** `NROS_ZEPHYR_FIXTURE_FILTER`
   covers `[[fixture]]` only. Without a sibling for `[[compile_check_fixture]]`,
   verifying one of these costs a full lane, which is how an unverified report
   like this one ends up filed instead of answered.

## Acceptance

- The lane is green, or this issue is closed with the measurement that shows the
  failure is gone.
- One compile-check fixture can be built by id, and the command is recorded here.

## Reproduced, on the live-peer lane (2026-09-30)

This issue records the failure as *"reported but NOT reproduced in a main
checkout"*. It reproduces.

Live-peer regression **36668247728** (schedule 04:18), job **109737902225**
(`rows whose board is NOT this runner`), step `just build`, at main
`965504e38`:

```
CMake Error at .../zephyr/3.7/zephyr/cmake/modules/extensions.cmake:428 (add_library):
  No SOURCES given to target: app
Call Stack (most recent call first):
  .../cmake/modules/kernel.cmake:216 (zephyr_library_named)
  ...
  CMakeLists.txt:2 (find_package)

FATAL ERROR: command exited with status 1: /usr/bin/cmake
  -B .../build/west-fixtures/zephyr_self_pkg_sibling -GNinja
  -DBOARD=native_sim/native/64 -DCONF_FILE=prj.conf
  -DZEPHYR_EXTRA_MODULES=/__w/nano-ros/nano-ros
  -S .../packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling/caller
```

`west-fixtures: 1 of 5 fixture(s) FAILED to build`, and that one is this
fixture — so the whole lane's `build-fixtures` recipe exits 1 on it.

## One thing worth noting about the sibling fixture's twin

The same `No SOURCES given to target: app` error appears earlier in the same log
for **`zephyr_self_pkg_rust`** (`-S .../zephyr_self_pkg/self/alpha_pkg`), and
that one is NOT counted as a failure: the line after it reads
`ok .../build/west-fixtures/zephyr_self_pkg_rust (nros-system/system_config.h)`.
So a configure-only check whose byproduct exists passes despite the same CMake
error, and only the sibling — whose product is missing — fails.

Whether that asymmetry is correct is not decided here. It does mean the error
message alone does not distinguish the two, which is worth knowing before
treating the sibling's failure as unique to it.

## REPRODUCED in CI, 2026-10-01 — with the west command line (2026-10-02)

The title's qualifier can come off: this is no longer only a report. The
**live-peer regression** lane reproduced it on a scheduled run.

Run **36814478655** (schedule, 2026-10-01T04:18:17Z, head `e61d7dfd2`), job
**110216752409** `rows whose board is NOT this runner`, step `Build the fixtures
those rows resolve`:

```
CMake Error at …/zephyr/cmake/modules/extensions.cmake:428 (add_library):
  No SOURCES given to target: app
Call Stack (most recent call first):
  …/kernel.cmake:216 (zephyr_library_named)
  …/zephyr_default.cmake:141 (include)
  …/ZephyrConfig.cmake:66 (include_boilerplate)
  …/ZephyrConfig.cmake:92 (include_boilerplate)
  CMakeLists.txt:2 (find_package)
CMake Generate step failed.  Build files cannot be regenerated correctly.
```

and the command, which is the thing a reproduction attempt needs and the
earlier report did not carry:

```
FATAL ERROR: command exited with status 1: /usr/bin/cmake \
  -DWEST_PYTHON=/usr/bin/python3 \
  -B<ws>/build/west-fixtures/zephyr_self_pkg_sibling -GNinja \
  -DBOARD=native_sim/native/64 -DCONF_FILE=prj.conf \
  -DZEPHYR_EXTRA_MODULES=<ws> \
  -S<ws>/packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling/caller
```

Zephyr `3.7` from the provisioned workspace
(`~/.nros/workspaces/zephyr/3.7/zephyr`).

Two things the log settles. **The configure got far enough to do our work**:
`nros_system_generate` baked `…/zephyr_self_pkg/sibling/alpha_pkg` into
`build/west-fixtures/zephyr_self_pkg_sibling/nros-system`, reported `domain 0
agrees`, resolved the codegen tool, and printed `Configuring done` — the failure
is at Zephyr's GENERATE step, after ours. So whatever is missing is the `app`
target's sources in the `caller` subdir, not the sync this fixture's siblings
(1488, 1501) were about.

**And the lane counts it correctly**, which is why it is visible at all:

```
west-fixtures: 1 of 5 fixture(s) FAILED to build.
               A fixture build that produces nothing is a build FAILURE,
               not a skip — the lane cannot promise what it did not build.
```

The other 4 of 5 built. This is the whole of that job's failure, so the
live-peer lane's red on that run is this issue and nothing else.

## Wider than the title: `zephyr_self_pkg_rust` fails identically, and the harness has been hiding both (2026-10-02)

`live-peer regression` run **36963917981**, job **110703711587**. The `self`
variant fails with the same error as the `sibling` one:

```
-- nros_system_generate: baking …/zephyr_self_pkg/self/alpha_pkg → …/zephyr_self_pkg_rust/nros-system (rmw=zenoh)
-- Configuring done
CMake Error at …/extensions.cmake:428 (add_library):
  No SOURCES given to target: app
CMake Generate step failed.
```

So this is not a `sibling`-layout problem. Both rows of `zephyr_self_pkg` —
`self/alpha_pkg` (`zephyr_self_pkg_rust`) and `sibling/caller`
(`zephyr_self_pkg_sibling`) — reach Zephyr's generate step with no sources on the
`app` target. Our half completes in both: the package is baked, the domain
agrees, `Configuring done` prints.

### And this explains the intermittency the title records

The entry above says "reported but NOT reproduced in a main checkout". The
reproduction was never the problem — **the harness counts these rows as built**.
Both declare `output = "nros-system/system_config.h"`, which
`nros_system_generate` writes *before* the generate step that fails, so
`scripts/build/west-fixtures.sh` finds the artifact and reports `ok`. In this run
four fixtures hit `FATAL ERROR` and the summary read `1 of 5 fixture(s) FAILED`.
That is filed separately as issue **1627**, because it is a measurement defect
that hides this one rather than a cause of it.

Practical consequence for anyone working on this issue: a green
`just zephyr build-fixtures` does **not** mean these two configure. Read the log
for `CMake Generate step failed`, or check for `build.ninja` in
`build/west-fixtures/zephyr_self_pkg_{rust,sibling}/`.
