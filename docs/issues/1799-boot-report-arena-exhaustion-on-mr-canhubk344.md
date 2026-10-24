---
id: 1799
title: "The boot report's arena-exhaustion record has never been dumped from silicon -- run it on the MR-CANHUBK344"
status: open
type: test-gap
area: core, boards, testing
severity: medium
found: 2026-10-11
related: [issue-1036, issue-1640, phase-412, phase-474, phase-512]
---

**Home: [phase-512](../roadmap/phase-512-on-hardware-measurements.md)** — H7, moved 2026-10-24 from phase-474, where it had been homed since 2026-10-17.

## What is missing

Carved out of issue 1036, whose every other part is done or tracked there.
`boot_report` exists for a board with no console -- the MR-CANHUBK344
(`mr_canhubk3/s32k344`), where `lpuart0` is not wired and `lpuart2` carries
the zenoh serial transport, so an arena exhaustion's log line reaches nothing
and the 120-byte RAM record is the only witness. Every step of that path has
been exercised except the one that runs on the board:

- host: `executor::tests::an_exhausted_arena_decodes_to_the_knob_an_operator_must_set`
  (symbol resolution, dump length, decode, verdict) and
  `arena_exhaustion_reaches_the_fatal_hook`, both in `just check node-std-tests`;
- QEMU, as a lane since 2026-10-11:
  `zephyr_cortex_m_qemu::zephyr_cortex_m_c_boot_report_names_arena_exhaustion`
  builds the same console-less `mps2/an385` image (fixture row
  `build-cortex-m-c-boot-report-zenoh`), dumps the record with the QEMU
  monitor's `pmemsave`, decodes it with `read-boot-report.py`, and asserts the
  guest halted in `arch_system_halt` (issue 1036, resolution).

Not done: the same image built for the S32K344, halted, and dumped with a
probe. This is a hardware run; no lane here has a board or a probe.

## The run

Build `examples/zephyr/c/talker` (zenoh) for `mr_canhubk3/s32k344` with this
extra conf (the same one the QEMU run used):

```
CONFIG_CONSOLE=n
CONFIG_UART_CONSOLE=n
CONFIG_LOG=n
CONFIG_NROS_BOOT_REPORT=y
CONFIG_NROS_ARENA_EXHAUSTION_IS_FATAL=y
CONFIG_NROS_EXECUTOR_ARENA_SIZE=32
```

`CONFIG_NROS_EXECUTOR_ARENA_SIZE=32` is exhausted on purpose: the image's
first registration claims 48 B. Then:

```
python3 scripts/read-boot-report.py --addr-only <zephyr.elf>     # -> <addr> <len>
pyocd commander -t s32k344 -c halt -c "savemem <addr> <len> report.bin"
python3 scripts/read-boot-report.py <zephyr.elf> report.bin
```

## Acceptance

The decode names the exhaustion, as it did on QEMU:

```
stage      4  RegisteringEntities -- an entity claimed arena; registration in flight
error-level log records      1   (the first raised at packages/core/nros-node/src/executor/arena.rs:677)
ARENA EXHAUSTED: an allocation of 48 bytes did not fit, short by 16 bytes.
  set NROS_EXECUTOR_ARENA_SIZE >= 48
```

(The `arena.rs` line number moves with the source; the stage, the shortfall
and the knob are what must match.) Plus the negative control:
`CONFIG_NROS_ARENA_EXHAUSTION_IS_FATAL=n` runs on with the entity missing, and
the record still names the exhaustion. Record both outputs here, with the
image's commit, and close the issue.

If the transport fails before registration, the record says so instead
(`stage 2, Transport ConnectionFailed` on QEMU, with the router not yet bound):
that is the record naming a different failure correctly, so re-run with the
peer up.
