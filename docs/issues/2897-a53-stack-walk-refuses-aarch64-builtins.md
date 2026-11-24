---
id: 2897
title: "The Zephyr stack-floor walk refuses every root on a 64-bit Arm (qemu_cortex_a53) image: no aarch64 libgcc builtins row, no frames for the arm64 arch entry points"
status: open
type: limitation
area: [zephyr, build, stack]
severity: medium
found: 2026-11-23
related: [phase-582]
---

## What happens

A `qemu_cortex_a53` Zephyr image (zephyr-sdk 0.16.9, `clock.source = sntp,platform`,
`clock.sntp_resync_interval_ms = 60000`, `NET_SOCKETS_SERVICE_STACK_SIZE=4096`) builds
rc=0, but `nros-stack-floor` refuses the walk for every root. `report.json` holds
`roles: {}` and only the stated stacks (`OPEN` 32768, `SNTP_RESYNC` 8192). Measured on
host 142, 2026-11-23, while checking the SNTP re-sync stack knob in flight on #4632.

The walk reaches functions that have no frame in any of its inputs:

- **libgcc outline atomics**, from the SDK's `aarch64-zephyr-elf/12.2.0/libgcc.a`
  (code key `9c13b2fe60cc`): `__aarch64_cas8_acq_rel`, `__aarch64_ldadd4_acq_rel`,
  `__aarch64_ldadd8_acq_rel`, `__aarch64_ldclr8_acq_rel`, `__aarch64_ldset8_acq_rel`,
  `__aarch64_swp8_acq_rel`. `config/stack-builtins.toml` has no row for that key.
- **arm64 arch entry points**, which are assembly and give the walk no frame:
  `__start`, `arch_cpu_idle`, `arm_smccc_hvc`, `arm_smccc_smc`, `z_arm64_context_switch`,
  `z_arm64_exit_exc`, `z_arm64_fpu_save`.

So no 64-bit Arm Zephyr image gets a stack floor, for the main thread or for any other
role. The check still cannot break the build: a role with no `config/stack-floors.toml` row
warns rather than refuses.

## The lower bound it prints is not a size

With every missing frame counted as 0, the walk reports at least 146,896 B from
`nros_zephyr_sntp_resync_loop` and at least 147,392 B from the main root (against 32,768).
The deepest frames on the re-sync path are
`nros_node::parameter_services::read_parameter_value_into` (8,592 B),
`create_client_trampoline<ZenohRmw>` (4,672 B) and `Executor::open_in` (4,096 B). The
re-sync thread reaches none of those: the walk gets there through bound indirect calls.
mps2's 179 KB has the same shape. Do not size `clock.sntp_resync_stack_bytes` or
`CONFIG_MAIN_STACK_SIZE` from it.

The 8,192 B re-sync default rests on a consumer's measured 7,024 B peak on a 64-bit Arm
Zephyr 3.7 image, plus the table's 10 % margin (#4632).

## Fix

1. Measure the archive: `just zephyr stack-builtins <sdk>/aarch64-zephyr-elf/lib/gcc/aarch64-zephyr-elf/12.2.0/libgcc.a`,
   and commit the generated row in `config/stack-builtins.toml`.
2. Give the arm64 arch entry points frames, the same way the Cortex-M ones get theirs, so
   the walk stops refusing on them.
3. Record an `[[observed]]` row in `config/stack-floors.toml` for the a53 image's
   `SNTP_RESYNC` role, from the boot-time stack peak line, so the role is checked against a
   measurement rather than the over-bound walk.

### Progress

- **Step 1 is done.** `scripts/lib/stack_builtins.py` reads A64: pre-indexed
  stores and `sub sp, sp, #N` (with `lsl #12`) count; post-indexed loads,
  `add sp` and `mov sp` restore; `bl`/CALL26 is a call, `b`/JUMP26 a tail
  jump; `blr`/`br` through a register are unmeasured. The row for the SDK
  0.16.9 `aarch64-zephyr-elf/12.2.0/libgcc.a` (code key `9c13b2fe60cc`) is
  measured: 223 of 260 functions have a frame, and the six outline atomics the
  walk reached are each 0 (leaves). The other 37 say why: they call `abort`
  or `malloc`, or branch through a register.
- **Rebuilt with step 1's row (host 142, 2026-11-24).** The outline atomics
  no longer refuse. The walk still refuses on the arm64 assembly alone:
  `__start`, `arch_cpu_idle`, `arm_smccc_hvc`, `arm_smccc_smc`,
  `z_arm64_context_switch`, `z_arm64_exit_exc` and `z_arm64_fpu_save`. With
  those counted as 0 it reaches at least 146,368 B from main (against 32,768)
  and at least 144,272 B from the SNTP root (against 8,192). Those are the same
  over-bound shape as before, so they are not sizes.
- **Step 2 is next, and its shape is open.**
  - Cortex-M's assembly is reached only through over-approximate bound edges,
    which skip a frameless target.
  - arm64's is reached by direct calls, so the walk refuses.
  - Candidate: measure a frameless function defined in an assembled object
    with step 1's A64 decoder, as `apply_linked_compiler_builtins` does for
    `compiler_builtins`.
  - Caveat: `z_arm64_context_switch` and `z_arm64_exit_exc` move `sp` between
    stacks, which a frame decoder must not read as a frame. Those want a stated
    frame with a reason, not a decoded one.
  - Needs the seven functions' disassembly from an a53 build.
- The a53 analyzer boot did not run: the board needs a tap NIC, and
  `/dev/net/tun` is not permitted on host 142.

Acceptance: the same a53 build's `report.json` has an `SNTP_RESYNC` role with a floor, and
the main root's floor is either checked or refused for a reason other than a missing frame.
