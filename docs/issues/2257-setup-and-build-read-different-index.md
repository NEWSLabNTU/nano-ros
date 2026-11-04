---
id: 2257
title: "`nros setup` provisions from the index it fetches from `main`, while `nros build` and `nros locate` read the release's own pins, so the CLI's remedy loops (QEMU nros8 vs nros7)"
status: open
type: bug
area: cli, setup, index
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I5, installation.md "Provision": `nros setup mps2-an385-baremetal --rmw zenoh` → `nros: fetched the SDK index → /home/dev/.nros/fetch/nros-sdk-index.toml`; it installed QEMU `11.0.0-nros8`, where the release pins `11.0.0-nros7`. The book says setup "reads the index the release ships".
- I6/E5: `nros build` ends with ``note: no qemu-system-arm from $QEMU_SYSTEM_ARM, the project prefix or the store (pinned 11.0.0-nros7) — provision it with `nros setup --tool qemu` ``; that command answers `present 11.0.0-nros8 (skip)`, and the note repeats. `nros locate tool qemu-system-arm` refuses the same way.

`setup.rs` `DEFAULT_INDEX_URL` is `main`'s index by design (RFC-0097 D5: a newer index is usable by an older CLI). The defect is that the build side resolves a different index than setup provisioned from, so following the CLI's advice changes nothing. The same index code is on `main`.

## Next

One index per host for every reader: whatever `nros setup` provisioned from is what `nros build`, `nros locate` and the drift check read. Then decide (RFC-0097 D5) whether a released CLI should follow `main`'s index at all, given issue 2258.
