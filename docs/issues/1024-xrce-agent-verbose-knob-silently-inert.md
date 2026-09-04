---
id: 1024
title: "`NROS_XRCE_AGENT_VERBOSE` passes `-v6` to an Agent built with the logger
  compiled OUT, so the knob is silently inert"
status: open
type: bug
area: testing, rmw-xrce
severity: low
related: [issue-0741]
found: 2026-09-04
---

## What was measured

Found while diagnosing issue 0741. The test harness honours
`NROS_XRCE_AGENT_VERBOSE` by passing `-v6` to the Micro XRCE-DDS Agent, but
`build/xrce-agent` is configured with `UAGENT_LOGGER_PROFILE=OFF`. With the
logger compiled out, `-v6` is accepted and produces **no output at all**.

## Why it is worth a row

The knob is the one thing a person reaches for when an XRCE test fails and the
symptom is opaque — which is exactly the situation 0741 sat in for five
sessions. Turning it on and seeing nothing is not neutral: it reads as "the
Agent had nothing to say", which is evidence, and it is false. A diagnostic that
answers "silence" when it means "I was compiled without a voice" is the same
shape as the vacuous-gate class this tree keeps finding — a check that cannot
reach what it claims to report.

## Options

* Build the fixture Agent with `UAGENT_LOGGER_PROFILE=ON` (cost: a rebuild, and
  some log volume when the knob is off — the profile gates compilation, the `-v`
  level gates emission, so an ON build stays quiet by default).
* Or make the harness REFUSE the knob against a logger-less Agent, naming the
  build option. Cheaper, and it converts a silent lie into a sentence.

The second is the minimum; the first is what makes the knob useful.

## Acceptance

* [ ] `NROS_XRCE_AGENT_VERBOSE=1` either produces Agent logs, or fails with a
      message naming `UAGENT_LOGGER_PROFILE`.
