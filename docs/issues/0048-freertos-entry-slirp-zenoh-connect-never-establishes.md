---
id: 48
title: FreeRTOS Entry firmware never connects over zenoh — deploy config is inert (uses Config::default) AND zenoh-pico open() hangs pre-connect
status: open
type: bug
area: freertos
related: [issue-0046, phase-212]
---

## Summary

The FreeRTOS Entry/run-plan firmware boots cleanly (since #46) to `Network ready.`
but its `Executor::open` never connects — it hangs, printing none of the run_plan
markers (`Application setup complete`, `Published:`, `Executor::open failed`).
`freertos_run_plan_runtime.rs` and `orchestration_tiers_freertos.rs` label this
**"timing-flaky"** and treat the connected run as best-effort. **That label is
wrong — the connection never establishes; it is not flaky, it is non-functional.**

Two **independent** root causes, both confirmed with on-device DBG + packet
capture (gratuitous-ARP-only). The earlier "subnet mismatch / guest→host gap"
framing was a symptom of cause 1.

## 1. Deploy-metadata config is inert — the firmware uses `Config::default()`

The `talker_entry` deploy overlay sets `locator = "tcp/10.0.2.2:7451"`
(Cargo.toml `[package.metadata.nros.deploy.freertos]`), but **that string is not
compiled into the firmware at all** — `strings` on the ELF shows only
`tcp/192.0.3.1:7447`. An on-device print confirms the running locator:

```
DBG48: app task at Executor::open locator=tcp/192.0.3.1:7447   ← board default, NOT the deploy 10.0.2.2:7451
```

Cause: `<Mps2An385 as BoardEntry>::run` hardcodes `Config::default()`
(`nros-board-mps2-an385-freertos/src/lib.rs:264`):

```rust
fn run<F, E>(setup: F) -> Result<(), E> { ...
    nros_board_freertos::run_entry::<Mps2An385, F, E>(Config::default(), setup)
}
```

`BoardEntry::run` takes only `setup` (no `Config`), so the macro can't thread a
deploy-derived config through it. Nothing reads the deploy block at build time:
the board-emitted C `NROS_APP_CONFIG` (build.rs, also the `192.0.3.1:7447`
default) has **no consumers** (a dead Phase-212.M-F.10.3 "Path C" stub), and
`nros::main!()` doesn't generate a config const from the deploy metadata. So
`locator` / `ip` / `gateway` / `domain_id` from the deploy block are all dropped —
the firmware is on `192.0.3.10/24` with locator `192.0.3.1:7447`, which is
unreachable over slirp (`10.0.2.0/24`). (The board default itself can't move to
`10.0.2.x`: the mcast cross-instance pubsub path needs `Config::default()` =
`192.0.3.10` / `Config::listener()` = `192.0.3.11` on a shared L2.)

## 2. zenoh-pico `open()` hangs on FreeRTOS before the TCP connect

Forcing the config fully correct (`Config::default()` → ip `10.0.2.15`, gw
`10.0.2.2`, locator `tcp/10.0.2.2:7447`, verified via DBG) and running a host
zenohd on `7447`, `Executor::open` **still** hangs — and a packet capture
(`-nic user,model=lan9118,id=mynet -object filter-dump,netdev=mynet`) shows the
guest emits **only one gratuitous ARP for its own IP**, then nothing:

```
ARP, Request who-has 10.0.2.15 tell 10.0.2.15     ← lwIP init only
(no ARP for the gateway 10.0.2.2, no SYN to 7447)
```

So the hang is **inside `z_open` / the zenoh-pico platform shim, before any TCP
connect** — not a slirp / guest→host networking problem (the firmware never tries
to reach the gateway). The session is **Client** mode (`SessionMode::Client`
default — connects to the locator, no multicast scout), so it is not a blocking
scout. Where it blocks (mutex/semaphore creation on the FreeRTOS heap, a task
handshake, or the lwIP socket setup) is still TBD — it needs `z_open`-level
instrumentation in `nros-rmw-zenoh`/zenoh-pico on the FreeRTOS target.

## Fix path

1. **Thread the deploy config into the firmware.** Either give `BoardEntry::run`
   a `Config` (signature change + macro passes a deploy-derived const), or have
   `nros::main!()` codegen emit an `NROS_APP_CONFIG`/`Config` from the deploy
   block and make the board read it instead of `Config::default()`. The
   `[transport]` config parser already accepts `ip`/`gateway`/`locator`; the
   codegen path just never populates them for the freertos firmware.
2. **Fix the zenoh-pico FreeRTOS `open()` hang.** Instrument `z_open` to find
   where it blocks before the connect; likely a platform-shim
   (mutex/semaphore/task) issue on FreeRTOS.
3. Then the `freertos_run_plan_runtime` / `orchestration_tiers_freertos`
   connected runs can assert (not best-effort) `Application setup complete` /
   `Published:`.

## Not blocked by this

#46 (the Executor stack overflow) is fixed: the firmware boots through the full
board lifecycle. The runtime gate `freertos_board_run_executes_run_plan` is green
on the boot lifecycle (the deterministic part). Only the *connected* run is gated.
