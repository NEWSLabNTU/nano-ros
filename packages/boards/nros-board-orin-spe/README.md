# `nros-board-orin-spe`

Board crate for running nano-ros on the **NVIDIA AGX Orin Sensor
Processing Engine (SPE)** — the always-on Cortex-R5F core that boots
before the CCPLEX, runs NVIDIA's FreeRTOS V10.4.3 FSP, and survives
Linux crashes. The natural home for the `autoware_sentinel` safety
island.

Phase 100.6 of [`docs/roadmap/phase-100-orin-spe-infra.md`](../../../docs/roadmap/phase-100-orin-spe-infra.md).

## Prerequisites

- **NVIDIA SDK Manager-installed FSP tree.** Set `NV_SPE_FSP_DIR` to
  the directory containing `lib/libtegra_aon_fsp.a` (and the matching
  `include/` headers). The FSP ships under NVIDIA's SDK Manager EULA;
  this crate cannot vendor it.
- **`armv7r-none-eabihf` rustup target.** Pinned in nano-ros's root
  `rust-toolchain.toml` (Phase 100.2) — `just workspace rust-targets`
  installs it.
- **Nightly toolchain** for `-Zbuild-std=core,alloc`. The pin is in
  `tools/rust-toolchain.toml` (the workspace nightly channel).
- **`arm-none-eabi-{gcc,ld,size}` on `PATH`** — needed by
  `cc::Build` to cross-compile zenoh-pico's C source. The system
  `gcc-arm-none-eabi` package ships these.

## Build

This crate is **excluded from the workspace** because the rest of the
workspace can't see the NVIDIA FSP. Build directly:

```sh
cd packages/boards/nros-board-orin-spe
NV_SPE_FSP_DIR=$HOME/nvidia/spe-fsp \
  cargo +nightly build --release
```

Or via the per-platform recipe (Phase 100.7):

```sh
just orin_spe build
```

The output is an `rlib` consumed by the firmware crate that wraps it
into a `staticlib` and links against NVIDIA's Makefile via
`ENABLE_NROS_APP := 1` to land inside `spe.bin`.

## Cargo features

| Feature       | Default | Effect |
|---------------|---------|--------|
| `fsp`         |   yes   | Link `tegra_aon_fsp.a`. Needs `NV_SPE_FSP_DIR`. |
| `cortex-r`    |   yes   | Register `critical_section::Impl` against the ARMv7-R CPSR I-bit (Phase 100.1). |

The `unix-mock` host bring-up path (FreeRTOS POSIX simulator + Linux
IVC bridge daemon) lives in `autoware_sentinel`'s `ivc-bridge` and in
`zpico-sys`'s integration tests (Phase 100.8) — *not* here.

## Defaults you'll override

`Config::default()` is a sensible bring-up baseline:

```rust
Config {
    zenoh_locator: "ivc/2",     // channel 2 = aon_echo
    domain_id: 0,
    app_priority: 12,
    app_stack_bytes: 16384,
    zenoh_read_priority: 16,
    zenoh_read_stack_bytes: 4096,
    zenoh_lease_priority: 16,
    zenoh_lease_stack_bytes: 4096,
}
```

The 16 KB app stack is half of the MPS2 board crate's 64 KB —
deliberately tighter to fit the SPE's 256 KB BTCM. If your closure
needs more, raise `app_stack_bytes` and shrink something else.

`Config::with_zenoh_locator` panics at boot if the locator is not
`ivc/...`. Phase 100 design §9 calls out the visual collision between
`serial/N` and `ivc/N` (small integer after a 5–6-character prefix);
the assertion turns the silent-disconnect failure mode into a loud
boot panic.

## Flash

Flash the SPE firmware partition (`A_spe-fw`) from an x86 host with
the Orin in USB recovery mode:

```sh
# from the L4T BSP directory:
sudo ./flash.sh -k A_spe-fw jetson-agx-orin-devkit mmcblk0p1
```

QSPI hardware firewall blocks `dd` to the SPE partition from Linux
userspace, so host USB recovery is the only single-partition path.
For OTA / production deployments, use NVIDIA's UEFI capsule mechanism
(updates *all* bootloader components — see autoware_sentinel Phase
11.7 for the full procedure).

## Memory budget

256 KB BTCM is the hard limit. After link, run
`arm-none-eabi-size spe.bin`:

| Component                         | Approximate footprint |
|-----------------------------------|------------------------|
| FreeRTOS V10.4.3 FSP runtime      | ~80 KB                 |
| zenoh-pico (no TCP/UDP, IVC only) | ~50 KB                 |
| `nros-c` + `nros-node` core       | ~30 KB                 |
| Sentinel algorithms (subset)      | budget-driven          |
| App task stacks                   | configured per Config  |

If you blow the budget, `cargo bloat` against the rlib is the right
first investigation. Application-side compromises (reduced sentinel
set, no MPC) live in `autoware_sentinel`; this crate's job is to keep
the per-feature footprint reportable.

## What this crate does NOT do

- Build FreeRTOS from source. The FSP ships a prebuilt `ARM_CR5` port
  inside `tegra_aon_fsp.a` with NVIDIA-specific tweaks; we don't
  recompile it.
- Provide TCP/UDP networking. The SPE has no MAC; **IVC** (Phase
  100.4) is the only link transport. `zenoh_locator` must use the
  `ivc/` scheme.
- Supply a `#[panic_handler]`. The firmware crate that consumes this
  rlib picks its own (`panic-halt`, custom logger, etc.).
- Manage the SPE partition flash itself. See "Flash" above.

## See also

- [`docs/roadmap/phase-100-orin-spe-infra.md`](../../../docs/roadmap/phase-100-orin-spe-infra.md) — the
  whole phase including this work item.
- [`docs/roadmap/phase-100-04-link-ivc-design.md`](../../../docs/roadmap/phase-100-04-link-ivc-design.md) — the
  IVC link-transport wire spec, cited by the bridge daemon.
- [`packages/drivers/nvidia-ivc/`](../../drivers/nvidia-ivc/) — the
  IVC driver crate that backs `PlatformIvc`.
- [`packages/platforms/nros-platform-orin-spe/`](../../platforms/nros-platform-orin-spe/) — the
  thin trait-impl crate.
