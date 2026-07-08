---
id: 163
title: "riscv-nuttx board has no `run_tiers` (RFC-0015 Model-1) seam — unwired and unprovable (no qemu-riscv-nuttx runtime)"
status: open
type: enhancement
area: nuttx
related: [rfc-0015, phase-281]
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

## Why it cannot be closed now (blocked)

There is no riscv-nuttx runtime to prove a `run_tiers` against:

- No `examples/**` riscv-nuttx project.
- No `qemu-system-riscv*` NuttX boot helper in `nros-tests` fixtures
  (`start_nuttx_virt` is arm-only; the only riscv qemu path is
  `threadx-riscv64`, a different RTOS).

Porting the arm block to riscv would be a **compile-only, e2e-unprovable**
symmetry seam — an untested claim, against the project's "prove it or defer it"
culture. Deferred until a riscv-nuttx runtime exists.

## Fix direction

When a riscv-nuttx qemu runtime lands (a `qemu-system-riscv` NuttX boot helper
+ at least one networked entry fixture):

1. Add `#[cfg(target_os = "nuttx")] impl QemuRvVirt { pub fn run_tiers(...) }`
   mirroring `nros-board-nuttx-qemu-arm/src/entry_212n.rs`, delegating to the
   shared generic `nros_board_nuttx::run_tiers::<Self, _, _>`.
2. Decide the eth0 story for the Entry path — either confirm the defconfig
   kernel-boot bring-up is sufficient for a networked multi-tier entry, or add
   an `entry_net_init` twin (issue #130 shape) if the guest needs the
   `SIOCSIFADDR` push.
3. Add `realtime_tiers_{rust,c,cpp}_riscv_nuttx_e2e` and, if riscv-nuttx is to
   be a first-class matrix axis, extend `exec_model_matrix.rs` `PLATFORMS`
   accordingly (else keep it an explicitly-documented off-matrix board).
