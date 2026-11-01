---
id: 2209
title: "An idle mps2-an385 serial image costs a host core under the SDK's QEMU 11, and 2 % under QEMU 9"
status: open
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

## Next

Bisect between QEMU 9.0.2 and the fork's 11.0.0-nros7 with this image, or read
the CMSDK UART and armv7m NVIC changes in that range for a line that keeps
`cpu_has_work` true while a PTY chardev is attached.
