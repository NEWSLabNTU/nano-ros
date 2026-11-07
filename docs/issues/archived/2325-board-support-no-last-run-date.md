---
id: 2325
title: "Board pages and `nros board show` give a verification cadence, but not the date or result of the last run"
status: resolved
type: enhancement
area: book, support-status
purpose: [industrial]
related: [phase-523, issue-2267]
found: 2026-11-07
resolved_in: "adoption-r2-provenance branch: a release records each verifying lane's last run; `nros board show` and `nros doctor --report` print it"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I8: `nros board show mps2-an385-baremetal` and `nros doctor --report` say `Verified in emulator (daily) … cadence daily`. No page or command gives the last run's date, its result or a link to it.
- The same release's First Project did not compile on that board (issue 2317), so "daily" did not cover the user's road.

## Next

Stamp each release's support table with the date, commit and result of the last verifying run, and make the daily lane run the book's First Project (the `probe=` blocks) from the release asset.

## Resolution (2026-11-07)

- At release time, `scripts/release/record-lane-runs.py` records the newest run
  that reached a verdict for each cadence: daily is `nightly.yml`, weekly is
  `probe-platforms.yml`, merge is `queue.yml`, the same map
  `check-board-tiers.py` enforces. It stores the date, the conclusion, the
  commit, the URL and each job's conclusion in the SDK root's
  `packages/boards/board-support-runs.json`.
- The status file now carries each row's `nightly_token` and probe tracks, so
  a row names its own job. The nightly sweep's `qemu` job is the
  mps2-an385 bare-metal row's evidence, not the whole run. Measured on
  2026-11-07: `nightly.yml` 2026-11-06 was `failure` overall, while its `qemu`
  and `freertos` jobs were `success`.
- `nros board show` and `nros doctor --report` print a `last run` line. A
  checkout says it records none. Test: `a_row_names_its_lanes_last_run`.
