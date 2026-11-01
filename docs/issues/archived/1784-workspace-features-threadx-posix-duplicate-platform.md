---
id: 1784
title: "`check::workspace-features`: nros-board-threadx's lib test links two
  platform ABIs (ThreadX and POSIX) and fails with duplicate `nros_platform_*`"
status: resolved
type: bug
area: [build, boards, threadx]
severity: medium
found: 2026-10-10
related: [1177, 1750, phase-505]
resolved_in: "PR #1864 (issue 1779): one nros_platform_* provider per linked graph"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What this is

`just check workspace-features` (in `check::build`, so in `just ci gate` step 4
and never on a merge-gating event — issue 1177) fails at its
`cargo test --no-run --workspace --exclude nros-c --no-default-features` step:

```
error: linking with `cc` failed: exit status: 1
  = note: rust-lld: error: duplicate symbol: nros_platform_clock_ns
          >>> defined at packages/platform/nros-platform-threadx/src/platform.c:57
          >>>   … in archive target/debug/build/nros-board-threadx-*/out/libnros_platform_threadx.a
          >>> defined at ../nros-platform-posix/src/platform.c:43
          >>>   … in archive target/debug/deps/libnros_platform_cffi-*.rlib
error: could not compile `nros-board-threadx` (lib test)
```

…and ~20 more `nros_platform_*` symbols. Under `--workspace`, cargo unifies
features across every member, so some member's `platform-posix` (or
equivalent) reaches `nros-platform-cffi`, which then compiles the POSIX C
port into its rlib; `nros-board-threadx`'s build script compiles the ThreadX C
port into its own archive, and the board's lib-test binary links both.

## Measured

- Reproduces on `origin/main` at `d23dff40e` and at `eba7549e2~1`, in a linked
  worktree with every submodule initialised (`just setup-worktree`).
- The same command PASSED in the same worktree roughly an hour earlier, on a
  tree whose `cargo` graph is identical (phase-484 W1's store-root branch), so
  the failure depends on build state as well as on the manifests — not yet
  explained. Do not "fix" it with `rm -rf target`; that destroys the only
  reproduction (CLAUDE.md).

## Next

Find which member enables the POSIX port in the unified graph
(`cargo tree -e features -i nros-platform-cffi --workspace
--no-default-features`), and decide whether the board's lib test may link a
second platform at all — it is the same "two answers to one ABI" shape as
issue 0616.

## Resolution (2026-10-18)

This is the same defect as issue 1772 and issue 1779, filed the same day.
PR #1864 fixed all three with one `nros_platform_*` provider per linked graph:
`nros-platform-cffi` states the provider it compiled, and a board build script
asks `nros_board_common::platform_port::defer_to_graph_provider` before
compiling its own port.

**Measured on `origin/main`, 2026-10-18,** in a worktree with
`third-party/threadx/kernel` initialised, which is the condition this failure
needs. Running `cargo test --no-run --workspace --exclude nros-c
--no-default-features` gives rc=0 and **0** `duplicate symbol` lines, and the
ThreadX board reports the deferral:

```
warning: nros-board-threadx@0.4.0: nros-board-threadx: this build already links the `posix` nros_platform_* provider (nros-platform-cffi, reached through cargo feature unification, e.g. `cargo test --workspace`); skipping the ThreadX C port, which would define every nros_platform_* symbol a second time (issue 1779).
```

**Sweep:** `git grep -n 'defer_to_graph_provider(' -- 'packages/**/build.rs'`.
Both workspace-member boards that compile a C port ask it: `nros-board-freertos`
and `nros-board-threadx`. The other two build scripts that compile a platform
port, `nros-board-threadx-linux` and `nros-board-nuttx-qemu`, are standalone
roots, so `--workspace` never unifies a second provider into them.
