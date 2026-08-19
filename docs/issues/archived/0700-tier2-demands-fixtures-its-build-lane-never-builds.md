---
id: 700
title: "`ci-matrix` selects the esp-idf and platformio bringup tests whenever their toolchains are present, but NO fixture lane builds those fixtures — so a green build lane hands the run a promise it never made"
status: resolved
type: bug
severity: medium
area: testing, build
related: [issue-0393, issue-0482, issue-0588, issue-0584, phase-340]
---

## Symptom

Tier 2, on a host with both toolchains provisioned, after
`just build-test-fixtures lane=tier2` returned **RC=0**:

```
Real failures: 3 / 3 total failures
  nros-tests::cli_bringup_esp_idf      esp_idf_esp32c3_2_component_bringup_builds
  nros-tests::cli_bringup_platformio   platformio_zephyr_framework_2_component_bringup_builds
  nros-tests::example_shape            every_canonical_leaf_has_readme   (unrelated, fixed)
```

with the resolver saying exactly the right thing:

```
binary MISSING for an in-lane coordinate:
  build/idf-fixtures/esp_idf_bringup/esp_idf_app/build/multi_pkg_workspace_esp_idf.elf
A gated run already asserted this lane's fixtures are built and fresh,
so this is a broken promise, not an environment skip.
```

It is a broken promise. The promise was never keepable.

## The gap, in three facts

1. **No fixture lane builds these.** `scripts/build/idf-fixtures.sh` is invoked
   from `just/esp32.just` and from `build-root.sh`'s kind table — and from
   nowhere else. `build-test-fixtures` contains **zero** references to `esp32`,
   `platformio`, or `idf` at any lane, `lane=all` included. Confirmed against
   the tier-2 build log: 0 occurrences of either string in a run that returned
   RC=0.

2. **The run selects them whenever the TOOLCHAIN is present.** `test-all`'s
   `env_exclude` deselects `binary(cli_bringup_esp_idf)` only when
   `idf.py` is absent AND `IDF_PATH` is unset AND `NROS_ESP_IDF_ENV_SHIM` is
   unset; `cli_bringup_platformio` only when neither `pio` nor `platformio` is
   on PATH. This host has `IDF_PATH=<repo>/esp-idf-workspace/esp-idf` (with a
   real `tools/idf.py`) and `pio` on PATH, so both are correctly selected.
   Selection keys on "can this host build it", the build lane keys on "is this
   coordinate in the lane", and nothing reconciles the two.

3. **The resolver cannot skip them, by construction.** `require_idf_fixture`
   resolves through `require_prebuilt_binary_fresh` and never consults the lane
   at all. Even if it did, these fixtures have **no `[[fixture]]` row** (0 rows
   matching `esp_idf_bringup|idf-fixtures|platformio`), so they have no
   coordinate — and `skip_reason_for_path` is `let row = attribute_path(p)?;`,
   whose `?` turns "cannot attribute" into `None`, i.e. *not out of lane*, i.e.
   run it. That is CLAUDE.md's stated rule working as designed ("an
   unattributable path is never skipped"); the rule assumes something built it.

Net: a lane that builds none of it, a selector that asks a different question,
and a resolver that is structurally unable to say "not mine".

## Why this is worth fixing rather than living with

The failure is indistinguishable from a real regression. It says "broken
promise", which is what a museum binary or a mis-scoped lane also says, so every
tier-2 run on a provisioned host costs somebody the same investigation. Issue
0588 already burned a cycle on the wrong one of these, which is why the message
lists three causes — this is a fourth it does not list.

It also silently sets the ceiling on what tier 2 can mean. Two tests in the run
scope can never pass in that lane, so "tier 2 green" is unreachable on a
provisioned host and the tier stops being a gate anybody can hold to.

## Directions

Not a plan; each is defensible and the choice is a maintainer's.

* **Give them coordinates.** A `[[fixture]]` row per bringup fixture makes them
  lane-attributable, so the tier-2 run SKIPS them and `lane=all` builds them.
  Most consistent with #393/#482 — the coordinate is how everything else in the
  tree answers this question — and the phase-340 P2 note already says a build
  with no row has no coordinate.
* **Build them in the lane that runs them.** Teach `build-test-fixtures` to
  invoke `just esp32 build-fixtures` / the platformio builder for the lanes
  whose run scope selects them. Honest, but it puts two more toolchains on the
  tier-2 critical path, which is what the tier ladder exists to avoid.
* **Make selection agree with the lane, not the host.** Add the lane predicate
  to the `env_exclude` block so a tier-2 run deselects what a tier-2 build never
  builds. Cheapest, and it leaves the fixtures buildable on demand — but it
  encodes the coupling in a third place rather than removing it.

**Not a direction:** deselecting on toolchain absence more aggressively. Both
toolchains ARE present here; that is the configuration in which this fails.

## Verification for whoever takes it

The reproduction is one clean lane build plus one run on a host with `IDF_PATH`
set and `pio` installed. The check that matters afterwards is that a tier-2 run
on a FULLY provisioned host is green — not one where the toolchains happen to be
missing, since absence hides the bug.


## RESOLVED 2026-08-19 — the coordinate, stated where the resolver can reach it

Took the first direction (make them lane-attributable), by the mechanism the
tree already has for a resolver that knows its coordinate without a
`[[fixture]]` row: `require_coord_in_lane`, as the px4 companion uses it.

```
esp-idf bringup     coordinate esp32,c,xrce
platformio bringup  coordinate esp32,c,zenoh
```

Neither is in tier 2 (`lane-coords tier2` = 13 coordinates; the esp32 ones are
`esp32,rust,zenoh` and `qemu-esp32-baremetal,rust,zenoh`), so a tier-2 run now
says so instead of failing:

```
[SKIPPED:lane] out of lane: esp_idf_bringup is at coordinate esp32,c,xrce,
which this run's lane does not select, so `just build-test-fixtures
lane=<this lane>` deliberately did not build it.
```

Verified against the real `lane-coords tier2` coordinate file, both tests.
Under bare nextest a `[SKIPPED:lane]` panic still prints as FAIL; `test-all`'s
junit rewrite is what turns it into a skip, which is issue 0673's mechanism.

### Two corrections to this issue's own analysis

Both matter for whoever revisits the build half.

**"No fixture lane builds these" is not quite right.** `build-test-fixtures`
fans out over `esp32`:

```sh
for platform in native qemu freertos nuttx threadx_linux threadx_riscv64 esp32 px4
```

and `just esp32 build-fixtures` ends with `bash scripts/build/idf-fixtures.sh
|| true`. So the lane DOES reach the builder. What it does not do is guarantee
an artifact: that builder self-gates on `idf.py`/`IDF_PATH` and is `|| true`, so
on a shell without the env it no-ops silently and the lane still returns RC=0.

**So the mismatch is narrower and nastier than "nothing builds it".** The
BUILDER gates on its env and may no-op; the SELECTOR gates on the test shell's
env and selects; neither consults the lane. The coordinate now settles it before
either gets a say — which is why this fix works regardless of which of the two
environments happens to be provisioned.

`esp32` being a tier-2 MODULE while `esp32,c,xrce` is not a tier-2 COORDINATE is
the distinction that makes both statements true at once, and it is exactly the
module-vs-coordinate split issue 0405 introduced `nros_lane_wants` for.

### Not done here

Giving these `[[fixture]]` rows outright. `compile_check_fixture` rows carry no
`platform`/`lang`/`rmw`, so they get no coordinate from `row_coord` — the west
bringup rows are attributed by other means — and a `[[fixture]]` row would need
a builder kind that can drive `idf-fixtures.sh`, which does not exist. The
coordinate-in-code route reaches the same outcome through a mechanism already
used and tested.
