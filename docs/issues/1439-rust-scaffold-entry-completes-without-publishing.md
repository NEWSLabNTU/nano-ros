---
id: 1439
title: "The scaffolded Rust entry now opens its session and reports `application
  complete` without ever publishing — issue 1295's `PublisherCreationFailed` is gone
  and a quieter failure is behind it, which the probe's own comment still misnames"
status: open
type: bug
area: tooling, examples, ci
severity: high
found: 2026-09-21
related: [1295, 1357, 0204, 1310]
---

## What happens

Nightly run **35572654294** (schedule, 2026-09-21T07:22), job **106247412496**
(`bootstrap-probe`), section `probe verify: quick-start Rust workspace runtime`.
`nros new … --lang rust`, `nros sync` and `nros build` all succeed — the entry
links and runs — and its entire output is:

```
[INFO] nros: session open
nros: application complete
PROBE FAIL: rust entry exited before publishing
```

`scripts/probe/verify-first-node.sh:83` waits for `Publishing: 1`; the process
exits first. The scaffold's talker is supposed to publish every 500 ms.

## Why this is a NEW finding and not one of the reds it replaced

Two known causes stood in front of it and both have moved:

* **Issue 1357** (`std_msgs — declared by …`) is what this job failed on
  yesterday — run 35496182846, job 106039438587, the same section. Tonight the
  generated `std_msgs` and `builtin_interfaces` crates compile from
  `/tmp/probe_quickstart_rs/generated/`, so the probe now gets past the refusal
  that has blocked it and reaches the runtime assertion for the first time.
* **Issue 1295** (`NodeError::Transport(PublisherCreationFailed)` on
  CycloneDDS) is `status: resolved`, and its signature is absent: the entry does
  not error at the first node, it reports `application complete` — the ordinary
  end-of-run line — after opening the session.

So the entry is not failing to create a publisher. It is running to completion
without doing the work the scaffold's `main` is supposed to loop on.

**The probe's own comment now misnames the red.**
`scripts/probe/verify-first-node.sh:76-78` says "Expected RED until issue 1295:
on CycloneDDS the generated Rust entry fails `PublisherCreationFailed` at
startup". That was true when written and is not true now — 1295 is resolved and
the assertion is failing for a different reason. A stale expectation in the
script is exactly how the next reader attributes a new defect to a closed issue,
which is the 1359-hid-inside-1158 shape. This commit corrects the comment; it
does not touch the assertion, which is doing its job.

## What this is NOT

- **Not a probe defect.** The assertion is the one issue 0204 exists to make:
  it requires the first publication rather than a successful build, and it fired.
- **Not the C arm.** The C quick-start section of the same script passed; only
  the Rust workspace arm fails.
- **Not issue 1310.** That is about a generated workspace entry never being RUN
  on the RMW axis at all. This entry is run, by the probe, and the running is
  what produced the finding.

## What would close it

The scaffolded Rust entry publishing `Publishing: 1` in the probe, on the
CycloneDDS image the book's first-project flow produces. Not measured here, and
worth measuring before assuming a site:

* whether the generated `native_entry` main actually reaches the scaffold's
  timer at all, or returns after registration;
* whether the talker node is registered but its timer never armed — the
  executor's spin would then have nothing to do and complete;
* whether `nros: application complete` is being printed on a path that should
  have been a spin, which would make it a lifecycle question rather than a
  scaffold one.

Acceptance is the `bootstrap-probe` job reaching `PROBE OK` for the Rust arm,
and the script's comment naming whatever the then-current expectation is.

## 2026-09-22 — reproduces, unchanged

Nightly run **35698520560** (schedule, 07:13), job **106650741362**
(`bootstrap-probe`), same section, byte-identical symptom:

```
[INFO] nros: session open
nros: application complete
PROBE FAIL: rust entry exited before publishing
```

`nros new --workspace` scaffolded 15 files (`lang=rust, rmw=cyclonedds`), sync
resolved, and the entry linked and ran. So this is a standing defect in the
book's front-door flow rather than the one-night observation the filing rests
on, and 1357's `std_msgs` refusal remains absent — the generated crates compile
from `/tmp/probe_quickstart_rs/generated/` again.

One line worth keeping from this run, not present in the original: the build
reports `resolved → …/resolved.toml (no count derived; see
[provenance].refused)`. Whether a refused provenance and a talker that never
publishes are the same fact is not established here — it is the first thing to
check, not a conclusion.

## 2026-09-25 — a SECOND lane, in a different workflow, fails identically

Everything above is `nightly.yml`'s `bootstrap-probe`. The `probe.yml` workflow
(cron `0 8 * * *`, landed 2026-09-21 in `a625ef279`) runs the same book flow in
its own container, and its `book probe — checkout track` job fails on this
defect too, byte for byte:

```
PROBE FAIL: rust entry exited before publishing
[INFO] nros: session open
nros: application complete
error: recipe `checkout` failed on line 30 with exit code 1
```

| run | date | job | verdict |
| --- | --- | --- | --- |
| 35703950897 | 09-22 08:16 | 106668267535 | `just: command not found` — NOT this issue, fixed the same day by `9722fca32` |
| 35836219468 | 09-23 08:16 | 107100197079 | this issue |
| 35974416022 | 09-24 08:17 | 107551395041 | this issue |
| 36112086671 | 09-25 08:17 | 107997623085 | this issue |

So **every run of this lane that could reach the assertion has hit it** — three
of four, the fourth predating the lane being runnable at all.

### What the second lane adds, and what it does not

It adds independence. Two workflows, two container images, two schedules, one
`nros new … --lang rust --rmw cyclonedds` scaffold, and the same three lines.
That removes the nightly's environment from the candidate set: whatever makes
the entry reach `application complete` without publishing travels with the
scaffold, not with the runner.

It does not narrow the cause. The 09-25 log shows the entry building to
`Finished dev profile … in 30.60s` and linking `native_entry v0.0.0
(/tmp/probe_quickstart_rs/build/posix-cyclonedds/native_entry)`, exactly as
before, so this is a third observation of the same symptom rather than a new
measurement of it. The 2026-09-22 note's open question — whether the refused
provenance and the silent talker are one fact — is still open and still the
first thing to check.

### Consequence for triage

This lane now has **no signal capacity** (CLAUDE.md, "A red CI lane answers one
of two questions"): a regression landing in `just probe checkout` would look
exactly like tonight. It also means issue 0204's front-door coverage is
reporting nothing about either track while this stands, and the sibling
`installed track` job has been green since the 09-22 `just` fix, so the two
tracks are not failing together.

Acceptance is unchanged, with one addition: **both** probe jobs reaching
`PROBE OK` for the Rust arm — the `nightly` `bootstrap-probe` and `probe.yml`'s
`book probe — checkout track`.
