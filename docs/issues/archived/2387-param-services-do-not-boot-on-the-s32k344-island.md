---
id: 2387
title: "The safety island's image with parameter services on does not boot on the S32K344: main overflows at 16 KiB, and the session does not open with a larger main"
status: resolved
type: bug
area: [platform, embedded, params]
severity: medium
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "PR #3409: the parameter service sets are built in place and carved one per declared node; eDMA v3 channels are quiesced at boot. The second failure was the platform heap, sized by the image."
related: [phase-512, phase-446, phase-474, 0852]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)** H10. Found
while measuring H10 on the MR-CANHUBK344 (island main 6b09fa7, nano-ros
ac23c0155b, serial zenoh link through the demo's gateway). The image is the
island's `build-board` image with `features = ["params", "param_services"]` in
`src/safety_island_bringup/system.toml`: three nodes, 21 declared parameters,
six parameter services per node (18 queryables).

## Resolution (2026-11-10)

Three causes, two of them nano-ros's, and the second failure was not what it
looked like.

**1. Main overflowed because the set of six was moved by value.**
`Executor::build_parameter_service_set` returned a whole
`ParameterServiceServers` (4,032 B on the island: six 648 B cffi service
handles and the node's name), and `reconcile_parameter_services` pushed it into
the state's inline `heapless::Vec`. The frames on thumbv7em: reconcile 10,260 B,
the builder 6,996 B, `create_param_srv` 1,000 B. Fixed: the set is built in
place in the state's next slot (`ParameterServiceServers::init_at`, each
handle written into its field as it is created, the ones already made dropped
on a part-way failure), and `create_param_srv` is never inlined. Now reconcile
2,248 B, the builder 312 B, `init_at` 2,128 B, `create_param_srv` 1,000 B. On
the board main peaks at 15,164 of 16,384 B with the services on (13,552 B
without them; paint read after the act).

**2. "The session does not open" was the platform heap, and SWT0 hid it.**
Sampled over SWD during the join wait (uptime, boot report, serial counters),
the board was not looping at stage 2: it reached stage 4 at about 1.1 s,
halted, and rebooted every 1.4 s. The halt is `nros_platform_panic` after
`nros_platform_alloc` failed (`CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL`): a
236 B `_z_slist_new` from zenoh-pico's declaration cache
(`_z_cache_declaration`, `_z_declare_resource`) while a queryable was being
declared, heap peak 87,552 of 94,720 B. The island's supervisor had already
started SWT0 at the first spin of a spawned level, so the halted core was
reset by the watchdog 600 ms later. The new boot opened a new serial link,
which the gateway logged as `Unexpected Init flag`; the act's readout came
after the gateway was gone, from a boot that could not open a session, so it
read stage 2 and `ConnectionFailed`. A larger main changed nothing because the
heap was the limit, not the stack. Not a nano-ros defect: the services need
more heap than the image gave them. Measured with a heap that holds them
(`heap_size = 129536`, run `i2387-h2`): 104,096 B at FirstSpin (41,232 B
without the services) and 119,648 B at the act's end (68,272 B without).

**3. A warm reset after a fatal halt faulted in `EDMA_HandleIRQ`.** A
functional reset (SYSRESETREQ) does not reset the S32K344's eDMA, and the MCUX
`dma3` HAL's `EDMA_Init` programs `MP_CSR` only (the older eDMA HAL clears
ERQ, INT and ERR), so a channel the previous image used keeps its request
enable and interrupt flag, and the driver enables the channel IRQs before any
handle exists. Fixed: `CONFIG_NROS_EDMA_WARM_RESET_QUIESCE` (default y with
`DMA_MCUX_EDMA_V3`), a PRE_KERNEL_2 hook that disables every channel's
request, clears DONE, ERR and INT and the controller's NVIC pending bits. On
the board: `i2387-h2` booted from a plain `pyocd reset` after `i2387-h1`
halted on heap exhaustion, the sequence that faulted in `h10-e2`. The demo's
run-board now resets by the reset pin as well.

Also: the parameter state carried one set per node-TABLE slot
(`MAX_SERVICE_SETS` = `MAX_NODES` = 4), and the table holds a slot for the
contract reporter's session-named node that never gets a set. It now carries
`PARAM_SERVICE_SETS`: one per node the contract declares when it declares
every node's parameters (the rows of `DECLARED_PARAM_SERVICE_SHAPES`, the same
count the builtin inbox and the queryable table use), else the table's bound.
4,032 B less executor backing: DTCM 120,928 -> 116,896 B on the island.

### What the services cost the island, after the fix

| | without the services | with them (`i2387-h2`) |
| --- | ---: | ---: |
| DTCM | 117,984 B (trace 53,248 B) | 116,896 B (trace 36,864 B): +15,296 B besides the trace |
| RAM (SRAM), heap 94,208 B | 262,976 B | 291,216 B (+28,240 B) |
| heap peak at FirstSpin / at the end | 41,232 / 68,272 B | 104,096 / 119,648 B |
| heap the image needs (peak + the 24,576 B floor) | 92,848 B | 144,224 B |
| main stack peak | 13,552 of 16,384 B | 15,164 of 16,384 B |

The board image cannot carry them within its own budgets: a 144 KiB heap
does not fit beside the services' static SRAM (the largest heap that links is
129,536 B, SRAM 326,544 of 327,680 B, 10,400 B of headroom against the 24,576 B
floor), and with an 8 KiB DTCM guard the trace buffer drops from 52 to 40 KiB,
under the 45.4 KB the worst act needs to its readout. The island keeps the
services off on the board; the heap cost of each service (about 3.5 KiB at
boot) is where to look if it is to carry them.

### Runs (the demo's `run-board.sh encore`, planning simulator)

| run | image | result |
| --- | --- | --- |
| `i2387-b1` | services, 16 KiB main, nano-ros ac23c0155b | main overflow at stage 4 (the original failure 1) |
| `i2387-f1`, `i2387-n2`, `i2387-n3` | in-place sets, heap 94,208 B | reboot loop: heap exhausted at stage 4, SWT0 reset |
| `i2387-g1` | the same, 24 KiB main | the same loop (the stack was not the limit) |
| `i2387-s1` | SWT0 started later (a probe, not kept) | halted at stage 4, `HEAP EXHAUSTED` 236 B, peak 87,552 B |
| `i2387-h1` | heap 122,880 B | FirstSpin at 104,096 B, then exhausted (1,024 B) |
| `i2387-h2` | heap 129,536 B | **PASS**, heap peak 119,648 of 130,048 B, main 15,164 B |
| `i2387-q1` | the QEMU island (mps2/an385, 1 MiB heap) | **PASS**, heap peak 121,208 B |

## The measurement as filed

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
