---
id: 1280
title: "A build in a worktree uses ANOTHER checkout's SDK trees — 19 `sdk-env.just` paths are inherited, absolute, and win over the worktree"
status: open
type: bug
area: [build, tooling]
related: [1253, 0491, 0986, phase-445]
---

## What happened

On 2026-09-11 a phase-445 agent ran `just nuttx build-riscv-c` in its own git
worktree (`.claude/worktrees/agent-…`). It reconfigured and rebuilt the NuttX
kernel of the **main checkout** instead: `/home/aeon/repos/nano-ros/third-party/nuttx/nuttx`
went from `CONFIG_ARCH_BOARD="qemu-armv7a"` to `"rv-virt"`, with its `.config`,
`include/nuttx/config.h` and `nuttx` binary rewritten at 01:11:18. Nothing in
the worktree's own `third-party/nuttx` was touched, and nothing said so. The
main checkout was restored with `just nuttx build` (`build-nuttx.sh` sees the
board mismatch and reconfigures).

## Why

`just/sdk-env.just` defaults every SDK source path with the inherited value
first:

```just
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
```

and the process that spawns worktree builds already carries those variables as
ABSOLUTE paths into the checkout it was started from. Measured in the session
that hit this — every one of the 19 `env(…, justfile_directory()…)` defaults in
`sdk-env.just` was set, all pointing at the main checkout:

`FREERTOS_DIR`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, `NROS_PLATFORM_CFFI_INCLUDE`,
`NROS_PLATFORM_FREERTOS_SRC`, `NROS_PLATFORM_POSIX_SRC`,
`NROS_PLATFORM_THREADX_SRC`, `NROS_VIRTIO_NET_NETX_DIR`, `NROS_C_INCLUDE`,
`NROS_CPP_INCLUDE`, `TBAND_DIR`, `NUTTX_DIR`, `NUTTX_APPS_DIR`, `THREADX_DIR`,
`THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR`, `NROS_ESP_IDF_WORKSPACE`,
`NROS_ESP_IDF_ENV_SHIM`.

So a worktree build compiles the MAIN checkout's FreeRTOS, ThreadX, NetX,
NuttX, platform sources and public headers, beside its own crates. Two harms,
and the second is the worse one:

1. **It writes into another checkout.** The NuttX kernel build is not
   read-only; it reconfigures the tree it is pointed at, so a worktree build
   silently changes the main checkout's state (and races any build there).
2. **It certifies the wrong code.** A worktree exists to test a change. An
   edit to `packages/platform/nros-platform-freertos/src` or
   `packages/api/nros-c/include` in the worktree is NOT what gets compiled —
   the build reads the main checkout's copy, links, passes, and reports the
   change as verified. That is issue 1253 exactly (a Zephyr workspace whose
   manifest bound another checkout's module, so the runner "certified
   `/mnt/evo`'s module"), one mechanism over.

CLAUDE.md prescribes linked worktrees for parallel sessions, so this is the
default shape for agent work, not an edge case.

## Fix direction (not decided)

Same answer 1253 chose for the Zephyr workspace: a path that belongs to a
DIFFERENT nano-ros checkout is refused, or re-rooted, never silently used.
Candidates:

- `sdk-env.just` resolves each default against `justfile_directory()` whenever
  the inherited value is inside a different nano-ros checkout (detected the way
  `check-zephyr-workspace-checkout.sh` detects one), and prints the rewrite;
- or a preflight (`check-tier-preconditions` / the lane preflight) refuses with
  a message naming both checkouts, as issue 1253's check does.

A value that points OUTSIDE any nano-ros checkout (a real out-of-tree SDK) must
keep working — the inherited-first order exists for that.

Acceptance: in a linked worktree with all 19 variables exported to the main
checkout, `just nuttx build` either builds the worktree's tree or refuses
naming both paths; and a FreeRTOS example built there compiles the worktree's
`nros-platform-freertos/src` (check with a deliberate `#error` in the worktree
copy).
