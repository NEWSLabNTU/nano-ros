# `nvidia-ivc`

NVIDIA Tegra IVC (Inter-VM Communication) driver. Self-contained — no
nano-ros-internal deps. Two compile-time backends:

| Feature      | Backend                                | Target      |
|--------------|----------------------------------------|-------------|
| `unix-mock`  | `UnixDatagram::pair()` (SOCK_DGRAM)    | Linux host  |
| `fsp`        | the FSP's `tegra_ivc_channel_*` (`NV_SPE_FSP_DIR`) | Orin SPE (Cortex-R5, softfp) |

Default build (no features) is an inert stub — every entry point fails
closed. Build with exactly one backend feature; the two are mutually
exclusive.

## Why two backends

The `fsp` backend needs NVIDIA's SPE FSP and an Orin to run on. The FSP
itself is open — BSD-3-Clause, with MIT FreeRTOS — and downloads without an
account, inside the Jetson Linux public sources
(`Linux_for_Tegra/source/spe-freertos-bsp.tbz2`); phase-418 §2a records the
survey. The `unix-mock` backend reproduces the IVC frame semantics
(fixed 64-byte SOCK_DGRAM datagrams, message boundaries preserved)
on plain Linux, which is enough for:

- CI integration tests (this crate's `tests/loopback.rs`)
- The `autoware_sentinel` Stage 1 POSIX dev path
  (FreeRTOS POSIX sentinel ↔ Linux IVC bridge daemon over a
  Unix-socket pair)

The same Rust API ([`Channel`](src/lib.rs)) and the same C ABI
(`nvidia_ivc_channel_*`) is exposed by both — callers don't branch.

## Quick test

```sh
cargo test -p nvidia-ivc --features unix-mock
```

The `loopback` test wires two channel IDs to one socketpair via
`unix_mock::register_pair`, exchanges single-frame and multi-frame
payloads, and asserts byte-perfect reassembly.

## Building for SPE — not supported

Orin SPE is not supported for now
([phase-418](../../../../docs/roadmap/archived/phase-418-orin-safety-island-and-spe.md)),
and the `fsp` backend's LINK MODEL is known wrong: `build.rs` expects
`$NV_SPE_FSP_DIR/lib/libtegra_aon_fsp.a`, but the FSP builds no static library
(`make bin_t23x` compiles its objects straight into `spe.elf`). The `tegra_ivc_*`
declarations in `src/fsp.rs` do match the FSP's `include/tegra-ivc.h`. How the
symbols are supplied was phase-418 item 418.6 — an integration shell that adds
nano-ros to the FSP's own make build — closed with the phase on 2026-11-01, so
nothing decides it now. The target was decided by measurement: the FSP
compiles `-mfloat-abi=softfp`, which is `armv7r-none-eabi`, not `-eabihf`.

`just check ivc-fsp` compiles this backend against a stand-in tree meanwhile.

Consumed by `zpico-link-ivc`, which forwards zenoh-pico's `Z_FEATURE_LINK_IVC`
`_z_*_ivc` symbols to this crate's C ABI.
