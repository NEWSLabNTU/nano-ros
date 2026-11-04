---
id: 2209
title: "An idle mps2-an385 image costs a host core under the SDK's QEMU 11 (wfi returns at once), and 2 % under QEMU 9"
status: resolved
type: bug
severity: medium
area: [testing, platform]
related: [phase-521, 2022]
found: 2026-11-02
---

## Symptom

phase-521 W5 gave the mps2-an385 serial link a tickless idle: UART0's RX
interrupt plus a SysTick one-shot under `wfi`, the same shape issue 2022 gave
the LAN9118. To accept it, the idle serial listener (`qemu-serial-listener`,
connected through a socat PTY pair to `rmw_zenohd`'s serial listener, no
talker) was measured by host CPU of the QEMU process over 10 s:

| QEMU | W5 idle hook | Host CPU |
|---|---|---|
| SDK `11.0.0-nros7` (what the harness runs) | on | 120–128 % |
| SDK `11.0.0-nros7` | off (hook pointer cleared with gdb) | 123 % |
| distro `9.0.2` | on | 0.8–2.2 % |
| distro `9.0.2` | off | 0.9 % |

So the cost is a property of QEMU 11 with a connected PTY chardev, not of the
guest's wait: the hook changes nothing measurable on either QEMU. Without a
connected chardev (`-serial null`, the session never opens) both QEMUs idle at
about 5 %. Per thread under QEMU 11: the vCPU thread ~70 %, the main loop
~42 %.

## What was seen inside the guest

Sampling with gdb under QEMU 11 puts the guest in `tickless::uart::wait_idle`
and around it, and a breakpoint after the `wfi` shows it returning long before
the SysTick one-shot expires with nothing pending (`ICSR` 0x800, `ISPR0` 0,
only IRQ 0 enabled). That points at `wfi` not halting the vCPU, but gdb's
breakpoints perturb exactly this, so it is a lead rather than a finding.

## Consequence

The phase-521 W5 acceptance, "QEMU idle under 2 % host CPU per serial cell",
cannot be asserted in a lane that runs the SDK's QEMU 11. The W5 change was
landed on the pair tests (`test_qemu_serial_pubsub_e2e`,
`test_qemu_xrce_pubsub_e2e`) and this measurement, without the CPU assertion.

## Not about the serial chardev (2026-11-02)

The network link shows it too. The idle `qemu-bsp-listener` (zenoh over the
LAN9118, phase-521 W5's executor park) arms its SysTick one-shot 175,157
times in 20 s under `11.0.0-nros7` and 41 times under `9.0.2`. At each arm
nothing is pending (`ICSR` 0x800, `ISPR0` 0, only IRQ 13 enabled) and the
requested one-shot is ~0.96 s, decreasing by ~5.7 us per arm: the `wfi`
returns at once. So the defect is QEMU 11's armv7m `wfi` on this machine,
whatever the link.

## Next

Bisect between QEMU 9.0.2 and the fork's 11.0.0-nros7 with this image, or read
the CMSDK UART and armv7m NVIC changes in that range for a line that keeps
`cpu_has_work` true while a PTY chardev is attached.

## Resolution (2026-11-04)

A QEMU defect, not ours, and already fixed upstream. QEMU 11.0.0's
`d238858bff` ("Implement WFE, SEV and SEVONPEND for Cortex-M") sets
`env->event_register` on every exception return (ARM ARM R_BPBR), and
`arm_cpu_has_work` returned true for an M-profile CPU whenever it was set.
Only `WFE` clears it, so a guest that waits with `WFI` alone (every nano-ros
idle hook) had work pending forever after its first interrupt: `HELPER(wfi)`
returned at once and the vCPU thread never halted. That is the "nothing
pending" the gdb samples saw — the pending thing was the event register,
which no guest register shows. QEMU 9.0.2 predates the WFE work, so it
halted.

Upstream `6fd2fcdc61` ("teach arm_cpu_has_work about halting reasons",
after v11.0.0) records why the CPU halted and lets the event register wake
only a `WFE`. It is cherry-picked onto the fork's patch line as
`7689da2d98` (conflicts only in unrelated GICv5 / migration context), the
`third-party/qemu/qemu` pin moves to it, and the SDK dist is re-cut as
`qemu 11.0.0-nros8`.

Measured, the fresh idle `qemu-bsp-listener` under its router, host CPU of
the QEMU process over 10 s after a 5 s settle:

| QEMU | Host CPU |
|---|---|
| SDK `11.0.0-nros7` | 120 % |
| fork + `7689da2d98`, local build | 0 % |
| SDK `11.0.0-nros8` (released dist) | 0 % |
| distro `9.0.2` | 0 % |

`nros-tests --test emulator` on the patched binary: 21/21 pass (bsp, RTIC
pubsub/service/action/mixed-priority, serial, XRCE, C/C++ API cells).

Measured on the LAN9118 link only; the serial cell shares the root cause
(the same `wfi` in its idle hook) and its pair test passes, but its idle
CPU was not re-measured.
