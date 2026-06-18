---
id: 85
title: threadx-riscv64 cyclonedds C fixtures fail to link — duplicate `ddsrt_setsockreuse`
status: open
type: bug
area: threadx
related: [phase-186, phase-251, issue-0084]
---

## Problem

After issue 0084 (duplicate `stderr`) was fixed, `just threadx_riscv64
build-fixture-extras` still fails — now on the **cyclonedds** C fixtures
(`threadx-riscv64-c-cyclonedds-all`, e.g. `riscv64_threadx_c_talker`):

```
rust-lld: error: duplicate symbol: ddsrt_setsockreuse
make: *** [.../threadx-riscv64-c-cyclonedds-all-*.mk:10: fixture-0000] Error 1
```

The zenoh C fixtures and the Rust `logging-smoke` fixture link fine; only the
cyclonedds variants fail.

## Root cause

The vendored CycloneDDS fork's ThreadX ddsrt port redefines a function the
generic ddsrt already provides:

- `third-party/dds/cyclonedds/src/ddsrt/src/sockets.c:445` —
  `ddsrt_setsockreuse(...)` (generic, compiled unconditionally; `ddsrt/CMakeLists.txt:99`).
- `third-party/dds/cyclonedds/src/ddsrt/src/sockets/threadx/socket.c:168` —
  `ddsrt_setsockreuse(...)` (the Phase-186 ThreadX port, added under `WITH_THREADX`;
  `ddsrt/CMakeLists.txt:117`).

Both are compiled into `libddsc.a` for the threadx build, so they collide.
The **posix** port correctly does NOT redefine `ddsrt_setsockreuse` (it relies on
generic `sockets.c`) — the ThreadX port is the buggy one. Same `--allow-multiple-definition`
removal (phase-251) that exposed 0084 exposes this.

## Fix prepared (2026-06-18) — awaiting fork push + superproject bump

Done in the cyclonedds submodule on branch **`nano-ros`** (commit `1ca48131`):
removed the redundant `ddsrt_setsockreuse` from `sockets/threadx/socket.c`
(replaced with a comment pointing at the generic `sockets.c`). Verified the
generic `sockets.c:445` provides it (SO_REUSEPORT → SO_REUSEADDR fallback; NetX has
no SO_REUSEPORT → SO_REUSEADDR, identical to the removed port code);
`ddsrt/CMakeLists.txt` compiles `sockets.c` unconditionally + the threadx port
under `WITH_THREADX` → collision gone with the port copy removed.

**Per the vendored-fork workflow, the agent did NOT push the fork.** Remaining
(maintainer): push `origin/nano-ros` (NEWSLabNTU/cyclonedds) → then bump the
superproject submodule pointer to `1ca48131`. Runtime build-verify
(`just threadx_riscv64 build-fixture-extras`) is maintainer/CI-side (riscv64 cross
toolchain).

## Fix (original analysis)

Remove the redundant `ddsrt_setsockreuse` definition from the ThreadX port
(`sockets/threadx/socket.c`); the generic `sockets.c` one (which calls
`ddsrt_setsockopt(SO_REUSEADDR)`, identical behaviour) serves it — exactly as the
posix port relies on it.

**Vendored-fork workflow:** the edit lands on the cyclonedds fork branch
(commit in submodule → rebase → maintainer pushes the fork → bump the superproject
pointer). The agent does not push fork remotes by default, so the submodule fix is
prepared locally and left ready for the maintainer to push + bump.
