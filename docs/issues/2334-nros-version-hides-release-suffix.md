---
id: 2334
title: "`nros version` prints `0.6.0`, hiding the `-nrosN` release suffix the installer and pin use"
status: open
type: bug
area: cli
purpose: [education, industrial]
related: [phase-523, issue-2328]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E8: the installer says `installed 0.6.0-nros7`, the pin must say `version = "0.6.0-nros7"`, and `nros version` prints `nros 0.6.0`. An instructor checking seats cannot tell `-nros6` from `-nros7` with it. `nros pin` shows the suffix.
- I12: `nros doctor --report` prints `nros 0.6.0` too.

## Next

Print the full release version (the index pin's `version`) in `nros version` and in doctor.
