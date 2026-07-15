# Phase 285 — riscv-nuttx runtime: boot harness + `run_tiers` seam (resolve #165)

Status: **COMPLETE — 2026-07-15** (W1–W6 all landed; #165 resolved, riscv C lane red tracked as #199) · Resolves issue #165 · Implements RFC-0015
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
  The chain is symbolized down to `ZenohRmw::open` / zenoh-pico session bring-up.
- [x] W2.b.2 **Root cause CONFIRMED (2026-07-09, qemu re-enabled): timing-dependent
  virtio-net IRQ re-entrancy RACE — NOT config.** A `-d exec` trace of the same image
  runs clean to idle (no `riscv_exception`); every gdb run crashes — QEMU slirp
  packet timing is host-timed, not `-icount`-controlled. zenoh's TCP connect runs the
  full TX poll synchronously with the virtio-net IRQ live (`netdev_upper_txavail →
  devif_poll → virtio_net_send → virtqueue_kick`, mutating the TX vring); a virtio
  IRQ mid-poll re-enters the vring → corrupt descriptor/ra → `jr` to `~0x4`. **arm
  boots the same image bare without panicking.** Ruled out by build+boot (4 config
  fixes failed): stack sizes, IOB config, full arm net-config mirror. The earlier
  "garbage vtable/ABI-mismatch" leads were transient heap reuse. Details in #167.
- [x] W2.b.3 **Fix #167 — RESOLVED (2026-07-13, `d06d25fa4`).** The W2.b.2 "virtio-net
  IRQ re-entrancy race" verdict was a red herring: the DEFINITIVE root cause is a
  `struct pollfd` ABI mismatch (Rust std's 8-byte layout vs NuttX's 24-byte one —
  `poll()` writes 6 fields per entry into the caller's array → 48-byte OOB smashes the
  entry task's saved ra). Fix = a `--wrap=poll` shim bridging the layouts (libc fork
  branch `nuttx-0.2` @ `adb4c592e`, wired via `nros-nuttx-ffi/.cargo/config.toml`
  link-args). Boot-verified on rv-virt. **W2 is fully done; W3–W6 are UNBLOCKED.**
  Note: `start_nuttx_riscv` still has no test consumer — W6 adds the first.

### W3 — eth0 on the riscv entry path (#130 shape)
- [x] W3.a **DONE (2026-07-15).** `configure_entry_eth0` / `entry_net_init` added to
  the riscv board's `entry_212n.rs`, delegating to the sole
  `node.rs::init_hardware` (`SIOCSIFADDR` push + `/dev/urandom` re-seed). Slirp
  defaults `10.0.2.15/24` via `10.0.2.2` — matching the rv-virt defconfig
  `NETINIT` (note: arm uses `.30`). **Empirical verdict: the defconfig `NETINIT`
  bring-up suffices for the default case** (pcap shows eth0 UP+RUNNING with the
  baked IP before the entry runs); the push exists so `DeployOverlay`
  ip/netmask/gateway overrides take effect on the entry path (sibling guests).

### W4 — `run_tiers` seam on `QemuRvVirt`
- [x] W4.a **DONE (2026-07-15).** `#[cfg(target_os = "nuttx")] impl QemuRvVirt {
  pub fn run_tiers(...) }` mirroring the arm seam — `entry_net_init(Some(deploy))`
  then `nros_board_nuttx::run_tiers::<Self, _, _>(deploy.boot_config, tiers, setup)`.
  `nros-orchestration-ir::board_path_for` + `nros-macros` gained the
  `nuttx-riscv` board key.

### W5 — a multi-tier riscv-nuttx example
- [x] W5.a **DONE (2026-07-15) — Rust twin.** `ws-realtime-rust` gained
  `src/riscv_nuttx_entry` (`deploy = "nuttx-riscv"`, locator
  `tcp/10.0.2.2:17867`) sharing the existing `demo_bringup` 2-tier plan (the
  `[tiers.*.nuttx]` table keys on the RTOS, so arm and riscv share it). Cross
  lane: local `riscv32imac-unknown-nuttx-elf.json` spec + `-Z build-std` +
  `--wrap=poll`, fixture `workspace-rust-nuttx-riscv-realtime` built by
  `just nuttx build-riscv-rust` (self-provisions the rv-virt kernel; deliberately
  NOT dependent on `build-riscv-c`, whose C half is red pre-existing — see the
  as-landed notes). No `-lxx` on rv-virt (staging has no libxx); `-lboard` from
  `arch/risc-v/src/board`.

### W6 — e2e + matrix decision
- [x] W6.a **DONE (2026-07-15) — Rust lane GREEN.**
  `realtime_tiers_riscv_nuttx_e2e` (W2 harness `start_nuttx_riscv` + the #158
  per-tier monotonic-counter proof, `ctrl_max ≥ 3× telem_max`) passes in ~12 s.
  C/C++ riscv e2e siblings deferred with the C lane (pre-existing
  `build-riscv-c` ffi-link red). Two riscv-only runtime fixes were needed:
  - **`CONFIG_SYSTEM_TIME64=y`** added to the rv-virt defconfig (arm already had
    it): the patched Rust libc fork hardcodes `time_t = i64`, so the 32-bit
    kernel default made every `clock_gettime` read garbage → std panicked
    `invalid timestamp` inside zenoh session bring-up (the abort landed before
    the connect-retry loop, presenting as `Transport(ConnectionFailed)`).
    Same ABI-mismatch class as #167's `--wrap=poll`.
  - **`CONFIG_NETUTILS_TELNETD` dropped** from the rv-virt defconfig: the
    empty-builtins stub (#18) makes `nsh_telnetstart`'s builtin lookup return
    NULL → `strlcat(NULL)` → Load access fault at boot (arm's defconfig never
    had telnetd).
- [x] W6.b **Decision: riscv-nuttx stays an OFF-MATRIX board** documented in
  `exec_model_matrix.rs` (the nuttx matrix cells are arm by design; riscv-nuttx
  is proven by its own e2e, not a matrix cell). Promotion would make all three
  langs' riscv e2e mandatory while the C lane is red pre-existing. #165 closed.

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
