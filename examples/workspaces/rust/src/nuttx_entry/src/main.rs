//! Entry pkg for the shared Rust workspace on NuttX QEMU ARM virt.
//!
//! Phase 225.O follow-up (known-issue #18) — the body is the SAME
//! one-line `nros::main!(model = …)` the native / freertos / threadx /
//! zephyr / esp32 entries use. `[package.metadata.nros.entry] deploy =
//! "nuttx"` maps the board to `nros_board_nuttx_qemu::QemuArmVirt`
//! and (since NuttX rides `Framework::OwnedSpin`) emits a hosted
//! `fn main()` that:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses `demo_bringup/launch/system.launch.xml`,
//!   3. registers `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;` (launch file = single source
//!      of truth for the node set) inside the `BoardEntry::run` closure,
//!   4. delegates to `<QemuArmVirt as BoardEntry>::run`
//!      (`nros-board-nuttx::run_entry`: opens an `Executor`, wraps it in
//!      `ExecutorNodeRuntime`, registers each launch-named node, spins
//!      forever).
//!
//! The NuttX flat-build init task calls `CONFIG_INIT_ENTRYPOINT="nsh_main"`,
//! NOT this `fn main` directly. The board crate
//! (`nros-board-nuttx-qemu`'s `entry.rs`) exports a
//! `#[no_mangle] nsh_main` that runs `nsh_initialize()` (virtio FDT
//! discovery + network bringup) and then calls the image's `main`
//! symbol — so the kernel reaches the entry emitted below.
//!
//! phase-359 W7 — NuttX used to be built as a hosted `std` target, and this
//! comment used to say so: it was "a plain `std` bin", unlike the
//! `#![no_std] #![no_main]` freertos/esp32 entries. It is now one of them —
//! `nros::main!()` emits the `extern "C" fn main` that `nsh_main` calls. The
//! prebuilt NuttX kernel libs are still linked by `build.rs`.

#![no_std]
#![no_main]

nros::main!(launch = "demo_bringup");
