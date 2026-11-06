---
id: 2257
title: "`nros setup` provisions from the index it fetches from `main`, while `nros build` and `nros locate` read the release's own pins, so the CLI's remedy loops (QEMU nros8 vs nros7)"
status: resolved
type: bug
area: cli, setup, index
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "issue-2257 branch: a release reads its shipped index (RFC-0097 D5 amended)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I5, installation.md "Provision": `nros setup mps2-an385-baremetal --rmw zenoh` → `nros: fetched the SDK index → /home/dev/.nros/fetch/nros-sdk-index.toml`; it installed QEMU `11.0.0-nros8`, where the release pins `11.0.0-nros7`. The book says setup "reads the index the release ships".
- I6/E5: `nros build` ends with ``note: no qemu-system-arm from $QEMU_SYSTEM_ARM, the project prefix or the store (pinned 11.0.0-nros7) — provision it with `nros setup --tool qemu` ``; that command answers `present 11.0.0-nros8 (skip)`, and the note repeats. `nros locate tool qemu-system-arm` refuses the same way.

`setup.rs` `DEFAULT_INDEX_URL` is `main`'s index by design (RFC-0097 D5: a newer index is usable by an older CLI). The defect is that the build side resolves a different index than setup provisioned from, so following the CLI's advice changes nothing. The same index code is on `main`.

## Next

One index per host for every reader: whatever `nros setup` provisioned from is what `nros build`, `nros locate` and the drift check read. Then decide (RFC-0097 D5) whether a released CLI should follow `main`'s index at all, given issue 2258.

## Resolution (2026-11-06)

Root cause shared with issues 2256–2258: `nros setup` on a release read
`main`'s fetched index, while every other reader (`qemu_bin::for_sdk`, the
build's source preflight, `nros locate`, the drift check) read the release's
own SDK root, about 20 sites. The maintainer ruled that a release uses its own
index, which amends RFC-0097 D5. `setup::release_reads_shipped` makes a
released `nros` answer the store rung with its shipped
`share/nros/nros-sdk-index.toml`, the same file the SDK root carries, and
fetch nothing unless `$NROS_INDEX_URL` names a mirror. A binary with no shipped
copy keeps the fetch path. Unit-tested; round two re-measures it on the
release that carries the change.

With one index, `nros setup --tool qemu` provisions the pin that `nros build`
and `nros locate` ask for, so the printed remedy no longer loops.
