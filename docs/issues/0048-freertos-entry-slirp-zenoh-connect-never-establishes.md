---
id: 48
title: FreeRTOS Entry firmware never connects to the host zenohd over slirp — subnet mismatch + guest→host gap (the "timing-flaky" connected-run is a misnomer)
status: open
type: bug
area: freertos
related: [issue-0046, phase-212]
---

## Summary

The FreeRTOS Entry/run-plan firmware boots cleanly (since #46) to `Network ready.`
but its `Executor::open` never connects to the host zenohd over QEMU slirp — it
hangs, printing none of the run_plan markers (`Application setup complete`,
`Published:`, `Executor::open failed`). `freertos_run_plan_runtime.rs` and
`orchestration_tiers_freertos.rs` both label this **"timing-flaky"** and treat the
connected run as best-effort. **That label is wrong — the connection never
establishes; it is not flaky, it is non-functional.**

## Root cause (two compounding problems)

### 1. Subnet mismatch (confirmed, primary)

The slirp QEMU launcher `QemuProcess::start_mps2_an385_networked` documents its
contract (qemu.rs:208-210):

> The firmware connects to the host via slirp gateway `10.0.2.2`. The firmware's
> config.toml must use the `10.0.2.0/24` subnet with gateway `10.0.2.2`.

But the firmware boots on the **board default `192.0.3.10/24`, gateway
`192.0.3.1`** (`nros-board-freertos/src/config.rs` + the C `NROS_APP_CONFIG` in
`nros-board-mps2-an385-freertos/build.rs`). The `talker_entry` / orchestration
deploy overlays override only the **locator** (→ `tcp/10.0.2.2:7451`), not the
IP/gateway — the deploy-metadata schema (`rmw` / `domain_id` / `locator`) has no
`ip`/`gateway` keys. So the firmware sits on `192.0.3.10` and cannot route to the
slirp gateway `10.0.2.2` (different subnet) → the connect goes nowhere.

The board default **cannot** simply move to `10.0.2.x`: the mcast cross-instance
pubsub path (`start_mps2_an385_mcast`) relies on `Config::default()` = `192.0.3.10`
and `Config::listener()` = `192.0.3.11` for two firmwares on a shared virtual L2.
slirp and mcast are two networking modes with different IP requirements; the IP
must be set **per-deployment**, which the metadata can't express today.

Verified: forcing the firmware to `10.0.2.15` / gw `10.0.2.2` makes it boot with
`IP: 10.0.2.15` (correct subnet) — necessary, but see #2.

### 2. Connected-run still fails on the correct subnet (observed)

Even with the firmware on `10.0.2.15` and a host zenohd (or a plain
`socket.accept()` listener) on `0.0.0.0:7451`, the host receives **no connection**
from the guest. So one of:
- QEMU slirp is not forwarding guest→`10.0.2.2:7451` to the host with the bare
  `-nic user,model=lan9118` config, or
- zenoh-pico's `open` on FreeRTOS hangs *before* emitting the TCP SYN.

Packet capture to disambiguate was inconclusive: `-netdev user,id=n0 -device
lan9118,netdev=n0 -object filter-dump` conflicts with the mps2-an385 *onboard*
LAN9118 (firmware gets no IP, 0 packets). A capture path that taps the onboard NIC
is needed.

## Fix path (multi-component)

1. Express a slirp-compliant per-fixture network (`ip`/`gateway`/`netmask`) — extend
   the deploy-metadata schema + the `[transport]` config overlay (the parser
   already accepts `("transport","ip")` / `("transport","gateway")`; the codegen
   path just doesn't populate them), or add a slirp board preset distinct from the
   mcast default.
2. Resolve #2: confirm via an onboard-NIC packet capture whether the guest emits a
   SYN; then either fix the QEMU netdev for guest→host forwarding, or fix the
   zenoh-pico FreeRTOS `open` connect.
3. Then the `freertos_run_plan_runtime` / `orchestration_tiers_freertos` connected
   runs can assert (not best-effort) `Application setup complete` / `Published:`.

## Not blocked by this

#46 (the Executor stack overflow) is fixed: the firmware boots through the full
board lifecycle. The runtime gate `freertos_board_run_executes_run_plan` is green
on the boot lifecycle (the deterministic part). Only the *connected* run is gated
here.
