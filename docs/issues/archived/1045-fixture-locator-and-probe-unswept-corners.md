---
id: 1045
title: "Two unswept corners of fixture resolution: the Zephyr and ThreadX
  locators, and a staleness probe that announces its own degradation only on
  the STALE path"
status: resolved
type: bug
area: testing, build
severity: medium
found: 2026-09-04
related: [issue-1005, issue-1027, issue-0196, issue-0442]
---

## Why this exists

Issues 1005 and 1027 are resolved and archived. Each recorded a corner it did
not reach, and both are about the same machinery — how a test finds a fixture
and decides whether it is fresh — so they are collected here rather than lost
in two archived files.

## What is left

### The Zephyr and ThreadX locators were never swept (from 1027)

1027 fixed five sites across `binaries/nuttx.rs` and `binaries/freertos.rs`,
moving them off a leaf `target/` literal and onto the manifest row
(`groups::select_sole_row` → `groups::row_resolved_dir` for the artifact root,
`row_profile_dir` for the profile). Zephyr and ThreadX resolve differently —
west leaves, `librustapp.d` — and were explicitly out of that sweep.

Being different is not being correct: the class 1027 measured is "the build side
moved to the phase-340 shared group dir and the test-side locator did not", and
nothing has checked whether these two locators are on the right side of it. The
symptom when they are not is the one 1027 measured: a freshly built image
reported `not prebuilt`, with the fallback arm — whose job is to warn about a
real miscompile — firing for a reason that has nothing to do with it.

### A degraded staleness probe is invisible on the FRESH path (from 1005)

`staleness::probe_accounting()` carries the "INPUT SET UNMEASURED" announcement
and is rendered only inside a STALE message. On the FRESH path — the direction
that matters, because that is where a probe that examined nothing reads as a
pass — it says nothing at all.

That asymmetry is what let 1005's symlink defect run unnoticed across the entire
cross-compiled half of the tree: `zpico_recorded_inputs` returned **0 entries**
for every FreeRTOS / NuttX / ThreadX fixture, so the probe silently ran the
hand-authored bootstrap walk its own doc comment calls unreachable, and every
verdict it produced was FRESH.

Same shape as issue 0442/0445: a verdict that explains itself is worth more than
a verdict, and the explanation has to reach the path where the answer is "no
problem here".

## Acceptance

* The Zephyr and ThreadX locators are checked against the same predicate the
  other four now use — attributable to a manifest row, or a stated reason why
  that family cannot be.
* A probe running a degraded input set says so on the FRESH path too, so
  "examined 0 inputs" and "examined 2286 inputs" do not read identically.

## Resolved 2026-09-04 — one half was a defect, the other was a false alarm

### The locators: MEASURED clean, and the reason is one table

The issue assumed the Zephyr and ThreadX locators might be on the wrong side of
phase-340's shared cargo dir. They are not, and the check is cheaper than the
sweep it asked for — `nros_cargo_profile::platform_profile` returns `Some` for
exactly three platforms:

    "freertos" => FREERTOS_QEMU_PROFILE
    "nuttx" | "nuttx-riscv" => NUTTX_RUST_PROFILE
    _ => None

Issue 1027's defect needed BOTH halves: a leaf `target/<triple>/<profile>/`
literal AND a platform whose real profile is a carve-out. `require_prebuilt_binary`
already redirects the artifact ROOT onto the shared group dir, so a literal on a
platform with no carve-out resolves correctly — which is exactly why FreeRTOS was
green before 1027 touched it (it spelled the carve-out directly).

Per family:

* **Zephyr** — not in `NROS_FIXTURE_SHARED_PLATFORMS` at all. West builds into its
  own root and `zephyr_staticlib_dep_file` scans `<root>/rust/target/*/<profile>/`,
  which is where west writes. Structurally outside the class.
* **ThreadX-linux** — already resolves through `groups::select_row`.
* **ThreadX-riscv64** — shared, but no carve-out, so the ambient profile its
  resolver spells is the right one.

**Three sites the issue did not know about were found on the way**, all in
`binaries/mod.rs`, which 1027's sweep did not cover: `build_qemu_test`
(`qemu-arm-baremetal`), `build_contract_monitor_bin` (`linux`) and the esp32
examples (`qemu-esp32-baremetal`). All three spell a leaf literal; all three
resolve, verified on disk against where the builds actually wrote, and
`contract_monitor_parity` demonstrates the redirect working (it reports STALE,
which means it FOUND the artifact).

So there is nothing to fix — and that is the whole hazard. They are one carve-out
away from being 1027 again, exactly as FreeRTOS was one profile spelling away.
Two tripwires now hold that:

* `the_leaf_profile_literals_only_work_because_their_platforms_have_no_carve_out`
  names each site with its platform and fails the day that platform gains a
  carve-out, with the row-route remedy in the message;
* `only_freertos_and_nuttx_carve_out_a_profile` pins the set itself, so the
  tripwire cannot quietly stop covering anything (issue 0196's rule).

### The probe: a real defect, fixed at the choke point

`probe_accounting()` was rendered only inside a STALE message, so on the FRESH
path "examined 0 inputs" and "examined 2286 inputs" read identically — and both
read as a pass. `record_fresh` now returns `Result` and announces a degraded
probe, with two shapes distinguished because they are different failures:

* `examined == 0` — the probe compared NOTHING; a fresh verdict is the absence of
  a measurement, not evidence.
* `UNMEASURED` — an arm fell back to a hand-authored list, so the verdict
  describes a different input set than the one that was built. This one fails
  safe, and still must not be silent.

Put in `record_fresh` rather than at the four `require_*_fresh` entry points on
purpose: every one of them ends there, so the fifth cannot forget it.

`NROS_STRICT_STALENESS_PROBE=1` turns the warning into a failure. Off by default
because making it fatal today would break every arm with no recorded input set —
a much larger change than making the degradation visible — and the knob is wired
through all six call sites rather than left as a constant nobody reads, which is
this repo's own recurring "correct and unreachable" failure.

Three unit tests, mutation-checked (forcing `fresh_verdict_warning` to `None`
fails two of them): the zero-examined case, the unmeasured-with-candidates case
that `examined > 0` alone would miss, and a NEGATIVE control asserting a healthy
probe stays SILENT — a warning that fires on every run is one a reader learns to
skip.

## Found while, filed separately

`ci_lane::tests::recipes_run_the_scope_their_lane_declares` is RED on `main`
(issue **1057**) — verified pre-existing by stashing this work. It is invisible
on pull requests, because `check-fast` runs no unit tests, and fails in the merge
queue instead.
