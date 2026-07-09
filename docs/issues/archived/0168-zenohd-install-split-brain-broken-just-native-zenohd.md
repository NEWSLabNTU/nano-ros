---
id: 168
title: "zenohd install location split-brain — `just native zenohd` broken, three docs give three conflicting launch commands"
status: resolved
type: bug
area: build
related: [rfc-0014, phase-277]
resolved_in: "2026-07-09 UX-audit fix (scripts/dev/zenohd.sh resolver)"
---

## Problem (summary)

zenohd lands in two places depending on setup route — `just zenohd setup` →
`build/zenohd/zenohd` (contributor), `nros setup native --rmw zenoh` →
`~/.nros/sdk/zenohd/<ver>/bin/` (user) — and neither is on PATH by design
(`activate.sh` only exports cross-gcc/genromfs/sccache store dirs). Yet nine
`just` recipes across eight modules invoked **bare `zenohd`**, so
`just native zenohd` (and every platform sibling) failed "command not found"
for anyone following the README, and the three run docs each assumed a
different launch command.

## Resolution

- New sourceable resolver `scripts/dev/zenohd.sh` (`nros_zenohd_bin`,
  mirroring `scripts/dev/clang-format.sh`): prefers the per-checkout
  `build/zenohd/zenohd`, falls back to the newest
  `${NROS_HOME:-~/.nros}/sdk/zenohd/*/bin/zenohd` (`sort -V`), then a PATH
  zenohd, else errors naming both setup routes.
- All nine bare-`zenohd` recipe sites now source it and exec the resolved
  binary: `just/{native,esp32,freertos,threadx-riscv64,threadx-linux,nuttx,
  zephyr-dev,qemu-baremetal}.just` `zenohd` recipes + the
  `qemu-baremetal test-rtic-main-e2e` background launch (its PATH-probe skip
  now uses the resolver too).
- Docs converge on one launch line: root `README.md` (both PATH-hack blocks →
  `just native zenohd`), `examples/README.md` quick-start header + interop
  section, `examples/native/README.md` (corrected the false "activate.sh puts
  zenohd on PATH" claim; its `just native zenohd` quick-start now actually
  works).

Verified: recipe dry-runs parse; resolver returns `build/zenohd/zenohd` on a
provisioned checkout and the newest store version in an SDK-only environment;
live `just native zenohd` boots the router on `tcp/127.0.0.1:7447`.

Residual (out of scope, tracked by #169/#172 doc sweeps): book pages that
still show bare `zenohd --listen …` in prose (`native-posix.md`,
`ros2-interop.md`, `workspace-entry-pkg.md`, `troubleshooting-first-10-min.md`,
`freertos.md`, `rmw-zenoh-protocol.md`) — cosmetic now that the canonical
form is `just <plat> zenohd`.
