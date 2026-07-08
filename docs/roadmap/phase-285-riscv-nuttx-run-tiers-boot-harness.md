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

### W1 — Unblock the riscv-nuttx build (arch-aware external staging)
- [ ] W1.a `stage-external-apps.sh` (or the `build-riscv-c` path): stage the
  `examples/qemu-riscv-nuttx/{c,…}` examples for a riscv build instead of reusing
  the arm staging; reset `nuttx-apps/external/` on an arch switch so
  `olddefconfig` never sources a stale-arch app `Kconfig`.
- [ ] W1.b Acceptance: `just nuttx build-riscv-c` succeeds **after** an arm
  nuttx build in the same tree (the 2026-07-09 failure), from a clean tree, and
  back-to-back arm↔riscv without a manual wipe.

### W2 — rv-virt NuttX boot harness
- [ ] W2.a Add `QemuProcess::start_nuttx_riscv(binary, networking)` +
  `is_qemu_riscv32_available()` to `nros-tests/qemu.rs`, mirroring
  `start_nuttx_virt` but `qemu-system-riscv32 -M virt -bios none` + the virtio-net
  MMIO device. (esp32's riscv32 uses the Espressif machine, not rv-virt; threadx
  is riscv64 — neither is reusable.)
- [ ] W2.b Prove the harness on the EXISTING `examples/qemu-riscv-nuttx/c/talker`:
  boot it + a native `/chatter` observer + a host zenohd; assert cross-process
  delivery. This is riscv-nuttx's FIRST runtime (today it is link-only).

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
