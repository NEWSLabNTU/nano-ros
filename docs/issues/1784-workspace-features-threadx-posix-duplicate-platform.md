---
id: 1784
title: "`check::workspace-features`: nros-board-threadx's lib test links two
  platform ABIs (ThreadX and POSIX) and fails with duplicate `nros_platform_*`"
status: open
type: bug
area: [build, boards, threadx]
severity: medium
found: 2026-10-10
related: [1177, 1750]
---

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
