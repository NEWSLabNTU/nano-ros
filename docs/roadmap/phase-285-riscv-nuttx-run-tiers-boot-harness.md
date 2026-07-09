# Phase 285 — riscv-nuttx runtime: boot harness + `run_tiers` seam (resolve #165)

Status: **Draft — 2026-07-09** · Resolves issue #165 · Implements RFC-0015
(Model-1 tiers) for the riscv-nuttx board · Sibling of phase-281 (arm-nuttx
tiers) + phase-130/#130 (nuttx entry eth0).

> **Goal.** Give `nros-board-nuttx-qemu-riscv` (`QemuRvVirt`) a *proven*
> RFC-0015 Model-1 `run_tiers` seam — not a compile-only symmetry stub. That
> requires first standing up an rv-virt NuttX **runtime** (currently riscv-nuttx
> is link-checked but never booted), then the eth0 + seam + a multi-tier example
> + e2e, and a decision on whether riscv-nuttx becomes a first-class matrix axis.

## Design findings (phase-285 exploration, 2026-07-09)

- **The seam is thin.** `nros_board_nuttx::run_tiers<B: BoardInit, F, E>(boot_config,
  tiers, setup)` (`nros-board-nuttx/src/lib.rs:361`) is generic over the board;
  the arm seam is just a per-board wrapper that pushes eth0 then delegates:
  ```rust
  // nros-board-nuttx-qemu-arm/src/entry_212n.rs
  #[cfg(target_os = "nuttx")]
  impl QemuArmVirt {
      pub fn run_tiers<F, E>(deploy, tiers, setup) -> Result<(), E> {
          entry_net_init(Some(deploy));                    // #130 eth0 push
          nros_board_nuttx::run_tiers::<Self, F, E>(deploy.boot_config, tiers, setup)
      }
  }
  ```
  The riscv twin is structurally identical over `QemuRvVirt`.
- **eth0 is the #130 story again.** `QemuRvVirt`'s entry path has a NO-OP
  `BoardInit::init_hardware` and NO `entry_net_init` — its comment claims the
  defconfig kernel-boot bring-up suffices. That is the exact assumption #130
  debunked on arm (the defconfig IP is not slirp-reachable → the guest never
  reaches `10.0.2.2`). The riscv **role** path (`node.rs:21` `init_hardware` →
  `apply_ip_config`, `SIOCSIFADDR`) already has the push; the entry path needs the
  same `configure_entry_eth0` twin.
- **The build blocker is arm-hardcoded staging.** `scripts/nuttx/stage-external-apps.sh`
  loops over `examples/qemu-arm-nuttx/{c,cpp}/` only. On a shared tree, a
  `just nuttx build-riscv-c` after an arm build reuses the ARM external staging →
  `olddefconfig` fails sourcing a missing arm-app `Kconfig` (observed 2026-07-09).
  The runtime work is blocked until staging is arch-aware.
- **riscv-nuttx is off-matrix.** `exec_model_matrix.rs:26`
  `PLATFORMS = [native, freertos, zephyr, nuttx]`; the nuttx cells are proven
  arm-only. Making riscv-nuttx a first-class axis means all three langs' riscv
  e2e must exist (the matrix invariant checks LANGS×PLATFORMS) — a W6 decision.
- **Runtime prerequisites are present locally:** `qemu-system-riscv32`,
  `riscv-none-elf-gcc`; the rv-virt boot command is emitted by
  `build-nuttx.sh:232` (`qemu-system-riscv32 -M virt -bios none -nographic
  -kernel <nuttx> -netdev user,id=u1 -device virtio-net-device,netdev=u1`).

## Waves

### W1 — Unblock the riscv-nuttx build (stale external staging) — DONE 2026-07-09
- [x] W1.a Root cause was narrower than "arm-hardcoded staging": the kernel
  `distclean` on reconfigure does NOT touch the apps tree, so a STALE
  `nuttx-apps/external/Kconfig` (a pre-212.M-F.12 per-example staging that
  `source`s per-example Kconfigs no longer present) survives arch-switches and
  makes `make olddefconfig` hard-fail. Fix: `build-nuttx.sh` now regenerates a
  valid `external/` via `stage-external-apps.sh` (current minimal
  integration-shell Kconfig) BEFORE `olddefconfig`, inside the reconfigure block.
- [x] W1.b Acceptance: reproduced the failure (dangling
  `nano-ros-BOGUS-stale/Kconfig` source + forced reconfigure) → build-nuttx.sh
  regenerated the Kconfig clean (BOGUS dropped) and `olddefconfig`/export passed
  (RC=0); `just nuttx build-riscv-c` builds the rv-virt kernel + C talker green
  after an arm build in the same tree.

### W2 — rv-virt NuttX boot harness — HARNESS DONE; W2.b BLOCKED 2026-07-09
- [x] W2.a Added `QemuProcess::start_nuttx_riscv(binary, networking)` to
  `nros-tests/qemu.rs` (`qemu-system-riscv32 -M virt -bios none -nographic -icount
  shift=auto` + the virtio-net MMIO device, matching `build-nuttx.sh`'s rv-virt
  export). Reuses the existing `esp32::is_qemu_riscv32_available`. Compiles; the
  harness boots the image and captures the console.
- [x] W2.b.1 **BLOCKED — riscv-nuttx image PANICS at boot** (a real runtime defect
  the harness just exposed — riscv-nuttx had never been booted, only link-checked).
  Filed as **issue #167** and **root-caused with gdb** (`-gdb tcp::1234 -S` +
  `riscv-none-elf-gdb`, 2026-07-09). `EPC=RA=0x4` is a jump through a garbage
  (non-null `~0x4`, slips past `beqz` guards) function pointer. NOT the kernel
  work-queue / netdev / virtio path (those run fine — `metal_io` ops valid, notify
  works). It is the **nano-ros backend-open path**:
  ```
  nros_app_main → nros_cpp_init(config,"node",storage)
    → nros_app_register_backends → nros_rmw_zenoh_register   [OK, CffiRmw in REGISTRY]
    → REGISTRY scan → <CffiRmw as Rmw>::open → CffiSession::open_with_vtable
      → vtable[0] = open_trampoline<ZenohRmw>  (C-FFI vtable @0x80089314 intact)
        → ZenohRmw::open → zenoh-pico session bring-up → garbage fn-ptr → fault
  ```
  Every layer down to `open_trampoline<ZenohRmw>` enters and never returns; the bad
  pointer is read **inside `ZenohRmw::open` / zenoh-pico open**. Leading cause:
  zpico↔zenoh-pico config ABI mismatch (issue #135 pattern — flag-gated struct
  fields shift a fn-ptr offset) or an unwired transport init on riscv. Full chain
  + hypotheses in issue #167.
- [ ] W2.b.2 **Fix #167** — trace `ZenohRmw::open` → zenoh-pico `_z_open` on rv-virt
  to the exact struct field/config flag whose pointer reads `~0x4`; compare arm-nuttx
  zenoh-config injection + generated-config sharing vs riscv; rebuild the riscv
  fixture after any zpico config change. **This crash gates W2.b → W3–W6.**

### W3 — eth0 on the riscv entry path (#130 shape)
- [ ] W3.a Add `configure_entry_eth0` / `entry_net_init` to the riscv board
  (reusing `node.rs::apply_ip_config`), and decide via W2's pcap whether the
  entry needs the `SIOCSIFADDR` push or the defconfig bring-up genuinely suffices
  on rv-virt. Confirm empirically, not by comment.

### W4 — `run_tiers` seam on `QemuRvVirt`
- [ ] W4.a Add `#[cfg(target_os = "nuttx")] impl QemuRvVirt { pub fn run_tiers(...) }`
  mirroring the arm seam — the eth0 push (W3) then
  `nros_board_nuttx::run_tiers::<Self, _, _>(deploy.boot_config, tiers, setup)`.

### W5 — a multi-tier riscv-nuttx example
- [ ] W5.a There is only a lone C `talker`; `run_tiers` needs a 2-tier plan. Add a
  `ws-realtime` riscv-nuttx twin (rust and/or C) — `demo_bringup/system.toml` with
  `[tiers.high]`/`[tiers.low]` + `[tiers.*.nuttx]` priorities, mirroring the arm
  `ws-realtime-{rust,c}` — so the macro emits `<QemuRvVirt>::run_tiers`.

### W6 — e2e + matrix decision
- [ ] W6.a `realtime_tiers_{rust,c,cpp}_riscv_nuttx_e2e` using the W2 harness +
  the deterministic per-tier max-value proof (#158 shape).
- [ ] W6.b Decide: promote riscv-nuttx to `exec_model_matrix.rs` `PLATFORMS`
  (then all three langs' riscv e2e are mandatory), OR keep it an explicitly
  off-matrix board documented in the matrix file. Close #165.

## Non-goals
- Real hardware (rv-virt QEMU is the baseline).
- A generic riscv64 nuttx board (this is riscv32 rv-virt).
- Changing the arm-nuttx seam (phase-281) — riscv mirrors it, does not refactor.

## Acceptance
- `just nuttx build-riscv-c` robust across arch switches (W1).
- riscv-nuttx boots + delivers cross-process under the new harness (W2).
- `<QemuRvVirt>::run_tiers` drives a real 2-tier riscv example to a passing
  deterministic e2e (W4/W5/W6).
- #165 resolved with an explicit matrix decision (W6.b) — no silent cap.

## Sequencing
W1 first (nothing riscv-runtime builds until staging is arch-aware — the observed
blocker). W2 (harness, proven on the existing C talker) gates W3–W6. W3→W4 (eth0
before the seam that uses it); W5 is independent; W6 needs W4+W5. This is a
larger port than a single sitting; each wave is independently landable + proven,
never a compile-only stub.
