---
id: 1024
title: "`NROS_XRCE_AGENT_VERBOSE` passes `-v6` to an Agent built with the logger
  compiled OUT, so the knob is silently inert"
status: resolved
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


## Resolved 2026-09-05 — already fixed, by issue 0741's work, before this was filed

Both halves of the acceptance below are on main, and neither is mine. Filing
this was a duplicate: I hit the symptom while diagnosing 0741 and wrote it up
without checking whether 0741's own remedy had already landed. It had.

**The build side.** `scripts/xrce-agent/build.sh` derives `logger_profile` at
file scope so BOTH build paths get it (the ROS-paired one and the bundled
fallback — deriving it inside the paired branch left the fallback's
`-DUAGENT_LOGGER_PROFILE=` expanding to nothing). Default stays `OFF` because
tracing is not free, and `NROS_XRCE_AGENT_LOGGER=1` turns it on:

    logger_profile="OFF"
    if [ -n "${NROS_XRCE_AGENT_LOGGER:-}" ] && [ "${NROS_XRCE_AGENT_LOGGER}" != "0" ]; then
        logger_profile="ON"
    fi

Crucially the choice is part of the freshness key —
`want="$agent_ref $ros_prefix logger=$logger_profile"` — so flipping the
variable REBUILDS rather than silently reusing the other flavour. That is the
half a plain env read would get wrong.

**The harness side.** `fixtures/xrce_agent.rs` says so at the moment the flag is
used, which is where a reader is standing when the log comes back empty:

    [xrce-agent] -v6 requested. If the log is EMPTY, this agent was built
    without its logger profile: rebuild with `NROS_XRCE_AGENT_LOGGER=1
    just xrce setup` (the stamp records the choice, so it will actually
    rebuild).

Its comment names the exact failure this issue describes — an empty log reading
as "the agent had nothing to say" rather than "this binary cannot say anything"
— and records that the misreading cost 0741 its only non-root instrument for
weeks.

**How this was verified:** by reading. The `eprintln!` is unconditional inside
the `if verbose` branch, so it fires whenever the knob is set; and `logger=` is
in the freshness string, so the rebuild follows. It was NOT observed at runtime:
the xrce talker fixture is stale against current main, and rebuilding the native
lane to watch one `eprintln!` is disproportionate. Stated rather than glossed,
because "verified by reading" and "verified by running" are different claims.

**The lesson is the filing, not the fix.** This is the eleventh issue this
session that duplicated work already done or in flight elsewhere. Every one
surfaced only when something forced a comparison — a merge conflict, a failing
gate, or reading the code before starting. Checking the code first would have
cost a minute and saved the row.

## Acceptance

* [x] `NROS_XRCE_AGENT_VERBOSE=1` either produces Agent logs (build with
      `NROS_XRCE_AGENT_LOGGER=1`) or prints a message naming the build option
      and the exact rebuild command.
