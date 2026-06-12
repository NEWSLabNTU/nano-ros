---
id: 48
title: FreeRTOS Entry firmware never connects over zenoh — RMW backend not linked (cause 2, FIXED) AND deploy config is inert (cause 1, open)
status: open
type: bug
area: freertos
related: [issue-0046, phase-212]
---

## Summary

The FreeRTOS Entry/run-plan firmware boots cleanly (since #46) to `Network ready.`
but its `Executor::open` never connects.
`freertos_run_plan_runtime.rs` and `orchestration_tiers_freertos.rs` label this
**"timing-flaky"** and treat the connected run as best-effort. **That label is
wrong — the connection never establishes; it is not flaky, it is non-functional.**

Two **independent** root causes. **Cause 2 (RMW backend never linked/registered)
is FIXED and verified.** Cause 1 (deploy-metadata config is inert) is still open
and is the only thing now blocking a fully-green connected run.

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

## 2. The zenoh RMW backend is never linked → `Executor::open` returns `NoBackend` (FIXED)

**This was mis-framed as a "zenoh-pico `open()` hang". It is not a hang and not a
networking problem — `Executor::open` returned `Transport(ConnectionFailed)`
*before any* `z_open` / TCP connect.** On-device instrumentation pinned the exit
to `nros_rmw_cffi::resolve_backend(...)` returning `BackendResolution::NoBackend`
(zero RMW backends registered in the CFFI vtable). The earlier
"gratuitous-ARP-only" capture is exactly consistent: open bails before any
connect, so the only traffic is lwIP's init ARP.

Root cause: the FreeRTOS Entry firmware linked only the **generic vtable shim**
(`nros-rmw-cffi`) and the **zenoh-pico C transport** (`zpico-sys`) — *not* the
`nros-rmw-zenoh` Rust crate that bridges zenoh-pico into the CFFI registry and
exposes `register()`. With no backend crate linked, nothing registers a vtable.
Compounding it: on `target_os = "none"` `linkme` is a no-op and the image does
not run the `.init_array` auto-register fallback (section.rs Phase 142), so even
a linked backend would need an *explicit* `register()` call — the `nros::main!()`
OwnedSpin branch never made one (only the Zephyr branch did, via
`__register_linked_rmw()`).

**Fix (three coordinated changes):**

1. `nros-board-freertos`: `rmw-zenoh` feature now enables `nros/rmw-zenoh`, so
   the `nros-rmw-zenoh` backend crate is actually pulled into the link graph
   (`rmw-zenoh = ["nros/rmw-zenoh"]`).
2. `nros` umbrella: `platform-freertos` forwards `nros-rmw-zenoh?/platform-freertos`
   (inert via `?` when the backend isn't linked), parity with the NuttX row, so
   `zpico-sys` builds zenoh-pico with the FreeRTOS platform manifest under the
   unified-RMW link path.
3. `nros::main!()` OwnedSpin branch: calls `::nros::__register_linked_rmw()` on
   `target_os = "none"` before the board opens the executor (mirrors the Zephyr
   branch + `zephyr_component_main!`).

**Verified:** the firmware now reaches `z_open` and actively retries the TCP
connect — packet capture shows repeated `ARP who-has 192.0.3.1` (the locator
host), where before there was only the single init ARP:

```
ARP, Request who-has 192.0.3.10 tell 192.0.3.10   ← lwIP init
ARP, Request who-has 192.0.3.1  tell 192.0.3.10   ← z_open connect attempt (repeats)
```

The connect does not yet *complete* — but only because of cause 1 (the baked
`192.0.3.1:7447` locator is unreachable over the slirp `10.0.2.x` net). That is a
networking/config gap, not a backend gap.

## Fix path

1. **(OPEN) Thread the deploy config into the firmware.** Either give
   `BoardEntry::run` a `Config` (signature change + macro passes a deploy-derived
   const), or have `nros::main!()` codegen emit an `NROS_APP_CONFIG`/`Config` from
   the deploy block and make the board read it instead of `Config::default()`. The
   `[transport]` config parser already accepts `ip`/`gateway`/`locator`; the
   codegen path just never populates them for the freertos firmware.
2. **(DONE) Link + register the zenoh RMW backend** — see cause 2 above:
   `nros-board-freertos rmw-zenoh → nros/rmw-zenoh`, umbrella
   `platform-freertos → nros-rmw-zenoh?/platform-freertos`, and the
   `nros::main!()` OwnedSpin branch calls `__register_linked_rmw()` on
   `target_os = "none"`.
3. Once cause 1 lands, the `freertos_run_plan_runtime` /
   `orchestration_tiers_freertos` connected runs can assert (not best-effort)
   `Application setup complete` / `Published:`.

## Not blocked by this

#46 (the Executor stack overflow) is fixed: the firmware boots through the full
board lifecycle. The runtime gate `freertos_board_run_executes_run_plan` is green
on the boot lifecycle (the deterministic part). Only the *connected* run is gated.
