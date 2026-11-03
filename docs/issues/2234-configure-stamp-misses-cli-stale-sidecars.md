---
id: 2234
title: "`nros configure` reports up to date while every probe sidecar is stale against a rebuilt CLI"
status: open
type: bug
area: [cli, configure, sizing]
found: 2026-11-03
related: [issue-1578, phase-530, rfc-0120]
---

## Symptom

Measured on `examples/workspaces/rust` during phase-530 W3, after
`just setup-cli` rebuilt the CLI from edited sources:

- `nros build demo_bringup:zephyr` printed `configure: up to date (1 image(s))`;
- the same run's resolve then printed, for every component,
  `<pkg>/metadata/<comp>.json: stale against its sources (re-run nros
  configure), so neither its counts nor its registration observations are
  used`, and the image's resolve fell to `entity counts not derived;
  configured defaults kept`.

So the build silently sized the image from defaults where the previous build
of the same tree derived its counts. `nros configure --force` re-probed and
restored them.

## Cause

Two freshness rules for one sidecar:

- the resolve reads a sidecar only when its recorded key equals
  `metadata_refresh::probe_inputs_key`: the package's source digest, the
  CLI's `NROS_CLI_SOURCE_STAMP`, and the probe closure digest;
- the configure stamp (`configure_stamp::currency`, through
  `stale_sidecar_components`) compares only the package's source-digest
  PREFIX of that key.

A rebuilt CLI (or an edit under `probe-source-dirs.txt`) moves the second or
third part, which the stamp never looks at, so the stamp stays current while
the resolve refuses every sidecar.

## Fix direction

Compare the whole key in `stale_sidecar_components` (keeping Ruling R5's
`UNKEYED` exemption), so one rule decides freshness for both readers.
