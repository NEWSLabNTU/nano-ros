---
id: 165
title: "riscv-nuttx board has no `run_tiers` (RFC-0015 Model-1) seam — unwired and unprovable (no qemu-riscv-nuttx runtime)"
status: resolved
type: enhancement
area: nuttx
related: [rfc-0015, phase-281, phase-285]
---

## Problem

`nros-board-nuttx-qemu-riscv` (`QemuRvVirt`) implements the single-tier Entry
path (`nros_platform::BoardEntry::{run,run_with_deploy}` →
`nros_board_nuttx::run_entry`) but has **no `run_tiers`** — the multi-tier
RFC-0015 Model-1 inherent entry that the `nros::main!` generic OwnedSpin arm
targets for a multi-tier plan. Its arm sibling
`nros-board-nuttx-qemu-arm` (`QemuArmVirt`) carries the seam:

```rust
// packages/boards/nros-board-nuttx-qemu-arm/src/entry_212n.rs
#[cfg(target_os = "nuttx")]
impl QemuArmVirt {
    pub fn run_tiers<F, E>(deploy: &DeployOverlay, tiers: &[TierSpec<'_>], setup: F) -> Result<(), E>
    where F: Fn(&mut RuntimeCtx<'_>) -> Result<(), E> + Sync, E: Debug {
        entry_net_init(Some(deploy));                    // issue #130 eth0 push
        nros_board_nuttx::run_tiers::<Self, F, E>(deploy.boot_config, tiers, setup)
    }
}
```

The riscv board has no such `impl QemuRvVirt { run_tiers }`, and — unlike arm —
no `entry_net_init` / `configure_entry_eth0` on the Entry path at all: its
`entry_212n.rs` comments (lines ~48–53) state the riscv Entry path relies on
NuttX bringing up `eth0` (virtio-net) during kernel boot from the defconfig,
rather than the arm path's `SIOCSIFADDR` push. (The legacy *role* path in
`node.rs` does push eth0 via `SIOCSIFADDR`, but the Entry path does not.)

## Why it is not already a defect / silent cap

The RFC-0015 Model-1 convergence matrix (`exec_model_matrix.rs`) declares
`PLATFORMS = [native, freertos, zephyr, nuttx]`, and the **nuttx** cells are
proven **arm-only** by design — `<QemuArmVirt>::run_tiers` +
`realtime_tiers_{rust,c,cpp}_nuttx_e2e`. riscv-nuttx is a **separate board**,
not a matrix axis, so its missing seam does not violate the matrix's
no-silent-caps contract. This issue exists so the gap is **tracked**, not
discovered later as an unexplained asymmetry.

## What DOES exist (correction, 2026-07-08)

riscv-nuttx is not a bare stub — the **build/compile-check** infrastructure is
in place:

- Board crate `nros-board-nuttx-qemu-riscv` (+ `nros-nuttx-ffi`), rv-virt
  `nuttx-config/defconfig`, `cmake/board/nano-ros-board-nuttx-qemu-riscv.cmake`,
  and the `riscv32imac-nuttx-elf` toolchain file.
- One example — `examples/qemu-riscv-nuttx/c/talker` (2 `platform = "nuttx-riscv"`
  rows in `examples/fixtures.toml`, Phase 194.3c).
- A dedicated build recipe — `just nuttx build-riscv-c` →
  `fixtures-build.sh nuttx-riscv c zenoh` — folded into `build-all`, which
  nightly CI runs (`nightly.yml`, `just nuttx build-all`). So the riscv board +
  its one C talker are **link-verified** in CI.

## The real gap (why it cannot be proven now)

The missing piece is the **runtime**, not the build:

- **No rv-virt NuttX boot harness.** `nros-tests` `qemu.rs` `start_nuttx_virt`
  is `qemu-system-arm -M virt -cpu cortex-a7` only. No `start_nuttx_riscv` /
  `qemu-system-riscv32 -M virt` NuttX runner exists — the riscv-nuttx fixtures
  are link-checked and **never booted**. (esp32's `qemu-system-riscv32` uses the
  Espressif machine model, not rv-virt; `threadx-riscv64` is a different RTOS.)
- **C-only, single role.** Only a C `talker` — no rust riscv-nuttx example, and
  no multi-tier example to drive a `run_tiers` at all.
- **No `run_tiers` on `QemuRvVirt`.**

So porting the arm `run_tiers` block would be a **compile-only, e2e-unprovable**
symmetry seam — an untested runtime claim, against the project's "prove it or
defer it" culture. Deferred until an rv-virt NuttX **boot harness** lands.

> **Update (2026-07-15): the gate is CLEARED.** Phase-285 W1+W2 landed the
> harness (`QemuProcess::start_nuttx_riscv`), the boot exposed and #167
> root-caused + fixed the rv-virt boot panic (`struct pollfd` ABI mismatch,
> `--wrap=poll` shim, `d06d25fa4`, boot-verified). Remaining work = phase-285
> W3–W6 (eth0 entry-path decision, the `run_tiers` seam, a multi-tier example,
> e2e + the matrix decision). The harness still has no test consumer — W6's
> e2e is the first.

## Fix direction

The gating prerequisite is a **runtime**, so the seam can be proven, not just
compiled:

1. Add an rv-virt NuttX boot harness — a `start_nuttx_riscv`
   (`qemu-system-riscv32 -M virt -bios none`, virtio-net + slirp) alongside the
   arm `start_nuttx_virt` in `nros-tests` `qemu.rs`. `build-nuttx.sh` already
   emits the exact command (its rv-virt export branch), so this is mostly
   lifting that into the test harness.
2. Add a multi-tier riscv-nuttx example (a rust and/or C `ws-realtime` twin) so
   there is something to drive `run_tiers` — the lone C `talker` cannot.
3. Add `#[cfg(target_os = "nuttx")] impl QemuRvVirt { pub fn run_tiers(...) }`
   mirroring `nros-board-nuttx-qemu-arm/src/entry_212n.rs`, delegating to the
   shared generic `nros_board_nuttx::run_tiers::<Self, _, _>`.
4. Decide the eth0 story for the Entry path — either confirm the defconfig
   kernel-boot bring-up is sufficient for a networked multi-tier entry, or add
   an `entry_net_init` twin (issue #130 shape) if the guest needs the
   `SIOCSIFADDR` push.
5. Add `realtime_tiers_{rust,c,cpp}_riscv_nuttx_e2e` and, if riscv-nuttx is to
   be a first-class matrix axis, extend `exec_model_matrix.rs` `PLATFORMS`
   accordingly (else keep it an explicitly-documented off-matrix board).


## Resolution (2026-07-15, phase-285 W3–W6)

All five fix-direction items landed; the seam is **runtime-proven**, not just
compiled:

1. Harness: `QemuProcess::start_nuttx_riscv` (phase-285 W2, earlier).
2. Example: `ws-realtime-rust/src/riscv_nuttx_entry` (`deploy = "nuttx-riscv"`,
   fixture `workspace-rust-nuttx-riscv-realtime`, `just nuttx build-riscv-rust`)
   sharing the arm example's `demo_bringup` 2-tier plan (the `[tiers.*.nuttx]`
   table keys on the RTOS).
3. Seam: `#[cfg(target_os = "nuttx")] impl QemuRvVirt { pub fn run_tiers(...) }`
   in `entry_212n.rs`, + the `nuttx-riscv` board key in
   `nros-orchestration-ir::board_path_for` / `nros-macros`.
4. eth0: `entry_net_init` / `configure_entry_eth0` twin added (delegating to
   `node.rs::init_hardware`). Empirical verdict: defconfig `NETINIT` suffices
   for the default; the push is what makes `DeployOverlay` overrides real.
5. e2e: `realtime_tiers_riscv_nuttx_e2e` GREEN (~12 s; #158 monotonic-counter
   proof). Matrix decision: riscv-nuttx stays an **off-matrix board**, documented
   in `exec_model_matrix.rs` (nuttx matrix cells are arm by design; the riscv
   C lane is red pre-existing → #199). C/C++ riscv e2e deferred with that lane.

Two riscv-only runtime fixes were required en route (both defconfig):
`CONFIG_SYSTEM_TIME64=y` (Rust libc fork hardcodes `time_t = i64`; the 32-bit
kernel default garbled every `clock_gettime` → std `invalid timestamp` panic
inside session bring-up — #167's ABI-mismatch class) and dropping
`CONFIG_NETUTILS_TELNETD` (empty-builtins stub #18 makes `nsh_telnetstart`
`strlcat(NULL)` → boot Load access fault).
