---
id: 2977
title: "Two Zephyr Cortex-M cells booted their Zephyr image through the bare-metal mps2 descriptor, and nothing could notice"
status: resolved
resolved: 2026-11-26
resolved_in: "the change that adds this issue"
type: bug
severity: low
area: [testing, qemu, zephyr]
related: [issue-2971, issue-2379, phase-544]
found: 2026-11-26
---

## What

mps2/an385 has two board descriptors on one QEMU machine:
`nros-board-mps2-an385` (`mps2-an385-baremetal`, which FreeRTOS images share)
and `nros-board-mps2-an385-zephyr` (issue 2379). A Zephyr image is meant to boot
through the second, via `start_zephyr_networked(MPS2_AN385_ZEPHYR, ..)` or
`start_zephyr_mps2_an385_networked_measured`. Those calls hold the descriptor to
the image's own west `run_qemu` line.

Two cells booted theirs through `start_mps2_an385_networked`, the bare-metal launcher:

- `violation_channel_e2e.rs`, the Zephyr Cortex-M violation-channel cell. Found
  while resolving issue 2971.
- `pubsub_zephyr_cortex_m_ros2_interop_e2e.rs`, the Zephyr C talker against ROS 2.

Both boot and run, because the machine and NIC are the same. So nothing failed. The
image got the wrong board's line: semihosting enabled, and the bare-metal board's
`-icount`. It also skipped the check against west's own run line.

## Sweep

Every QEMU boot in `nros-tests`, and the image each one resolves:

    git grep -nE 'start_(mps2_an385|zephyr)[a-z_]*\(|Boot::FreertosMps2|Boot::FreertosQemu' \
        -- packages/testing/nros-tests/tests

The two above are the only Zephyr images on a bare-metal launcher.

- Every other `start_mps2_an385*` site boots a FreeRTOS or bare-metal image. That
  includes the generic dispatchers: `rtos_e2e` `Platform::Freertos`, `entry_e2e`
  `Boot::FreertosMps2`, `realtime_sched_tasks_e2e` `Proof::SerialDispatch`,
  `sched_dims_applied_e2e` `Boot::FreertosQemu`, and `param_service_heap`.
- No non-Zephyr image boots on the Zephyr descriptor (`zephyr_cortex_m_qemu`,
  `port_templates_e2e`).

## Fix: the mismatch is refused, not just corrected

A boot read from a descriptor now carries `zephyr_board` (its `[[board]]
platform` is `zephyr`). `QemuProcess::start` checks the image against it. An
image whose symbol table names `z_cstart`, the Zephyr kernel's C entry, is a
Zephyr image. A Zephyr image on a non-Zephyr board, or the reverse, fails at launch
with a message naming the right launcher. The check covers every launcher at
once, generic dispatchers included, so the next mismatch cannot be written.

Measured: before the two sites moved, the violation cell's real Zephyr ELF was refused,
with "is a Zephyr image but was booted on a non-Zephyr board descriptor (issue 2977);
use `QemuProcess::start_zephyr_networked(MPS2_AN385_ZEPHYR, ..)` or
`start_zephyr_mps2_an385_networked_measured`". Fixture-free tests pin the
detection, both directions of the refusal, and the kind each mps2 descriptor
states.

The violation cell moved to `start_zephyr_mps2_an385_networked_measured` in issue
2971's change, for the fixed icount its guest-time bound needs. The interop cell
moved to `start_zephyr_networked(MPS2_AN385_ZEPHYR, ..)`.
