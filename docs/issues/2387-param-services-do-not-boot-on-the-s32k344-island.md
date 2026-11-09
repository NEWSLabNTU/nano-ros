---
id: 2387
title: "The safety island's image with parameter services on does not boot on the S32K344: main overflows at 16 KiB, and the session does not open with a larger main"
status: open
type: bug
area: [platform, embedded, params]
severity: medium
found: 2026-11-10
related: [phase-512, phase-446, phase-474, 0852]
---

**Home: [phase-512](../roadmap/phase-512-on-hardware-measurements.md)** H10. Found
while measuring H10 on the MR-CANHUBK344 (island main 6b09fa7, nano-ros
ac23c0155b, serial zenoh link through the demo's gateway). The image is the
island's `build-board` image with `features = ["params", "param_services"]` in
`src/safety_island_bringup/system.toml`: three nodes, 21 declared parameters,
six parameter services per node (18 queryables).

## What the services cost, statically

Against the same image without the services (linker table and ELF):

| region | without | with | delta | where |
| --- | ---: | ---: | ---: | --- |
| DTCM | 117,984 | 137,312 (does not link) | +19,328 | `__nros_levels_backing` 33,128 -> 52,448 B: the executor's parameter home (`ParamHome`: the state with its inline service sets and the request/reply buffer pair) |
| RAM (SRAM) | 262,976 | 291,216 | +28,240 | `BUILTIN_INBOX` 19,800 B (18 x 1,100 B), `SERVICE_BUFFERS` 1,296 -> 7,128 B, `g_sessions` +2,008 B, the POSIX mutex and cond pools +576 B |
| FLASH | 690,188 | 724,176 | +33,988 | |

DTCM overflows by 6,240 B, and the image's 8 KiB DTCM guard
(`dtcm-headroom.ld`) asks for 14,432 B more. Moving the trace buffer to SRAM
instead overflows RAM by 16,792 B. The measurement image below shrinks the
trace buffer (`CONFIG_RAM_TRACING_BUFFER_SIZE` 53,248 -> 36,864) to link:
DTCM 120,928 of 131,072 B, RAM 291,216 of 327,680 B. The configure also
asks for `CONFIG_MAX_PTHREAD_COND_COUNT` 19 -> 37 (7 subscribers + 22
queryables + 8) and the mutex pool was raised to match (39 -> 57).

## Failure 1: main overflows during registration at 16 KiB

With the island's `CONFIG_MAIN_STACK_SIZE=16384` the image opens its session
and stops at stage 4 (`RegisteringEntities`, 2 arena allocations, heap peak
39,352 of 94,720 B). The core halts in `arch_system_halt` with
`K_ERR_ARM_USAGE_ILLEGAL_EPSR` in the idle thread: `z_main_stack` reads no
`0xaa` paint left (16,384 of 16,384 B used) and the idle stack directly
below it is overwritten. Without the services the same image's main peaks at
13,552 B (read on the board after an act, `h10-b24`), so the registration
path with the services needs more than the 2,832 B left.

The QEMU island shows the same overflow with a name on it. The demo's QEMU
image (the island's `trace-qemu-build` recipe, mps2/an385, 1 MiB heap, the same 16 KiB
main, the services on; run `h10-q1`) prints
`parameter store: 22 slots (8976 B) + state and services (26576 B) carved
from the executor backing`, registers
`/mrm_comfortable_stop_operator/get_parameters` and `.../set_parameters`,
and then halts with `MPU FAULT`, `ZEPHYR FATAL ERROR 2: Stack overflow on
CPU 0`, current thread `main`, the faulting instruction in
`__aeabi_memcpy4` writing 0x20245a00 (the main stack's guard). A large
by-value copy onto main. The likely one:
`Executor::build_parameter_service_set` (nros-node
`src/executor/spin.rs`) returns a whole `ParameterServiceServers` by value,
and `reconcile_parameter_services` then pushes it into the state's inline
`heapless::Vec`, so a set of six servers is moved through main's frames
(issue 0756 already forbids that for `ParamState`). Building the set in
place, in the carved slot, is the fix direction. `MAX_SERVICE_SETS` is
`MAX_NODES`, 4 here for 3 nodes, so one carved set is never used.

## Failure 2: with a larger main, the session does not open

Raising main to 24 KiB (also with the nano-ros heap at 102,400 B) or 32 KiB,
the image loops at stage 2 (`BootConfigResolved`, `LAST ERROR Transport`,
heap peak 2,032 B): the gateway logs `New transport opened` and then
`Unexpected Init flag in message` every two seconds (126 and 65 in a 120 s
join wait, run `h10-e6`). The same image without the services and a 24 KiB
main joins and passes the encore act (`h10-b24`), and the services image with
a 16 KiB main and the larger heap opens the session and reaches stage 4
(`h10-e5`, failure 1). So the open fails only with both: the services and a
main above 16 KiB. Not yet explained; the console (lpuart0) is not wired on
this bench.

## Also seen: after a fatal halt, a system reset does not recover the board

After failure 1, every later boot through `pyocd reset` (SYSRESETREQ), of
any image including the shipped one, faults at the first interrupt unlock
(`arch_switch_to_main_thread`) in `EDMA_HandleIRQ` with a MemManage data
access at 0x4008 (an eDMA channel register offset off a NULL base): a
channel interrupt is pending before its handle exists. `pyocd reset -m hw`
(the reset pin) clears it, and the shipped image then boots and passes
(`h10-b1`). Worth a guard in the serial link's DMA bring-up (clear or
disable the channels before the IRQs are enabled), or at least a note in the
board runbook.

## Closes when

The island's board image with parameter services on boots, joins the demo
and passes an act, with its main stack peak and heap peak recorded in
phase-512 H10; or the services are declared out of scope for the S32K344
image and H10 is narrowed to the targets that can carry them.
