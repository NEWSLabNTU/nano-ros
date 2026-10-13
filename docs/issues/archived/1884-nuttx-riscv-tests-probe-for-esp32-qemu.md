---
id: 1884
title: "NuttX-riscv tests report `qemu-system-riscv32 not found` on a host with a working one: they ask the ESP32 probe, which demands the Espressif fork's `esp32c3` machine"
status: resolved
type: bug
severity: medium
area: [testing, nuttx, ci]
related: [1525, 0488, 1851]
found: 2026-10-14
resolved: 2026-10-14
---

## Measured

On `main` (`d7909df96a`), in a clean environment (`env -i … source ./activate.sh`),
`c_riscv_nuttx_talker_delivers_cross_process` fails in 13 ms:

```
[UNMET PRECONDITION] qemu-system-riscv32 not found
```

`realtime_tiers` reports the same for its three `nuttx-riscv` rows. The host has
`/usr/bin/qemu-system-riscv32`, Ubuntu's `qemu-system-misc` (QEMU 9.0.2), whose
`-machine help` lists `virt`.

The same test PASSED in the 2026-10-13 tier-2 run on this host. That run's
`PATH` reached a user-installed Espressif QEMU in `~/.local/bin`, which lists
`esp32c3`. The probe is the same code at both commits; the result depends on
which binary `PATH` finds.

The probe is the only thing wrong. With `QEMU_SYSTEM_RISCV32` pointed at a shim
that answers `-machine help` with an extra `esp32c3` line and otherwise runs
`/usr/bin/qemu-system-riscv32`, the test passes in 3.4 s on the stock QEMU.

## Cause

Both NuttX-riscv tests guard their boot with
`nros_tests::esp32::is_qemu_riscv32_available()`:

- `packages/testing/nros-tests/tests/c_riscv_nuttx_e2e.rs:48`
- `packages/testing/nros-tests/tests/realtime_tiers_e2e.rs:555` (`Boot::NuttxRiscv`)

That probe exists for ESP32-C3 (issue 0488). It resolves through the
`esp32-qemu` store row, then `PATH`, and passes only if the binary lists the
`esp32c3` machine. A NuttX rv-virt image never uses that machine:
`QemuProcess::start_nuttx_riscv` boots `-M virt` through the generic
`qemu_system_cmd("qemu-system-riscv32")` ladder, which a stock QEMU satisfies.

So the probe and the boot resolve different binaries and ask different
questions. Two things make it fail almost everywhere:

1. **Issue 1525 made ESP32 dormant (2026-10-07)** and commented out
   `[tool.esp32-qemu]` in `nros-sdk-index.toml`. `nros setup` can no longer
   provision the Espressif fork, and since issue 1851 the store rung is built
   from the pin, so there is no store rung at all.
2. **The CI runner image installs apt `qemu-system-misc`**
   (`scripts/ci/runner-container.sh`), the stock build with no `esp32c3`.
   `[tool.qemu]`'s dist builds `arm-softmmu,riscv64-softmmu` only, so it
   supplies no riscv32 binary either.

So `nuttx-riscv,c,zenoh`, a tier-2 coordinate, reports an unmet precondition on
every host without a hand-installed Espressif QEMU. `unmet!` fails the test, so
the lane stays red with nothing measured.

## Fix

- `nros_tests::qemu::qemu_system_has_machine(exe, machine)` asks the binary
  `qemu_system_path` resolves, the one the boot runs, whether it offers the
  machine. `is_qemu_riscv32_virt_available()` sits beside
  `start_nuttx_riscv` and asks for `virt`.
- `c_riscv_nuttx_e2e` and `realtime_tiers`' `Boot::NuttxRiscv` use it. The
  ESP32 probe stays for ESP32 only; its remaining callers
  (`esp32_emulator.rs`, `logging_smoke.rs`) boot ESP32 images.
- Gate: `check-qemu-one-ladder` now also refuses a Rust file that calls the
  ESP32 probe without booting an ESP32 image (`start_esp32_qemu`). It has
  self-test cases, and putting the old call back in `c_riscv_nuttx_e2e.rs`
  fails it.

Sweep: `git grep -n "is_qemu_riscv32_available\|require_qemu_riscv32" -- '*.rs'`

## Support status of riscv32 (as of 2026-10-14)

| Target | Status | Lanes | QEMU |
|---|---|---|---|
| NuttX rv-virt (`nuttx-riscv`, 5 fixture rows) | **Supported** | tier 2: `c`; tier2-nightly: `c`, `cpp`, `rust` | stock `qemu-system-riscv32 -M virt` (apt `qemu-system-misc`; no store dist) |
| ESP32-C3 (`esp32-qemu`) | **Dormant** (issue 1525): code kept, nothing builds, tests or provisions it | none | Espressif fork, `esp32c3` machine; index row commented out |

ThreadX is riscv**64** (`threadx-riscv64`, store `qemu-system-riscv64`) and is
not affected.
