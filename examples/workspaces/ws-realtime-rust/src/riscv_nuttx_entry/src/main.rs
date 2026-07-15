//! Entry pkg for the RT-tiers Rust workspace on NuttX QEMU rv-virt (riscv32).
//!
//! phase-285 W5 (RFC-0015 Model 1, issue #165) — the riscv-nuttx projection of
//! `ws-realtime-rust`. Same one-line `nros::main!(launch = ...)` as the native /
//! zephyr / arm-nuttx siblings; `deploy = "nuttx-riscv"` (Cargo.toml) selects
//! the rv-virt board (`QemuRvVirt`, `Framework::OwnedSpin`), and the
//! `[tiers.*]` table in `system.toml` (with `[tiers.*.nuttx]` raw SCHED_FIFO
//! priorities — the tier table keys on the RTOS, shared with the arm board)
//! flips the macro's generic OwnedSpin arm onto `<QemuRvVirt>::run_tiers`:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` (ctrl + telem) + `system.toml` tiers,
//!   3. pushes the guest IP into `eth0` (phase-285 W3 `entry_net_init`; the
//!      rv-virt defconfig NETINIT already brings it up with the same slirp
//!      defaults, so the push matters only for `DeployOverlay` overrides),
//!      opens the ONE zenoh session, and spawns one `std::thread` per tier —
//!      the boot thread runs `high` (`ctrl`, 10 ms), a pool thread runs `low`
//!      (`telem`, 100 ms); each tier registers through the same closure with
//!      its `active_groups` filter installed,
//!   4. the nodes publish `/ctrl` + `/telem` for cross-process observers.
//!
//! The NuttX flat-build init task calls `CONFIG_INIT_ENTRYPOINT="nsh_main"`;
//! the board crate exports a `#[no_mangle] nsh_main` that runs
//! `nsh_initialize()` (virtio FDT discovery + network bringup) and then
//! reaches this `fn main()`.

nros::main!(launch = "demo_bringup:system.launch.xml");
