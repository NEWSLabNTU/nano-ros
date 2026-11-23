---
id: 2871
title: "An owned entry that never starts nano-ros on the host cannot get a census; is on_missing = \"warn\" the answer, or should the image state it?"
status: resolved
type: design-question
severity: low
area: [cli, census, zephyr]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix/issue-2871-no-nano-ros-entry"
related: [2431, 2806, 2861, 2862, phase-478]
---

## What happened

The safety island (autoware-safety-island fork, branch `nano-ros-next` at
f6c8bef, nano-ros pin 8d4e70a382) moved its FVP CAN phase onto `nros run`.
That gave the CAN output test its own image:

- bringup `can_output_test_bringup`, image `fvp-aemv8r-smp`;
- a hand-written owned entry in the package
  `src/asi_tests/asi_can_output_test_zephyr`
  (`nano_ros_add_executable` in the west app's own CMakeLists.txt, issue 2861).

The program drives the CAN device directly. It never initialises nano-ros, on
the host or on the target. Since issue 2862 an owned entry is sized by a
census, so the configure runs the host census build, which exits 0 and writes
nothing. The configure then stops with the refusal in
`packages/cli/nros-cli-core/src/cmd/entity_census.rs`:

    `<binary>` exited 0 in census mode but wrote no census at `<path>` -- the
    image is built without `metadata-mode` (a C/C++ native umbrella) or without
    `nros-board-linux/census` (a Rust entry) -- both funnels say so and exit
    non-zero, so a zero exit with no file is a binary with no census funnel at all

The island got past it by setting `[census] on_missing = "warn"` on that
bringup. Its CI run 45099337935 passed on all three jobs.

## Why this may be deliberate

"Exited 0, no census" cannot tell these two cases apart:

1. the program uses no nano-ros entities, so an empty census is the truth;
2. the census writer was never reached (no funnel linked, or an early return),
   so an empty census would size the image wrongly.

Taking an empty census silently would hide case 2. So the refusal may be the
right default.

## The question: which answer?

- **(a) The image states it.** A `[census]` or entry field says "this entry
  runs no nano-ros". The configure then takes an empty census without running
  the binary, and the refusal stays for every entry that does not say so.
- **(b) `on_missing = "warn"` stays the answer.** It gets documented as the
  way to build a test-only entry that never starts nano-ros, with what it
  costs: under warn, issue 2806 refuses every count the census would state.

This issue leaves the choice open for the census owner.

## Evidence

- ASI commit f6c8bef (`ci(fvp): the CAN and statistics phases on nros run`),
  bringup `can_output_test_bringup`, entry package
  `src/asi_tests/asi_can_output_test_zephyr`.
- ASI CI run 45099337935 (with the warn opt-out).

## Resolved, 2026-11-23

Ruled (a) by the user, 2026-11-23: the image states it, and the refusal stays
for every image that does not.

- **The field** is `[image.<id>] runs_nano_ros = false`, on the image and not
  in `[census]`: the census of an owned entry is keyed by bringup and image,
  and the statement is about the one program that image builds, so a bringup
  with several images states it for exactly the ones it means. It sits beside
  `producers` and `no_inventory`, is never folded from `[image_defaults]`,
  and `true` is the default and changes nothing.
- **The configure** (`nros dev image entity-census take --image`, which the
  CMake road's root and the Zephyr module hook both call) neither builds nor
  runs the program. It records an empty census at the usual
  `owned/<bringup>/<image>.census.json`: the recorder schema version, no
  nodes, provenance naming the package's `CMakeLists.txt` and `src/` so a
  reader checks it fresh as for a taken one, and a `stated` line naming the
  statement, with its inputs by absolute path (the configure stamp checks a
  census against the workspace, not the package). A past `.not-taken`
  record is retired. The image is then sized from a census-backed inventory
  marked empty BY STATEMENT, which derives instead of refusing: zero demand
  from the program's own entities, plus the runtime's families the bringup's
  features imply, as for any image. The real empty answer, neither issue
  2806's "census not taken" refusals nor the "no components were registered"
  refusal an empty inventory nobody stated still gets. A derived zero is
  demand, floored at the pool: `nros-rmw-cffi` floors a derived
  `node_count` of zero at one, the smallest legal size of its node table
  (an explicit `NROS_RMW_MAX_NODES` is still range-checked as written).
- **Misuse is refused.** `runs_nano_ros = false` on an image that is not sized
  by an owned entry's census (a host image, a generated entry, or an entry
  in a bringup that declares `[[component]]` rows) is refused by the take and
  by `nros build`'s resolve, since those programs run nano-ros. An entry that
  states it and DOES create entities is not detected: detecting it needs the
  run the statement exists to skip. Its pools are sized for zero entities,
  so such an image fails at boot on its first entity, a loud failure rather
  than a silent one.
- **Unchanged:** an entry that does not state it and exits 0 in census mode
  with no census still refuses with the same message; the owned-entry wrapper
  of that refusal now names `runs_nano_ros = false` as the remedy for a
  program that never initialises nano-ros. `[census] on_missing = "warn"`
  keeps its meaning (issue 2806).

Tests: `entity_census::tests::an_entry_stating_it_runs_no_nano_ros_gets_an_empty_census_unrun`
(no host step runs; the census is fresh, empty and census-backed),
`an_unstated_entry_that_writes_no_census_still_refuses` (the negative
control), and `build::not_derived_verdict_tests::runs_no_nano_ros_on_an_image_without_an_owned_census_is_refused`.
