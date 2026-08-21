---
id: 758
title: "No platform wall-clock epoch source — embedded consumers hand-roll
  SNTP before boot, and stamped messages are wrong until they do"
status: open
type: enhancement
area: core, boards
related: [rfc-0052]
---

## Problem

An embedded image has no wall-clock epoch at boot. Messages it stamps
(`now()` from a monotonic source) carry boot-relative time, and any peer
that validates stamps rejects them. The concrete consumer case
(autoware-safety-island, phase-3 driving demo): the island stamped
control commands from its boot epoch and Autoware's `vehicle_cmd_gate` /
monitors rejected them as stale — autonomous mode could never actuate
until ASI added its own SNTP step:

- a `platform_init_clock_via_sntp()` platform hook (Zephyr SNTP lib
  underneath, `CONFIG_SNTP_SERVER_ADDRESS` Kconfig),
- an unprivileged host-side `scripts/sntp-server.py` for tap/FVP setups,
- a call wired into its board network hook before `nros::init`.

That is ROS-infra living consumer-side. Every embedded nano-ros consumer
that talks to a stamped-message peer will need the same thing, and
timer/age machinery (RFC-0052 age monitors, `ExecutorConfig`-style epoch
knobs) wants the same epoch fact internally.

## Direction

A platform epoch source in nano-ros:

- Platform ABI: an optional `epoch_us()` / `set_epoch_us()` pair (or a
  one-shot `acquire_epoch()` hook) in the platform vtable — optional
  slot, `Option<fn>` like the rest (RFC-0054 rules; bindgen regen).
- A small SNTP client as the first provider (Zephyr has one in-tree;
  lwIP ships SNTP for the FreeRTOS family; POSIX boards just read
  `CLOCK_REALTIME`).
- Config: server address/port as a board/deploy fact (system.toml rung),
  not a consumer #define.
- Boot ordering: acquired after netif-up, before components construct —
  the board bring-up already has exactly that seam (network hook).

ASI then deletes its `platform_init_clock_via_sntp` copy and the
board-hook call; its `sntp-server.py` host helper can move to nano-ros
tooling or stay consumer-side.

## Non-goals

Full time sync (PTP, continuous discipline). One epoch acquisition at
boot is what stamped-message interop needs; drift handling can layer
later.
