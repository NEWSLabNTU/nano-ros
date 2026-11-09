---
id: 2367
title: "`nros run` on native_sim takes its link from the host default, not from the locator the image's build baked"
status: open
type: bug
severity: low
area: [cli, run, zephyr]
related: [phase-548, 2301]
found: 2026-11-09
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**
(found fixing issue 2301).

## What

Issue 2301's fix makes the run plan read a Zephyr image's locator from its
build's rendered `zephyr/.config` (`CONFIG_NROS_ZENOH_LOCATOR`, or the XRCE
agent pair) when the board names none. That fallback serves the emulator
roads (mps2-an385-zephyr dials `tcp/10.0.2.2:7447`).

A native_sim image does not reach it: its link comes earlier, from
`decorate`'s host default (`tcp/127.0.0.1:7447`). A user who sets a
different locator in the image's Kconfig is not followed, so `nros run`
starts the router where the image does not dial.

Not measured with a changed locator; found by reading the plan code.

## Done when

A native_sim image whose build baked a non-default locator gets a run plan
that links to that locator (or `nros run` refuses, naming the disagreement),
with a unit test over a fixture `.config`.
