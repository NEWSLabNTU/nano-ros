---
id: 512
title: "`check-readiness-marker-literals` is blind to the WORST case — a grep
  literal that matches no marker at all"
status: open
type: bug
area: testing
related: [issue-0481, issue-0489, phase-342]
---

## The gap

`scripts/lib/readiness_marker_check.py:57`:

```python
if not (exact or len(prefix) >= 2):
    continue
```

A `wait_for_output_pattern` literal is reported when it **matches a known
`output::` constant exactly**, or when it **ambiguously prefixes two or more**.
A literal matching **nothing** is skipped.

That is backwards with respect to severity:

| literal | gate | what actually happens at runtime |
| --- | --- | --- |
| matches a constant | reported | fine — it is the right marker, just spelled out |
| ambiguously prefixes ≥2 | reported | matches whichever the process prints |
| **matches nothing** | **silent** | **never matches — the wait burns its whole timeout** |

The unflagged case is the only one guaranteed to fail.

## It has already cost real time

Issue 0489: `esp32_emulator.rs:211` and `:402` waited on the literal
`"Waiting for messages..."`. No listener prints that any more — phase-342 W7
converged every example onto `LISTENER_READY_MARKER` — so each test burned its
full 60 s and reported "ESP32 listener failed to start", blaming the fixture for
a stale string in the harness. **108 s of a 137.9 s suite.**

The gate was green throughout, and reported `OK (32 baselined, 0 new)` on the
unfixed tree, because `"Waiting for messages..."` matches no current constant.

## Why it is not a one-line fix

Flagging every literal that matches no constant would fire on every legitimate
ad-hoc pattern, and the suite has many — `"Publishing:"`-style delivery greps,
QEMU boot strings, `"data:"` from a ROS 2 CLI subscriber. A gate that fires on
correct code gets switched off within a week.

What is needed is a way to separate "stale readiness marker" from "deliberate
ad-hoc pattern". Options, none costed yet:

* **A baseline of allowed ad-hoc literals**, the shape the current gate already
  uses for its 32 (now 0) baselined sites. Cheap, and it makes the intent
  explicit at each site; costs a one-time sweep.
* **Restrict the rule to readiness waits specifically** — e.g. only literals
  passed to a `expect_ready`-adjacent call, so delivery/boot greps are out of
  scope by construction. Narrower, no baseline, but needs the call sites to be
  distinguishable.
* **Assert liveness instead**: a wait that times out reports which patterns the
  process DID print. Catches the class at runtime rather than statically, and
  complements rather than replaces the gate.

## Note

The baseline currently reads `0 baselined, 0 new` — all 32 sites have since been
converted, so the gate's *backlog* arm is done. This gap is in what it looks at,
not what it has left to fix.

Recorded as its own issue because it currently lives only inside archived issue
0489's write-up, where nobody looking for gate coverage would find it.
