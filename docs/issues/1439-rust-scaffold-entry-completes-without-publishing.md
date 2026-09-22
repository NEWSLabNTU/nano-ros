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
