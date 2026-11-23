---
id: 2871
title: "An owned entry that never starts nano-ros on the host cannot get a census; is on_missing = \"warn\" the answer, or should the image state it?"
status: open
type: design-question
severity: low
area: [cli, census, zephyr]
found: 2026-11-23
related: [2431, 2806, 2861, 2862]
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
