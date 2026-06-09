---
id: 17
title: Zephyr workspace Entry — native_sim zenoh E2E delivers (RESOLVED)
status: resolved
type: bug
area: zephyr
related: [issue-0018]
---

**Status (2026-06-09, RESOLVED): the Phase 225.P Zephyr workspace Entry
now publishes `/chatter` over zenoh on native_sim and an external native
listener receives it cross-process** (`Received: 0,1,2,…`).
`test_zephyr_workspace_entry_native_sim_e2e` passes (`1 passed`, 9
messages delivered in a 41 s window). The chain — `just zephyr
build-fixtures` (`west build`) → boot `zephyr.exe` → `nros_net_wait`
network gate → register the launch node set → register the zenoh backend
→ `Executor::open` → publish → cross-process delivery to the external
listener — works end to end.

**The earlier "environmental NSOS offload is broken" diagnosis was WRONG
— same misdiagnosis class as issue #18 (NuttX).** The evidence that read
as "NSOS never issues a `connect()`" was actually an EMPTY locator: the
Rust path used `ExecutorConfig::default_const()` (empty locator) → no TCP
target → zenoh-pico fell back to multicast scouting (which native_sim
can't satisfy), so there was nothing to `connect()` *to*. NSOS host-socket
offload is fully functional: with the locator fixed, `strace` shows
`connect(127.0.0.1:7456)=EINPROGRESS` followed by `sendto(...)` carrying
the `0/chatter/std_msgs::msg::dds_::Int32_` declarations + data samples,
and `zenohd --debug` logs the accepted transport, the subscriber/token
declarations, and routes data to the external listener.

The fix was a two-part cascade in the never-before-exercised Rust
Zephyr-zenoh native_sim path (commit `fix(zephyr): wire RMW backend +
baked locator …`):

1. **No RMW backend linked.** On `target_os = "none"` (native_sim)
   `linkme` is a no-op and the image does not run the `.init_array`
   auto-register fallback, so the CFFI vtable had no transport and
   `Executor::open` returned `Transport(ConnectionFailed)`. The
   `nros::main!` Zephyr branch now calls `nros::__register_linked_rmw()`
   (a feature-dispatched, idempotent facade) before `Executor::open`;
   `zephyr_component_main!` (single-node) does the same.

2. **Empty locator.** `default_const()` → multicast scouting. The branch
   now bakes the locator via `option_env!("NROS_LOCATOR")`, and the Entry
   `build.rs` re-exports `CONFIG_NROS_ZENOH_LOCATOR` (the Kconfig the C API
   path already consumes) into that env — Kconfig is now the single source
   of truth for both languages.

**native_sim timing note:** on a slow native_sim host the Entry's
zenoh-pico session setup + first publish lands ~20 s after boot, then the
publish cadence tracks the ~2.5 s lease keepalive. The E2E listener wait
is 40 s to accommodate this (it always runs the full duration — the
listener `spin_blocking`s and never self-exits, so the bound caps
wall-time, not the success path). CI is faster; the bound is generous, not
tight.

**Follow-up (single-node reference `test_zephyr_to_native_e2e`):** the
shared macros are fixed, but the single-node zephyr examples' `build.rs`
still calls the renamed-away `export_kconfig_bool_options` (now
`export_bool_kconfig`) and does not yet bake `CONFIG_NROS_ZENOH_LOCATOR`
into `NROS_LOCATOR`. So the single-node reference E2E needs the same
per-example `build.rs` locator-baking pass the Entry got before it will
deliver. Tracked as a remaining cleanup under this issue.

**Cross-reference**: the sibling issue #18 (NuttX) is also RESOLVED via
the same locator + backend-register cascade (its entry boots on
`qemu-system-arm` rather than native_sim, but the root cause and fix shape
are identical).
