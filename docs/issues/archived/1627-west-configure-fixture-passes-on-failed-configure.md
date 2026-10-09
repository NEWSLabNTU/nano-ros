---
id: 1627
title: "A `west-configure` fixture whose declared `output` is written BEFORE the
  generate step counts as built, so a failed configure is indistinguishable from
  a successful one — 4 FATAL errors reported as `1 of 5 FAILED`"
status: resolved
type: bug
area: ci, testing, zephyr
severity: medium
found: 2026-10-02
resolved: 2026-10-10
related: [issue-0700, issue-1016, issue-1536, issue-1453, issue-1777, phase-477]
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
- **Not a disk or runner fault.** The run's OTHER job did die of issue 1353's
  ENOSPC, so this needs saying precisely: THIS job (110703711587) carries no
  ENOSPC annotation, reported `freed 5264 MB; 67912372 KB free` — **65 G free** —
  ran 04:19:05 to 04:56:48 and completed every step after the failing one. The
  miscount happened with ample disk.

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

## Two of the four now generate (2026-10-02, issue 1536)

`zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling` never named a source for
Zephyr's `app`, which is the whole of their `No SOURCES given to target: app`.
Issue 1536 gave each a never-compiled `src/main.c`; both now print
`-- Generating done` and leave `build.ninja`, so the `build.ninja` output this
issue proposes would hold for them. `west_board_import` (idlc) remains a row it
would turn red. Separately, a narrowed Zephyr lane no longer builds these rows at
all (1536 widened `NROS_ZEPHYR_FIXTURE_FILTER` to them), so the live-peer board
job no longer reaches any of the four.

## 2026-10-09 — decisions (phase-477 D1, D2)

- **The fix:** a `west-configure` row declares `build.ninja` as its output — the
  "What would close this" section above, taken as written. `west_board_import`
  is expected to turn red on the idlc provisioning gap; that red is real.
- **The filter:** a narrowed Zephyr lane keeps honouring
  `NROS_ZEPHYR_FIXTURE_FILTER` for the five west compile checks. Their coverage
  must come from a lane chosen for it; if none builds them, they join tier 2.

## Resolved (2026-10-10) — phase-477 decisions D1 and D2

The maintainer's 2026-10-09 decisions (phase-477 "Decisions — 2026-10-09"):
**D1** — a `west-configure` row declares `build.ninja`; **D2** — the narrowed
Zephyr lane keeps honouring `NROS_ZEPHYR_FIXTURE_FILTER`, and the five west
compile checks get their coverage from a lane chosen for it (tier 2 if none).

### D1 — what changed

- `examples/fixtures.toml`: `west_board_import`, `zephyr_self_pkg_rust` and
  `zephyr_self_pkg_sibling` declare `output = "build.ninja"`. `west-fixtures.sh`
  keeps its one rule ("the row declares what must exist"); west's exit status is
  still not read. CMake's Ninja generator writes `build.ninja` only when
  GENERATE succeeds, so the premise the rule needs now holds.
- **Gate:** `fixtures-manifest.py validate-compile-checks` (run by
  `just check fixtures-manifest`, on the fast line) refuses a `west-configure`
  row whose `output` is not in `WEST_CONFIGURE_GENERATE_OUTPUTS`
  (`build.ninja`). Negative control, against the pre-fix manifest
  (`git show origin/main:examples/fixtures.toml`, passed as `--manifest`):

  ```
  fixtures-manifest.py: west_board_import: builder 'west-configure' gates on its
  output EXISTING, so the output must be written by CMake's GENERATE step — one
  of build.ninja (got 'CMakeCache.txt'). …
  rc=1
  ```
  and `validated 63 compile-check fixture(s)`, rc 0, against the fixed one.
- **The test side had the same hole, one layer down.** `require_west_fixture`
  read `.compile-ok` only if present; with no stamp it fell through to the
  artifact the TEST reads (`CMakeCache.txt`, `system_config.h`), which a failed
  configure leaves on disk. So the consumer passed over a build the lane had
  counted FAILED. It now requires the stamp (written only when the declared
  output exists), through the same tier-aware `require_prebuilt_binary` funnel.
  Measured on the broken fixture below: with the change,
  `zephyr_self_pkg_rust_builds_via_shim` FAILS with
  `FixtureNotBuilt(… zephyr_self_pkg_rust/.compile-ok …)`; with that one line
  reverted it PASSES over the same failed build.
- Other consumers of the rows' `output`, checked: `compile-check-signature.sh`
  hashes the whole record (the change re-stales the three rows once — one
  rebuild), `compile-check-stale.sh` and `check-fixtures-stale.sh` ignore the
  field, and nothing else under `scripts/` or `packages/testing/nros-tests`
  reads it.

### D1 — acceptance, run locally (Zephyr 3.7 store workspace, native_sim)

Deliberately broken `zephyr_self_pkg_rust` (its `target_sources(app …)` line
commented out, so configure succeeds and generate fails), built through
`just zephyr build-fixtures` with the filter narrowed to that row:

| `output` | west's log | verdict |
| --- | --- | --- |
| `build.ninja` (fixed) | `No SOURCES given to target: app` / `CMake Generate step failed` | `MISSING build.ninja` — `1 of 1 fixture(s) FAILED`, recipe exit 1 |
| `nros-system/system_config.h` (pre-fix) | same | `ok (nros-system/system_config.h)` — `1/1 ok`, exit 0 |

Restored, the genuinely configuring rows: `zephyr_self_pkg_rust` and
`zephyr_self_pkg_sibling` both `-- Generating done`, `ok (build.ninja)`;
`west_board_import` `ok (build.ninja)` on this host (`idlc` from
`/opt/ros/humble`, cyclonedds submodule initialised). The `board_import` and
`zephyr_self_pkg` nextest targets: 3 passed.

**The expected red is real.** In a fresh worktree without
`third-party/dds/cyclonedds`, `west_board_import` now reports
`MISSING build.ninja` (configure: `Cyclone DDS submodule not initialised`),
where it used to report ok. On a runner without `idlc` it will report the
idlc error above. That provisioning gap is filed as **issue 1777** — it needs
its own fix (provision `--rmw cyclonedds` in the lane, or gate the row), and
this issue does not hide it.

### D2 — which lane builds the five west compile checks

**Tier 2 already does, and its fixture gate already demands them**, so no lane
change was needed:

- `.github/workflows/run-matrix.yml:148` runs `just build tier2`;
  `justfile:295` (`_build-scope`) maps it to `build-test-fixtures lane=tier2`.
- `lane-coords tier2 --modules` lists `zephyr` (measured: freertos, native,
  nuttx, qemu, threadx_linux, threadx_riscv64, zephyr), so the zephyr stage runs
  `just zephyr build-fixtures` (`justfile:2087` jobserver path, `justfile:2217`
  make path) with NO `NROS_ZEPHYR_FIXTURE_FILTER` set.
- `just/zephyr-ci.just:532` runs `scripts/build/west-fixtures.sh`, whose only
  narrowing is that filter (`scripts/build/west-fixtures.sh:166`); it does not
  read `NROS_FIXTURE_COORDS`, so all five rows build under tier 2.
- `scripts/check-fixtures-stale.sh:339` drops the west rows only for scope
  `native`; tier 2's `coords` scope keeps demanding their `.inputsig`, so tier 2
  cannot go green without building them.
- The tier-2 nightly does the same (`.github/workflows/nightly.yml:1202`,
  `just build tier2-nightly`).

The only narrowed caller is live-peer's board job
(`scripts/check-interop-verdicts.py:263`, filter
`build-ws-rs-qos-entry-zenoh`), which correctly builds none of them.
