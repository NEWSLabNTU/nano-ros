---
id: 16
title: threadx-riscv64 build-fixture-extras exits 127 on the maintainer host
status: resolved
type: bug
area: threadx
related: [phase-226]
resolved_in: idlc guard in build-fixture-extras
---

Surfaced by Phase 226: the rc=127 came from the ThreadX-RV64 **Cyclone**
fixtures, which self-provision CycloneDDS from source with `BUILD_IDLC=OFF`
(`cmake/platform/nano-ros-threadx.cmake`) and thus need a host `idlc` from
PATH or the project's `build/cyclonedds/bin/idlc` (`just cyclonedds setup`).
A clean `source ./activate.sh` shell has none; an incremental build dir with
a stale cached idlc path invokes the missing binary as a build-time custom
command → opaque rc 127.

Fixed: `just/threadx-riscv64.just::build-fixture-extras` now guards the
Cyclone block — folds `build/cyclonedds/bin/idlc` onto PATH when present, and
if no `idlc` is resolvable skips the Cyclone fixtures with an actionable hint
(`just cyclonedds setup` / put idlc on PATH /
`NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0`) instead of dying with 127.
`just threadx_riscv64 doctor` reports `idlc` readiness (advisory). Behavior
unchanged when `idlc` is already on PATH.
