//! Entry pkg for the RT-tiers Rust workspace on NuttX QEMU ARM virt.
//!
//! phase-281 W3-nuttx (RFC-0015 Model 1) — the tiers-on-NuttX projection of
//! `ws-realtime-rust`. Same one-line `nros::main!(launch = ...)` as the native /
//! zephyr siblings; `deploy = "nuttx"` (Cargo.toml) selects the NuttX board
//! (`QemuArmVirt`, `Framework::OwnedSpin`), and the `[tiers.*]` table in
//! `system.toml` (with `[tiers.*.nuttx]` raw SCHED_FIFO priorities) flips the
//! macro's generic OwnedSpin arm onto `<QemuArmVirt>::run_tiers`:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` (ctrl + telem) + `system.toml` tiers,
//!   3. pushes the guest IP into `eth0` (issue #130 / `entry_net_init`), opens
//!      the ONE zenoh session, and spawns one `std::thread` per tier over it —
//!      the boot thread runs `high` (`ctrl`, 10 ms), a pool thread runs `low`
//!      (`telem`, 100 ms); each tier registers through the same closure with its
//!      `active_groups` filter installed,
//!   4. the nodes publish `/ctrl` + `/telem` for cross-process observers.
//!
//! The NuttX flat-build init task calls `CONFIG_INIT_ENTRYPOINT="nsh_main"`; the
//! board crate exports a `#[no_mangle] nsh_main` that runs `nsh_initialize()`
//! (virtio FDT discovery + network bringup) and then reaches this `fn main()`.

nros::main!(launch = "demo_bringup:system.launch.xml");
