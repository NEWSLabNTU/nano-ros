---
id: 2384
title: "The Zephyr qemu_cortex_a53 SMP realtime image hangs at its first network send about half the time"
status: open
type: bug
severity: medium
area: [zephyr, testing, qemu]
related: [phase-466]
found: 2026-11-10
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**.
The cell is tier 2's `CorePinPlacement/zephyr/c` (`sched_dims_applied_e2e`),
so a hang here is a red with no verdict on the SMP placement it exists to
measure.

## Symptom

The image (`workspace-zephyr-c-realtime-smp`, build dir
`build-ws-c-realtime-entry-smp`, board `qemu_cortex_a53/qemu_cortex_a53/smp`)
boots, brings up the network and the second core, and then stops:

```text
*** Booting Zephyr OS build v3.7.0 ***
<inf> net_config: IPv4 address: 10.0.2.15
Secondary CPU core 1 (MPID:0x1) is up
qemu-system-aarch64: Slirp: Failed to send packet, ret: -1
qemu-system-aarch64: Slirp: Failed to send packet, ret: -1
qemu-system-aarch64: Slirp: Failed to send packet, ret: -1
```

No `nros` line follows, and no `ConnectionFailed`. The cell reports
`NO RUNTIME OUTPUT: 16 non-empty line(s), none from the nano-ros runtime`.

## Measured (2026-11-09/10, ros2 box, QEMU 6.2.0 Debian, 4 cores)

All runs used one fixture, built once.

| harness | runs | ACCEPT |
| --- | --- | --- |
| main (`ad54791aa1`/`8e89470860`) | 10 | 6 |
| PR #3360 (descriptor boot) | 14 | 7 |
| solo `nros run --plan`, router on `0.0.0.0:7591` started by hand, 30 s | 6 | 1 |
| solo `nros run --plan`, no router | 6 | 0 (expected: `ConnectionFailed`) |

- The rate is the same on main and on #3360. The QEMU argv is the same apart
  from where `-smp cpus=2` sits:
  `-cpu cortex-a53 -machine virt,gic-version=3 -nic user,model=e1000 -smp cpus=2`.
- The solo run with a router rules out interference from the other cells in
  `sched_dims_applied`. It also rules out the router being late, since the
  router listened before QEMU started.
- With no router, the same `Slirp: Failed to send packet` lines appear, and
  then zenoh-pico reports `ConnectionFailed` about 1 s in. With a router, the
  guest prints neither the session nor the failure. It goes silent after the
  same lines.
- The image's locator is `tcp/10.0.2.2:7591` (from `.config`).

## Not known yet

Where the guest is stuck: in the e1000 driver, in the net stack's TCP connect
under SMP, or in the zenoh-pico open. Next steps:
- attach `-gdb` on a hung boot and read both cores' PCs;
- try `cpus=1` on the same image, to see whether SMP is needed for the hang;
- try the store's pinned QEMU in place of the Debian 6.2 one.

## Acceptance

- `CorePinPlacement/zephyr/c` passes 20 runs out of 20, solo and in
  `sched_dims_applied`.
- Or, if the hang is in QEMU 6.2, the cell names the QEMU it needs and
  refuses another one.
