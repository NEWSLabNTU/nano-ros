---
id: 48
title: FreeRTOS Entry firmware never connects over zenoh — RMW backend not linked (cause 2) + deploy config inert (cause 1)
status: resolved
type: bug
area: freertos
related: [issue-0046, phase-212]
---

## Resolution (both causes fixed)

The FreeRTOS `talker_entry` firmware now connects: it boots on the slirp guest IP
`10.0.2.15`, completes the TCP + zenoh-pico session handshake to the host zenohd
at `tcp/10.0.2.2:7451`, and prints `Application setup complete — entering spin
loop` (Executor::open succeeded → run-plan ran → the talker publishes `/chatter`).
The `freertos_board_run_executes_run_plan` runtime gate now **asserts** the
connected run (was best-effort). Both root causes below are closed.

## Summary

The FreeRTOS Entry/run-plan firmware booted cleanly (since #46) to `Network
ready.` but its `Executor::open` never connected — two **independent** root
causes, both now fixed:

- **Cause 2** — the zenoh RMW backend was never linked/registered → `NoBackend`.
- **Cause 1** — the deploy-metadata config (locator/ip/gateway) was inert, so the
  firmware used `Config::default()` (unreachable `192.0.3.1:7447`).

## 1. Deploy-metadata config is inert — the firmware uses `Config::default()` (FIXED)

**Fix:** `nros::main!()` now threads the deploy block into the firmware's boot
`Config` via a new `BoardEntry::run_with_deploy(&DeployOverlay, setup)` seam:

- `nros-platform`: added `DeployOverlay` (all-`Option` fields: locator / ip /
  gateway / netmask / domain_id) + a **provided** `BoardEntry::run_with_deploy`
  whose default body ignores the overlay and calls `run` — so POSIX / framework
  boards are untouched.
- `nros::main!()` (OwnedSpin branch): reads
  `[package.metadata.nros.deploy.<board>]`, bakes a `DeployOverlay` const, and
  calls `run_with_deploy` instead of `run`.
- `nros-board-mps2-an385-freertos`: overrides `run_with_deploy` to overlay the
  supplied fields onto `Config::default()` before `run_entry`.
- `talker_entry` deploy block gained `ip = "10.0.2.15"` / `gateway = "10.0.2.2"`
  (slirp addressing) alongside the existing `locator = "tcp/10.0.2.2:7451"`.

`strings` on the ELF now shows `tcp/10.0.2.2:7451` (the `192.0.3.1:7447` default
is gone) and the firmware boots on `10.0.2.15`. Original analysis follows.

---

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

The connect originally did not *complete* — but only because of cause 1 (the baked
`192.0.3.1:7447` locator was unreachable over the slirp `10.0.2.x` net). With
cause 1 fixed too, the firmware now dials `tcp/10.0.2.2:7451` and connects.

## Fix path (both done)

1. **(DONE) Thread the deploy config into the firmware** — `BoardEntry::run_with_deploy`
   + `DeployOverlay`, `nros::main!()` bakes `[deploy.<board>]`, the freertos board
   overlays it onto `Config::default()`. See cause 1 above.
2. **(DONE) Link + register the zenoh RMW backend** — see cause 2 above:
   `nros-board-freertos rmw-zenoh → nros/rmw-zenoh`, umbrella
   `platform-freertos → nros-rmw-zenoh?/platform-freertos`, and the
   `nros::main!()` OwnedSpin branch calls `__register_linked_rmw()` on
   `target_os = "none"`.
3. **(DONE)** `freertos_run_plan_runtime` now asserts (not best-effort) the
   connected run reaches `Application setup complete`. The fixture is built in
   **release** (debug zenoh-pico on the emulated M3 is too slow to finish the
   session handshake within the test budget). `orchestration_tiers_freertos`
   still labels its connected run best-effort — a follow-up can tighten it the
   same way.

## Not blocked by this

#46 (the Executor stack overflow) is fixed: the firmware boots through the full
board lifecycle.
