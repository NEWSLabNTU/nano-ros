# Phase 414 — RTOS runtime correctness: the e2e failures that reproduce SOLO

**Status (2026-09-03). Opened as a HOME, not as a plan.** Five open issues had
no phase that could hold them, and the survey that found that is the whole
reason this doc exists — an issue with no home is an issue nobody is
accountable for, the same shape as a gate that sits in a lane no CI job runs.
No work item here has been started.

## Why these five are one phase, and why the two neighbours could not take them

They are RUNTIME failures on a real RTOS: the image builds, links and boots, and
then does the wrong thing. That is a different activity from either neighbour:

* **[phase-349](phase-349-rtos-integration-shells.md)** is BUILD integration —
  "make FreeRTOS an imported library like the rest". It is about how the RTOS
  enters the build, not about what the image does afterwards.
* **[phase-358](phase-358-embedded-runtime-under-load.md)** is footprint,
  overrun and overload — failures that appear when you PUSH the runtime. These
  five fail at rest.

The distinguishing property, and the reason they are worth grouping: **each
reproduces SOLO.** CLAUDE.md's standing advice for a QEMU red is to retest it
alone before believing it, because full-sweep lanes flake under load. These
already survive that test, so they are not the flake class — they are defects
with a stable reproduction and no owner.

## Work items

Each is an existing issue. The item is "close it"; the issue holds the evidence.

* **W1 — [issue 0877](../issues/0877-freertos-pubsub-passes-by-hand-fails-under-harness.md),
  FreeRTOS pubsub delivers NOTHING under the test.** The most severe: a
  transport that hand-delivers and then delivers nothing at all. Start here —
  a platform that cannot pass its own pubsub e2e makes every other FreeRTOS
  result unreadable.
* **W2 — [issue 0867](../issues/0867-nuttx-c-action-goal-send-times-out.md),
  `test_rtos_action_e2e` nuttx/C fails 3/3 SOLO.** Explicitly not the load
  flake; three of three alone.
* **W3 — [issue 0870](../issues/0870-nuttx-cpp-action-client-transport-tx-failed.md),
  NuttX C++ `create_action_client` fails.** Likely shares a cause with W2 —
  same platform, same entity family, different language binding. Do them
  adjacently and say whether the cause was shared; if it was, that is one fix
  and the phase shrinks.
* **W4 — [issue 0847](../issues/0847-xrce-entity-drop-after-session-close.md), an XRCE
  publisher outliving `executor.close()` segfaults in its own Drop.** A
  lifetime/ordering defect, not a transport one. It is the only one here whose
  fix is likely in our own Rust rather than in a vendored stack.
* **W5 — [issue 0741](../issues/0741-xrce-service-reply-history-payload-too-small.md),
  `test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the
  request.** INTEROP with a real ROS 2 peer, so it may belong with
  [phase-303](phase-303-xcdr2-interop.md) instead; it sits here because it is
  an XRCE service failure like W4's neighbourhood and because 303 is about
  XCDR2 encoding specifically. **Move it if the cause turns out to be
  encoding.**

## Acceptance

* Each of the five is resolved, or reassigned to a phase that fits better with
  the reason recorded.
* For W2/W3, an explicit statement of whether the cause was shared — that
  answer is worth more than either fix.

## What this phase deliberately does NOT do

It does not add tests, lanes or gates. Every one of these already has a failing
test; the problem is that nothing was accountable for making them pass.
