---
id: 1851
title: "QEMU is located three different ways: the tests search env → build/qemu → store → PATH, the `just` recipes search one of those rungs each"
status: resolved
resolved: 2026-10-13
type: tech-debt
area: [build, testing]
severity: low
found: 2026-10-13
related: [0488, 0930, 1767, RFC-0095, RFC-0103, phase-484]
---

## Measured (2026-10-13)

Residue of issue 1767 item 10. The `[tool.qemu]` index row (`11.0.0-nros6`,
`nros-sdk-index.toml:153`) provisions QEMU into the store (`[tool.esp32-qemu]`
is commented out, DORMANT with ESP32 — issue 1525 — `nros-sdk-index.toml:888`);
`just native setup` instead installs the same pin into a project-local prefix (`just/native.just:235`: `nros setup --tool qemu --prefix
build/qemu`). Then each consumer looks in its own subset of places:

| consumer | rungs |
| --- | --- |
| `nros_tests::qemu::qemu_system_arm_path` (`packages/testing/nros-tests/src/qemu.rs:55`) | `$QEMU_SYSTEM_ARM` → `build/qemu/bin` → store `qemu` → PATH |
| `nros_tests::esp32::qemu_system_riscv32_path` (`packages/testing/nros-tests/src/esp32.rs:44`) | `$QEMU_SYSTEM_RISCV32` → store `esp32-qemu` → PATH |
| `just/freertos.just:4`, `just/nuttx.just:6` (`QEMU_BIN`) | `build/qemu/bin` only |
| `just/qemu-baremetal.just:8` / `:505` | `build/qemu` prefix, then PATH |
| `just/esp32.just:35,51,295`, `scripts/esp32/launch-esp32c3.sh:97` | PATH only |
| `just/threadx-riscv64.just:95`, `just/nuttx.just:662` | PATH only |
| `scripts/ci/runner-doctor.sh:497` | `build/qemu/bin`, then PATH |

So a host whose QEMU is in the store only (what `nros setup` produces) passes
the QEMU tests while `just freertos`/`just nuttx` run recipes find no binary,
and, once ESP32 is revived, the `just/esp32.just` recipes find the system `qemu-system-riscv32`
— which has no `esp32c3` machine, the exact case `esp32.rs:44`'s store rung was
added to fix (issue 0488). `build/qemu` is also one more home for the pin
besides the store (issue 0930: it can be older than the pin and nothing says so).

## Direction

One ladder (env override → store pin → PATH), reached by the shell through
`nros sdk-path qemu` / `nros locate` and by the tests through the same
`nros_store_bin`; retire the project-local `build/qemu` prefix in favour of the
store (RFC-0095 D2), or keep it only as an explicit override. Gate: no
`qemu-system-*` resolved outside the ladder.

## Fix (2026-10-13)

One ladder, everywhere — `QEMU_SYSTEM_<ARCH>`, the project-local
`build/qemu/bin`, the store at the PINNED version, then `$PATH`:

- **Rust:** `nros_tests::qemu::qemu_system_path(exe)` / `qemu_system_cmd` /
  `qemu_system_available`. `qemu_system_arm_path` is now a wrapper over it. The
  ESP32 resolver uses the same rungs over the `esp32-qemu` row, without the
  project prefix. The NuttX RISC-V, ThreadX RV64 and Zephyr AArch64 launchers,
  which ran bare `Command::new("qemu-system-…")`, and the FreeRTOS tiers
  test's PATH probe all go through it.
- **Shell:** `scripts/lib/qemu-bin.sh` — `nros_qemu_bin <exe> [tool]
  [noprefix]` and `nros_qemu_report <exe> <min-version>` (the doctor line).
  `just freertos`, `nuttx`, `qemu`, `threadx_riscv64` and `esp32`, the two
  launch scripts, `runner-doctor.sh` and the root `just doctor` reminder all
  ask it. `build/qemu` stays as rung 2: CI provisions it
  (`setup-qemu-patched`, `runner-provision.sh nros-qemu`), and the order
  between it and the store is now the same for every consumer.
- **A second defect, found on the way:** `nros_tests::nros_store_bin` returned
  the FIRST version directory `read_dir` listed, not the pin. On this host the
  store holds `qemu/11.0.0-nros2` and `11.0.0-nros6`. It now constructs
  `<store>/sdk/<tool>/<pin>/bin/<exe>` from `sdk_pinned_version` (issue 1546's
  rule). That also fixes its other caller, the XRCE agent.
- **Gate:** `check-qemu-one-ladder` (fast line) refuses a `qemu-system-*`
  launched or probed by bare name in shell, `just` or Rust code, including a
  `just` expression whose fallback is the bare name.
- **Doctors:** a binary below the ARM floor (7.2) now fails the `nuttx` doctor
  where it used to warn, the same as the `qemu` doctor.

## Verified

- On this host (no `build/qemu`; store `11.0.0-nros6`; distro QEMU 6.2):
  - before: `just qemu doctor` failed with "[MISSING] patched qemu-system-arm",
    and `just freertos`/`just nuttx` launched the distro 6.2;
  - after: every doctor reports `qemu-system-arm 11.0` and
    `qemu-system-riscv64 11.0` from the pinned store, and `just qemu qemu-bin`
    prints the store path.
- `QEMU_SYSTEM_ARM=/usr/bin/qemu-system-arm` makes the report fail with the
  version it found.
- `qemu_patched_binary` tests: 3/3.
- Negative control: the gate's matcher flags 36 sites across the 11 swept files
  at `origin/main` and 0 after.
