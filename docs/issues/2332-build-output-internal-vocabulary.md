---
id: 2332
title: "A clean First Project build prints internal terms (census, sizing descriptor, probe sidecar, dangling-entity) and a stale-metadata hint that `nros configure` contradicts"
status: open
type: enhancement
area: cli, ux
purpose: [education]
related: [phase-523, issue-2270, issue-2305]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E6: every `nros run` prints, on a clean first project, `entity-census take …: no contract`, `census check: no contract`, `take buffer not derived … (RFC-0033)`, `sizing descriptor →`, `registration observed for 0 of 0 subscription row(s), from 0 probe sidecar(s)` and `[dangling-entity] warning`.
- `metadata/sensor_node.json: stale against its sources (re-run nros configure)` came with `nros configure` → `up to date`, a contradiction. On the release this was issue 2317's missing probe closure, which is now fixed.
- I2: the MAX_CBS refusal points at `.cargo/config.toml [env]` (a generated file) and cites issue 0257, while the book's remedy is `[image.<id>.knobs.executor] max_cbs` in `system.toml`.

## Next

Issue 2270 quieted the defaults summary. Do the same for the rest: put these lines behind `-v`, drop RFC numbers from user output (issue 2305), and make the MAX_CBS refusal name the `system.toml` knob.
