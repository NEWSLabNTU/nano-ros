<!--
Three questions. Please answer them in prose — they are phrased so that a
wrong answer is possible, which is the whole point. A checkbox everyone ticks
carries no information; a sentence that turns out to be false is a thing a
reviewer can catch.

New here? Read CONTRIBUTING.md. The short version: `just ci-l1` (~6 min, no
SDK, no QEMU, no fixtures) is the entire verification obligation for an
outside contributor.
-->

## What this changes, and why

<!-- One or two paragraphs. Link the issue: "Fixes #NNNN" / "Refs #NNNN". -->

---

## 1. Which lanes did you run, and what was the result?

<!--
Name the verb, verbatim, and paste or paraphrase how it ended. For example:

    just ci-l1 — passed (5m51s). check-rmw-cyclonedds skipped: submodule not
    initialised. check-required-features-tests reported [SKIPPED:capability]
    (no zenoh router on this host).

"CI passed" is not an answer to this question — it names no lane. There are
seven (L0 source through L6 hardware), `just ci-l1` covers the first two, and
a green there says nothing about the other five.

If a sub-lane SKIPPED, say so here. Skips are legitimate and expected on a
fresh clone; letting them read as passes is not.
-->

## 2. What could you NOT verify, and why?

<!--
Everything above your ceiling belongs here. Be concrete about the gap between
what your change claims and what you actually observed.

An outside contributor cannot run the cross-build, QEMU, live-ROS-2 or
hardware lanes at all — no SDK, no QEMU, no router. That is expected; say it.
"I changed a Zephyr code path and could not build for Zephyr" is a genuinely
useful sentence, and it tells a reviewer exactly where to look.

"Nothing" is a valid answer only if it is true.
-->

## 3. Was this authored with AI assistance?

<!--
Yes / no / partly, and roughly which parts.

**This is not a filter.** nano-ros is itself largely agent-built; the answer
changes nothing about whether the change is accepted, and there is no bar that
AI-assisted work has to clear and human work does not.

What it changes is review EMPHASIS. The characteristic failure mode of agent
work is confident-and-wrong rather than broken — code that compiles, passes
every gate, and rests on a false premise. Three retractions in one session in
this repo (a wrong root cause, an over-broad staleness rule, an assertion whose
only claim was `min(a, b) <= b`) all passed every gate they were subject to.
So when the answer is yes, a reviewer reads the REASONING and not only the
diff, and is likelier to ask "how do you know?" about a claim that looks
settled. Answering honestly gets you a better review, not a slower one.
-->

---

<!--
Before submitting:

- [ ] `git commit -s` — DCO sign-off on every commit (see CONTRIBUTING.md).
- [ ] `just format` run, and the diff contains only paths you meant to change.
- [ ] One issue per PR; incidental fixes split into their own PR.
- [ ] If this touches `build.rs`, `justfile`, `just/**`, `.github/**` or any
      script CI executes, say why above — those get read differently.
-->
