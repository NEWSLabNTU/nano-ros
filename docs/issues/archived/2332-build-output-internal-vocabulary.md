---
id: 2332
title: "A clean First Project build prints internal terms (census, sizing descriptor, probe sidecar, dangling-entity) and a stale-metadata hint that `nros configure` contradicts"
status: resolved
type: enhancement
area: cli, ux
purpose: [education]
related: [phase-523, issue-2270, issue-2305]
found: 2026-11-07
resolved_in: "adoption-r2-cli-ux branch: build detail behind -v, the talker scaffold declares its external reader, the MAX_CBS refusal names the system.toml knob"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E6: every `nros run` prints, on a clean first project, `entity-census take …: no contract`, `census check: no contract`, `take buffer not derived … (RFC-0033)`, `sizing descriptor →`, `registration observed for 0 of 0 subscription row(s), from 0 probe sidecar(s)` and `[dangling-entity] warning`.
- `metadata/sensor_node.json: stale against its sources (re-run nros configure)` came with `nros configure` → `up to date`, a contradiction. On the release this was issue 2317's missing probe closure, which is now fixed.
- I2: the MAX_CBS refusal points at `.cargo/config.toml [env]` (a generated file) and cites issue 0257, while the book's remedy is `[image.<id>.knobs.executor] max_cbs` in `system.toml`.

## Next

Issue 2270 quieted the defaults summary. Do the same for the rest: put these lines behind `-v`, drop RFC numbers from user output (issue 2305), and make the MAX_CBS refusal name the `system.toml` knob.

## Resolution (2026-11-07)

Issue 2270's approach, extended: a fact a user must act on prints, the
reasoning behind a correct build is detail.

- `build_notes` is the Rust twin of `nros_message_detail`. `nros build` and
  `nros run` call `enter_driver`; from there a detail line prints only under
  `-v` or `NROS_VERBOSE=1`. `nros run` gained `-v`, and hands it to the build
  it starts. A verb typed by hand (`nros ws entity-census take`) keeps its
  notes, because there the note is the answer.
- Detail now: `entity-census take …` (host producer, no contract, fresh),
  `census check: no contract`, a clean `census check: N confirmed …` report,
  `sizing descriptor →`, the `registration observed …` tally, and every
  "payload classes / take buffer / pool budgets not derived" line (the sizing
  summary line already counts them; `--explain-sizing` lists them). A
  census check with warnings or errors still prints, and a skipped probe
  sidecar still names its remedy. On the cmake road a clean check is
  `nros_message_detail`; one with warnings stays a STATUS line.
- `RFC-0033` is gone from the three "carry no derived bound" reasons.
- `[dangling-entity] warning` came from the talker scaffold's contract,
  which declared `/chatter`'s publisher and no reader. Its reader is outside
  the workspace (`ros2 topic echo`), so the scaffold now says
  `external: sub`.
- The MAX_CBS refusal (`nros::main!` and `codegen-system`) names
  `[image.<id>.knobs.executor] max_cbs = <n>` in the bringup's `system.toml`
  instead of `.cargo/config.toml [env]`, and the sibling `max_callbacks`
  refusal drops its tracker id too.

Measured on a fresh `nros new sensor_ws --board linux --template talker`: the
default build prints no census, descriptor, registration or take-buffer line
and no resolver warning; `nros build -v` prints the take-buffer line. The
stale-metadata contradiction was issue 2317, fixed before this. The cross
(QEMU) image's census lines are covered by the same calls, not re-measured on
a QEMU build here.
