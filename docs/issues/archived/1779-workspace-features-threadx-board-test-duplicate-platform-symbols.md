---
id: 1779
title: "`check-workspace-features` fails to link `nros-board-threadx`'s test binary when the ThreadX submodule is initialised: the POSIX and ThreadX platform archives both define `nros_platform_*`"
status: resolved
type: bug
area: [build, threadx, ci]
severity: low
found: 2026-10-10
related: [1706, issue-1772]
resolved_in: "one nros_platform_* provider per linked graph: boards ask nros-platform-cffi first (issue 1779)"
---

## What was measured

`just ci gate` on a linked worktree where `third-party/threadx/kernel` had been
initialised (to boot an rv-virt-threadx image for issue 1706) failed at
`check::build` → `workspace-features`, on its first command:

```
cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet
error: linking with `cc` failed
  -o .../target/debug/deps/nros_board_threadx-<hash>
  rust-lld: error: duplicate symbol: nros_platform_clock_ns
  >>> defined at nros-platform-threadx/src/platform.c:57  (libnros_platform_threadx.a, nros-board-threadx OUT_DIR)
  >>> defined at nros-platform-posix/src/platform.c:43    (libnros_platform_cffi-*.rlib)
```

…and the same for every `nros_platform_*` symbol (clock, epoch, alloc/realloc/
dealloc, heap counters, sleep, yield, random). The test binary is
`nros-board-threadx`'s own, built for the host under `--workspace` feature
unification, which also gives `nros-platform-cffi` its `posix-c-port`.

## Why it is usually invisible

`nros-board-threadx/build.rs` compiles `libnros_platform_threadx.a` only when the
ThreadX port sources exist (`THREADX_DIR` set AND the submodule initialised; the
guard around line 118 returns otherwise with a warning). Agent worktrees and the
CI lanes that run this gate do not initialise the submodule, so the archive is
never built and only the POSIX port links. Any host with the submodule present
(a developer checkout that has built a ThreadX fixture) sees this red.

The change that hit it (issue 1706: two fixture bins, one board-crate log-writer
call, test/matrix rows) touches neither crate's features nor the workspace
membership.

## Direction

A host-built test target of a board crate should link exactly one platform port.
Either gate the ThreadX archive's `rustc-link-lib` to non-host targets (the host
test binary needs none of it), or exclude `nros-board-threadx`'s test target from
the workspace-unified `--no-default-features` combo with a stated reason.
Acceptance: `workspace-features` green with the submodule initialised, and still
green without it.

## Resolution

Issues 1772 and 1779 are the same defect, filed twice on 2026-10-10. Both are
closed by this fix, and the full record is in 1779.

**Pre-existing, measured on main.** It was reproduced on unmodified `main`
(`f6fcdf49d3`) in a worktree where `third-party/threadx/kernel` was
initialised. `cargo test --no-run --workspace --exclude nros-c
--no-default-features` failed with 20 `duplicate symbol: nros_platform_*`
errors. `cargo build --tests` with the same arguments and `--keep-going`
showed the class: of every test binary in the workspace, `nros-board-threadx`'s
lib test was the only one that failed.

**The dependency edge.** `--workspace` builds every member with ONE feature set
per crate. `nros-board-linux` (and the dev-deps of `nros-node` and `nros-log`)
turn on `nros-platform-cffi/posix-c-port`, so `nros-board-threadx`'s host test
binary links the POSIX port. That board's `build.rs` also compiles the ThreadX
port `+whole-archive`, because its only consumers sit inside the zpico-sys
rlib. Every `nros_platform_*` symbol is therefore defined twice, provided the
POSIX archive's `platform.o` is pulled at all. Issue 1732 added
`nros_posix_install_termination_guard` / `nros_posix_termination_requested` to
that very member (`platform.c:1094`), referenced from
`nros_platform::termination`, which is what made the member pulled. Where the
ThreadX submodule is absent, the board skips its C build and only the POSIX
port links, which is why CI worktrees stayed green.

**Fix: one provider per linked graph, as a protocol rather than an exclusion.**

- `nros-platform-cffi` STATES the provider it compiled. It gains
  `links = "nros_platform_cffi"` and emits `cargo:abi_provider=<posix|stubs|none>`,
  which a direct dependent's build script reads as
  `DEP_NROS_PLATFORM_CFFI_ABI_PROVIDER`.
- Every build-time compile of another port ASKS first, through the new
  `nros_board_common::platform_port::defer_to_graph_provider`. When the graph
  already has a provider, the board skips its C port, as it already does when
  the sources are absent, with a `cargo:warning` naming the provider and this
  issue. There are three sites: `nros-board-threadx`, `nros-board-freertos` and
  `nuttx_platform_build::run_platform`.
- The result no longer depends on whether a submodule happens to be
  initialised. A real ThreadX, FreeRTOS or NuttX image never enables
  `posix-c-port`, so its port still compiles. Its build scripts see `none`.

**One sibling is exempt, with its reason at the site.** `nros-rmw-xrce-cffi`
compiles the SAME `nros-platform-posix` sources into a demand-driven (not
whole-archive) archive. With identical symbol sets the linker never pulls a
second member, so it cannot collide. That matches the `--keep-going`
measurement: only the whole-archive ThreadX port failed.

**Gate:** `check-platform-port-single-provider` (fast lane). Every
`.compile("nros_platform_…")` outside `nros-platform-cffi` must call the helper
before it compiles, unless it carries a reasoned
`// platform-port-provider-exempt:`. The provider must keep its `links` key and
its `abi_provider` line. It has a self-test of six cases.

**Measured:**

- **After the fix:** the issue's command exits 0 with the ThreadX submodule
  initialised, and `nros-board-threadx`'s build-script output records the
  deferral warning.
- **Mutation, the rule:** with `decide()` forced to `Compile`, the command
  fails again with 20 duplicate symbols.
- **Mutation, the gate:** with the ThreadX ask removed, the gate reports
  `packages/boards/nros-board-threadx/build.rs:286`.
- **Unit tests:** `platform_port::tests` covers both outcomes of the rule.
