---
id: 189
title: "qemu-arm-baremetal serial/XRCE lanes: session open dead AFTER the #184 heap fix — zenoh-serial hangs at Executor::open, XRCE fails ConnectionFailed fast"
status: resolved
resolved_in: "2026-07-14 — zenohd transport_serial reprovision + ZPICO_SERIAL swap (7c609a0ac); XRCE register() in setup_transport + serial/ custom-vtable dispatch (this commit)"
type: bug
area: baremetal
related: [issue-0184, issue-0178, phase-282]
---

## Summary

With #184's heap fix in (the three images now boot past the 74888-byte
executor-backing allocation), the two lanes fail one layer deeper —
serialized, fresh images 2026-07-13:

- `emulator::test_qemu_serial_pubsub_e2e` (zenoh-pico serial, socat pty ↔
  zenohd serial plugin): BOTH talker and listener print through
  `Serial ready.` then hang silently — no `Executor::open failed`, no
  publish, no panic; 97 s to test timeout. The session open never completes
  and never errors.
- `emulator::test_qemu_xrce_pubsub_e2e` (MicroXRCEAgent on a socat pty):
  boots, then `Executor::open failed: Transport(ConnectionFailed)` within
  ~2 s of boot — the uxr session create against the agent fails fast.

## Suspects (untriaged)

1. **#178 layers 2–3 family** — the zenoh-pico connect busy-wait needs a
   `wfi`-yield for QEMU (`-icount shift=auto`) to let host-timed I/O
   deliver; #178 proved the ethernet direct-exec path yields "some other
   way" while RTIC doesn't. The SERIAL direct-exec poll loop may lack
   whatever yield the ethernet path has.
2. **phase-282 tx rework** (zenoh-pico fork: batching + flush thread +
   split tx locking, `798328d78`/`25c3a6d3c`) — if the serial link's
   handshake writes now sit in a batch that only a flush *task* drains,
   a single-threaded bare-metal image may never emit InitSyn. The
   ethernet baremetal lanes' current state should discriminate (same
   threading model, different link).
3. The XRCE fast-fail is likely a different mechanism than the zenoh hang
   (it errors instead of hanging) — possibly agent-side pacing vs the
   1 s startup delay, or the serial framing on the pty.

## Progress — zenoh-serial half RESOLVED (2026-07-14)

Two stacked defects, both fixed and verified (serial pubsub e2e green 4/4,
~9 s; ethernet lanes 8/8 unaffected):

1. **The provisioned zenohd has no serial transport.** The phase-187 SDK
   migration dropped `--features zenoh/transport_serial` (the legacy
   `scripts/zenohd/build.sh` always had it): a `1.7.2-nros1` router refuses
   `--listen serial/...` ("Unicast not supported for serial protocol") and
   exits; the harness's 2 s blind sleep returned the corpse, so every guest
   hung at its serial handshake. Fixed: `[tool.zenohd]` → `1.7.2-nros2`
   source-built with the feature (dist rows return when the sdk repo seeds
   nros2 assets), `ci/nano-ros-sdk/scripts/build-zenohd.sh` carries the
   flag, `just zenohd setup` is provenance-version-aware (a bare `-x` check
   pinned the pre-serial binary forever), and `ZenohRouter::start_serial`
   fails loud when the router dies at startup.
2. **Serial-only firmware compiled the smoltcp spin branch.** The Phase
   136.4 manifest migration hardcoded `ZPICO_SMOLTCP` in
   `[platform.bare-metal] defines` — its own comment deferred the Phase-132
   `ZPICO_NO_SMOLTCP` opt-out "if such a board materialises" (the serial
   boards already existed). On a serial-only image the smoltcp branch's
   clock (`smoltcp_clock_now_ms`) is frozen, so `zpico_spin_once(10)` only
   returned on router keepalives (~2.5 s), and the no_std executor credits
   just the REQUESTED 10 ms per spin — the 1 Hz timer needed ~250 s wall to
   come due. Fixed in the runner (Step 6): serial-only link set +
   `ZPICO_NO_SMOLTCP` swaps the define for `ZPICO_SERIAL` (probe-verified:
   session opens, spins honor the 10 ms budget, publishes flow).

## XRCE half RESOLVED (2026-07-14)

`test_qemu_xrce_pubsub_e2e` failed (`Executor::open failed:
Transport(ConnectionFailed)` ~2 s after boot) with **zero bytes** on the
pty (socat -x) while the vtable install in `setup_transport` provably
succeeded and the identical UART/pty wiring carried the zenoh serial
lane. Root cause: **no RMW backend was ever registered on this image** —
a #163/#131-class registration gap, not a transport defect:

- `nros::main!()`'s `__register_linked_rmw()` is a Phase-249 no-op (the
  board owns registration since Phase 248 C5a).
- The board's explicit registration in `entry.rs::boot()` covers only
  zenoh, gated `#[cfg(feature = "rmw-zenoh")]` — the XRCE image builds
  the board `default-features = false, features = ["board-entry",
  "xrce-transport"]`, so it compiles out.
- nros-rmw-xrce-cffi's linkme auto-register never fires on bare-metal
  (no `.init_array`).

So `Executor::open` failed inside the CFFI resolve before a single byte
could reach the UART. (The pre-BoardEntry examples called
`xrce::register()` by hand — exactly what `xrce_transport.rs`'s doc
comment still shows; the phase-244.D1 BoardEntry migration lost that
call.) Fixed: `Mps2An385::setup_transport` now calls
`nros_rmw_xrce_cffi::register()` (fail-loud) right after the vtable
install.

Also landed while triaging: `xrce_session_open`'s locator dispatch now
implements the documented `serial/...` → custom-vtable contract
(nros-rmw-xrce-cffi lib.rs doc) on non-POSIX builds when a vtable is
armed — previously such a locator silently fell through to the bare
host:port UDP path (no UDP provider on a serial-only image →
ConnectionFailed, zero wire bytes). This lane's baked locator is
`custom://uart` so it never hit that branch, but any board configured
with a zenoh-style `serial/<dev>#...` locator + XRCE transport would
have. POSIX `serial://` routing unchanged.

Verified: manual harness (socat pair + MicroXRCEAgent serial -v6) shows
the full session handshake + steady publishing on the wire;
`test_qemu_xrce_pubsub_e2e` passes (~3 s). Regressions:
`test_qemu_serial_pubsub_e2e` green, ethernet emulator set 8/8, POSIX
XRCE lanes (c_xrce_api + large_msg + bridge_mixed_rmw, 10/10) green on
freshly rebuilt fixtures.

## History caveat

These lanes were part of the museum-binary population (#182 class): the
last *proven* pass predates the phase-271 executor rework (their 24 KB
heap could not even boot a post-271 image, and the published phase-204/207
footprint figures were measured on pre-271 images). Treat "green history"
as unproven, per the #178 note.
