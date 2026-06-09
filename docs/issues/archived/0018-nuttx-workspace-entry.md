---
id: 18
title: NuttX workspace Entry — builds, boots, and delivers E2E via cargo lane
status: resolved
type: bug
area: nuttx
related: [phase-225, issue-0017]
resolved_in: 93dbae16b + c5453a5b3 + 7648cb43a
---

**Resolved (2026-06-09)**: the NuttX workspace Entry now publishes `/chatter`
over zenoh and an external native listener receives it cross-process
(`Received: 17,18,19,…`; ~92 packets on the wire). The full chain —
`just`/lane build → boot on `qemu-system-arm` virt cortex-a7 → register the
zenoh backend → `Executor::open` → publish → cross-process delivery — works
end to end through the fixture lane
(`scripts/build/workspace-fixtures-build.sh nuttx rust`).

The transport blocker was a multi-layer cascade in the never-before-built
cargo-NuttX-zenoh path, all fixed: (1) runtime locator defaulted to loopback
`127.0.0.1:7447` on the env-less guest — now baked via `option_env!`;
(2) the entry linked NO RMW backend — added `nros/rmw-zenoh`;
(3) `nros/platform-nuttx` didn't forward `nros-rmw-zenoh?/platform-nuttx`
(so `zpico-sys` missed `ZENOH_NUTTX` and zenoh-pico's NuttX
SO_LINGER/TCP_NODELAY setsockopt guards stayed off) — fixed;
(4) `nros-smoltcp` gated `portable-atomic/unsafe-assume-single-core` too
broadly (`cfg(any(arm, riscv32))` caught hosted armv7a-nuttx) — narrowed to
bare-metal (`target_os="none"`); (5) the unified-RMW
`nros_rmw_register_backend!` macro is a no-op on NuttX (linkme unsupported)
and the flat image doesn't run `.init_array`, so `nros-board-nuttx::run_entry`
now calls `nros_rmw_zenoh::register()` explicitly (entry → board-qemu-arm →
board-nuttx wiring).

The standalone image is linked the freertos way: a `build.rs` on the entry
crate links the prebuilt NuttX staging libs (`$NUTTX_DIR/staging/*.a`) +
`dramboot.ld` + vector-table head object + `--entry=__start -nostartfiles
-nodefaultlibs`, so the NuttX flat-build kernel image *is* the cargo binary
(`std` resolves from NuttX `libc.a`).

**Caveat (open follow-up)**: the NuttX standalone flat image miscompiles at
the `nros-fast-release` opt-level (boots to `main` but the runtime never
functions; `release` opt-level 3 works), so the lane forces `--release`
(`workspace-fixtures-build.sh`) until that profile-specific break is
root-caused.
