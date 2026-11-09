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

## Measured: the QEMU version is not the cause (2026-11-10, newslab-118, 48 cores)

An A/B through the harness: `sched_dims_applied`, which starts and stops its
own router, run 10 times per arm on one fixture built from main. The QEMU was
chosen with `QEMU_SYSTEM_AARCH64`. The a53 boot resolves its binary through
`qemu_system_path` (the plan's `tools`), so each arm ran the binary it names.

- Arm 6.2 is Debian's `/usr/bin/qemu-system-aarch64` 6.2.0, with the host's
  libslirp 4.6.1.
- Arm 11.0.0 is the pinned fork, `nano-ros-v11.0.0-patches` (`7689da2`), built
  with `--target-list=aarch64-softmmu --enable-slirp` and its own libslirp, as
  the store dist bundles it.

| host load | 6.2 ACCEPT | 11.0.0 ACCEPT |
| --- | --- | --- |
| idle (load avg about 1.6) | 10/10 | 10/10 |
| 24 of 48 cores busy | 8/8 valid* | not measured* |
| 48 of 48 cores busy (load avg 22, rising to 48) | 10/10 | 9/10 |

\* Two 6.2 runs and every 11.0.0 run of that round were invalid. A branch
switch in the worktree mid-run made the fixture STALE, so they never booted.
They are not counted.

The one failure at full load is this issue's shape, on the PINNED QEMU:
`Slirp: Failed to send packet, ret: -1` five times, and then no core-pin
marker (run 6 of the 11.0.0 arm). So swapping the Debian 6.2 binary for the
store build does not remove the hang. Adding `aarch64-softmmu` to
`[tool.qemu.source]` may still be worth doing for the one-ladder rule, but it
is not this fix, and the acceptance's "names the QEMU it needs" branch does
not apply.

The rate tracks the host more than the QEMU. It is 6 in 10 on the 4-core
ros2 box (table above), 0 in 20 on an idle 48-core host, and 1 in 20 with
every core busy. The next steps are unchanged: `-gdb` on a hung boot, and
`cpus=1`, both best on a small or loaded host where the hang reproduces.

## Not known yet

Where the guest is stuck: in the e1000 driver, in the net stack's TCP connect
under SMP, or in the zenoh-pico open. Next steps:
- attach `-gdb` on a hung boot and read both cores' PCs;
- try `cpus=1` on the same image, to see whether SMP is needed for the hang;
- ~~try the store's pinned QEMU in place of the Debian 6.2 one~~: done above; it hangs too.

## Acceptance

- `CorePinPlacement/zephyr/c` passes 20 runs out of 20, solo and in
  `sched_dims_applied`.
- Or, if the hang is in QEMU 6.2, the cell names the QEMU it needs and
  refuses another one.
