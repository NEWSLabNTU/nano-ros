---
id: 2270
title: "The default talker's sync and boot print internal issue numbers and a \"contract violation\" a newcomer cannot act on"
status: resolved
type: bug
area: cli, ux, logging
purpose: [education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "issue-2270 branch: the descriptor writer reports defaults once; no tracker id in user text"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E12, first-project.md "Run it": the first `nros sync` (the release's spelling of `nros configure`) prints ``take buffer not derived, so `RX_BUF` keeps the crate default (issue 1233)…`` and ``…stay on the `--config` road (issue 1381)``; at runtime the board prints `[WARN] nros: … contract violation: release-jitter-runtime spin measured=10556 declared=10000`.

On a scaffolded project with nothing changed, these read as errors and name internal tracker ids. Measured on the release; the sync messages' code is still on `main` (`leaf_settings.rs` cites 1381).

## Next

A scaffold's default build is silent unless the user can act; internal ids stay out of user-facing text; the default talker's timing contract matches what the board achieves under QEMU.

## Resolution (2026-11-06)

Maintainer ruling: a summary line plus a scaffold that declares; strip every id
from user-facing text and gate it.

- **Sizing advisories, measured on `main`.** A fresh `nros new ws --board linux
  --template talker` printed six `cargo:warning`s from `nros-node`,
  `nros-rmw-zenoh` and `zpico-sys`, four about one publisher with no stated
  `durability`, two citing tracker ids. The descriptor's WRITER now reports
  them once (`nros_sizing_descriptor::defaults`, RFC-0100 D6 amended), and the
  build scripts no longer repeat a fallback that list covers.
- **The scaffold declares.** `nros new` writes the bringup's
  `system.contract.yaml`: `params: {}`, and on a talker its publisher's
  `durability: volatile`. The talker's build now prints one line,
  `nros: sizing: 1 input uses a default (the defaults are safe and nothing
  needs declaring)`. The remaining input is the unbounded `std_msgs/String`,
  which no declaration bounds. `nros build --explain-sizing` lists it.
- **C warnings.** The same build also printed six warnings from our own C
  sources. Five were `"/*" within comment`, from a `/**` glob written inside a
  block comment. The sixth was `-Wunused-function` for
  `zpico_posix_fifo_set_priority`, dead since issue 0803 removed its callers.
  The globs are reworded and both dead helpers are deleted. The build now
  prints none.
- **Tracker ids.** 86 user-facing string literals in the CLI, the build scripts and the build-helper crates
  cited an id. All 86 are rewritten, each id kept in a comment beside its
  string. Gate `just check user-text-issue-ids` (fast line, self-testing)
  refuses a new one; a comment written into a generated file keeps its id.
- **"contract violation" at boot** (`release-jitter-runtime`) did not
  reproduce on `main`: `nros run` reached `Publishing:` with no warning.
  Round two re-measures it on the next release.
- **Sync's issue-1233 and issue-1381 messages** were already gone from `main`.
