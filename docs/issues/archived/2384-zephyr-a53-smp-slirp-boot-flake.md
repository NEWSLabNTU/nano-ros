---
id: 2384
title: "The Zephyr qemu_cortex_a53 SMP realtime image hangs at its first network send about half the time"
status: resolved
type: bug
severity: medium
area: [zephyr, testing, qemu]
related: [phase-466, 2389]
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "#3395 (CONFIG_NROS_CONSOLE_WHOLE_LINES) and #3400 (the a53 image's 10 s connect budget); the leftover shape is issue 2389"
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**.
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

## Measured: there is no hang, there are two failures (2026-11-10, ros2 box)

Twelve failing runs of the cell, all on one fixture built once, run under
`taskset -c 0-3` to reproduce the load. Every one falls in exactly one of two
classes, and neither is a QEMU, timer or scheduler defect.

| class | runs | what the guest log shows |
| --- | --- | --- |
| A: the image dies at boot | 6 | `zpico Session -> ConnectionFailed` at 2.08 s, then `RMW session open failed` and `NodeError::Transport(ConnectionFailed)` |
| B: the marker is split | 6 | the image runs normally; the ACCEPT marker is printed with `running_on=1`, but cut in two by another core's output |

### Class A: one connect attempt meets a lossy early network

A gdb dump of one hung boot (both cores halted in `arch_cpu_idle`, each
thread's backtrace unwound from its saved context) shows:

- `main` is DEAD (`thread_state` 0x8): `bg_thread_main` returned and
  `z_thread_entry` aborted it;
- the sched task thread was never started, and all 16 `posix_thread_pool`
  slots are unused, so no zenoh-pico read or lease task ever existed;
- the only live threads are `logging`, `sysworkq`, `net_mgmt` and `tcp_work`,
  each idle on its own queue.

That is the whole "hang". The kernel tick stops because nothing has a timeout
pending (the timeout list holds only the IPv6 router, prefix and address
lifetime timers, and CPU1's comparator is programmed to exactly the router's
deadline). CPU0's timer stays masked after its last interrupt, which is how
Zephyr 3.7's tickless `arm_arch_timer` behaves on SMP: only the core that last
calls `sys_clock_set_timeout` re-arms its own timer. The timer is not the
cause.

The harness does not race the router: `ZenohRouter::start_on` waits for the
router's TCP accept before the guest boots. The guest's connect budget is the
default single attempt (issue 2207: `CONNECT_TIMEOUT_MS = 0`; the
`realtime-c` fixture states neither `connect_timeout_ms` nor
`connect_forever`). The guest log shows `Slirp: Failed to send packet` during
its first two seconds; when that loss lands on the open handshake under host
load, the single attempt fails before slirp's TCP recovers, and `main` returns.

### Class B: two console paths interleave on SMP

The marker is printed in every class-B run. For example:

```
nros: core pin observed sched_task=`high` runn[ctrl] tick=97
```

with `...running_on=1` arriving on a later line. The test matches line by
line, so it misses the marker. The marker goes through `printk`
(`CONFIG_PRINTK_SYNC=y` serialises `printk` calls only); the application's
`[ctrl] tick` lines go through picolibc `printf` to the console hook, which
shares no lock with `printk`. On two cores the two paths interleave character
by character. Placement works; the report of it is garbled.

### What this rules out

- QEMU 6.2 versus the pinned 11.0.0 (measured above).
- A lost timer interrupt (gdb, above).
- `cpus=1`: this image is built with `CONFIG_MP_MAX_NUM_CPUS=2` and refuses to
  boot with one vCPU (`Failed to boot secondary CPU core 1 (MPID:0x1)`), so that
  experiment needs a separate one-core build and was not run.

### Not measured: other boards the harness boots on slirp

The early-frame loss belongs to the harness's choice of network, not to a
board, so any image the harness boots on QEMU's user network can meet it. These
boots use slirp (`nros_tests::qemu`):

| board | harness boot |
| --- | --- |
| `mps2-an385` (bare metal and FreeRTOS) | `start_mps2_an385_networked`, `start_mps2_an385_on` (the image's `-nic`, from `image_nic`) |
| `mps2-an385` (Zephyr) | `start_zephyr_networked`, `start_zephyr_mps2_an385_networked_*` |
| `nuttx-qemu` (armv7a virt and riscv virt) | `start_nuttx_virt`, `start_nuttx_riscv` with networking |
| `mps3-an536` (FreeRTOS) | `start_mps3_an536_slirp` (its multicast boot uses a socket backend, not slirp) |

`threadx-qemu-riscv64` boots on socket/dgram backends, not slirp.

As of this writing, no image on these boards states
`[knobs.zenoh.limits] connect_timeout_ms` or `connect_forever`; the only
image in the tree that states a connect budget is the issue-2207
`connect_forever` threadx-linux listener row. Loss was not measured on any of
these boards. If one of their cells flakes with
`zpico Session -> ConnectionFailed` at boot followed by silence, this issue is
the first place to look.

## Fix

One change per class, each its own pull request:

- **Class B**: `CONFIG_NROS_CONSOLE_WHOLE_LINES` (nros-board-zephyr, on by
  default for SMP images). A stdout hook collects each thread's C stdout into a
  thread-local line and sends every complete line with one `printk`, so stdout
  joins the path `printk` already uses and no second console path is added.
- **Class A**: the `realtime-c` `smp_bringup` image states
  `[image.zephyr_smp.knobs.zenoh.limits] connect_timeout_ms = 10000`. The
  slirp attachment is the harness's (the board descriptor says Zephyr's own run
  uses a TAP), so the budget belongs to the one image the harness boots there,
  not to the board. It reaches the build through the image's cargo `[env]`
  file: `pub const CONNECT_TIMEOUT_MS: i32 = 10000;` in both `nros-rmw-zenoh`
  OUT_DIRs.

Measured with both changes on one fixture, under `taskset -c 0-3`: 49 of 50
runs ACCEPT (20 plain, 30 under the gdb wrapper), against roughly half before.
No run split the marker and none failed with `ConnectionFailed`.

The one failure is a third shape: no output after boot, and the full 90 s cell
timeout used without the 10 s budget ever ending in `ConnectionFailed`. That is
[issue 2389](../2389-zephyr-a53-open-silent-past-connect-budget.md).

## Acceptance

- `CorePinPlacement/zephyr/c` fails neither way described above, with both
  fixes landed: no split marker, and no boot-time `ConnectionFailed` while the
  budget lasts. Measured 49 of 50 under load; the remaining failure is issue
  2389's.
