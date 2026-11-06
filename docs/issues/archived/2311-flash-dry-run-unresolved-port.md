---
id: 2311
title: "`nros flash --dry-run` stops when the port does not resolve, so a reader must pass `--port` just to read the plan"
status: resolved
type: bug
area: [cli, flash]
severity: low
found: 2026-11-06
related: [2290]
resolved: 2026-11-07
resolved_in: "fix-flash-dry-run-port branch: a dry run keeps its plan and marks the port unresolved"
---

## Measured

Found by the phase-537 W9 book session, while writing Flash a real board. On
a scaffolded Nucleo-F767ZI workspace with the board recorded in the bench file
but unplugged, `nros flash --dry-run` exited 3 with the same refusal a real
flash gives. The same happened with two attached devices matching the record,
or an unreadable bench file. With no bench entry at all, the port was left
out of the plan without a word. The book's dry-run step had to pass `--port`
explicitly to get a plan out.

## Code site

`packages/cli/nros-cli-core/src/cmd/flash.rs`: the bench-port resolution ran
before the dry-run branch and returned its refusal for both.

## Resolution

A dry run now keeps the plan when the port does not resolve. It keeps the
bench entry's probe, and prints `# console.port: unresolved` with the reason,
as TOML comments (on stderr under `--json`). A real flash still stops on the
refusal. Tests: `a_dry_run_keeps_its_plan_when_the_port_does_not_resolve` and
`the_dry_run_text_shows_an_unresolved_port`.
