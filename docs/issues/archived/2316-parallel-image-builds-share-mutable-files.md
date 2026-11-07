---
id: 2316
title: "Parallel image builds of one workspace race on two shared files: the riscv64 link wrapper's archives, and `nros-sdk.lock`"
status: resolved
type: bug
area: [build, cli, threadx]
severity: medium
found: 2026-11-07
related: [1845, 0850, 0805]
resolved: 2026-11-07
resolved_in: "phase-535 W5 (threadx-riscv64) PR: the link wrapper takes a per-build-dir flock; the project lock is written atomically under a directory flock"
---

## Measured

Found by phase-535 W5, the first time a threadx-riscv64 workspace held more
than one image per configure. `workspace-fixtures-build.sh threadx-riscv64`
runs one `nros build` per row, in parallel. Two failures came out of that,
both from files every image of one workspace shares:

1. **C, one configure, several executables.** Linking
   `riscv_threadx_service_server_entry` died with `Bus error` and an LLVM
   "PLEASE submit a bug report" banner, while
   `riscv_threadx_service_client_entry` linked in the same second. Both
   link steps ran `cmake/toolchain/riscv64-lld-wrapper.sh`, which rewrites
   every `.a` argument IN PLACE (strip soft-float objects, localize mem
   symbols, restore a memo by `cp`). The archives (`libnros_cpp.a`,
   `lib<pkg>__nano_ros_c.a`, ...) belong to the configure, so both links
   rewrote the same files, and one tool's mapped archive was truncated under
   it by the other's write: SIGBUS. Before W5 each riscv64 configure built one
   executable, so no two links ever shared an archive.
2. **Rust, twelve `nros build`s of one workspace.** Seven of the twelve
   failed with `Error: invalid nros-sdk.lock` / `TOML parse error at line
   195`. Every build ends in `project_lock::record`, a load -> merge -> save of
   the workspace's one `nros-sdk.lock`, and `SdkLock::save` was a plain
   `fs::write`. Two saves of different lengths interleaved, leaving a short
   write over a long file (the tail `2.1"` of a longer save), and every later
   load refused it. The torn file PERSISTS: the next run failed the same 12
   rows before any of them recorded anything.

## Resolution

- `riscv64-lld-wrapper.sh` re-execs itself under `flock` on
  `$PWD/.nros-riscv-link.lock` (the link runs from the build directory, which
  is exactly the set of shared archives). The lock covers the rewrite and the
  link, since the link reads the archives too. The memoisation from issue
  0850 keeps the serialised part short.
- `SdkLock::save` writes through `atomic_file::atomic_write` (temp + rename),
  so a reader never sees a half-written lock.
- `project_lock::record` holds an advisory `File::lock` on the lock's
  DIRECTORY for the whole load -> merge -> save, so two recorders cannot lose
  each other's entries. A directory lock adds no file to the tree (an
  untracked file beside the lock is a staleness input, issue 1845).

Verified: the 9 C, 8 C++ and 12 Rust threadx-riscv64 workspace images of
phase-535 W5 build in one parallel run each, with no failure.
