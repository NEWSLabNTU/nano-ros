---
id: 2271
title: "`install.sh` no longer ends with the `export PATH` line the install page tells the reader to copy"
status: open
type: bug
area: book, install
purpose: [education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E13, installation.md "Install the nros CLI": the page says install.sh prints the `export PATH` line; its output ends with `nros-install: next → nros setup <board>`. The reader copies the line from the page instead.

`main`'s install.sh prints the export only on one branch (`scripts/install.sh` ~271), then the `next →` line.

## Next

install.sh always prints the PATH line when the bin dir is not on PATH (as its last instruction), or the page says when it does not.
