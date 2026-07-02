---
id: 129
title: "Zephyr rust workspace-entry lane broken on current main: executor heap alloc panics the default malloc arena; with that fixed, Executor::open fails Transport(ConnectionFailed) — stale prebuilt ELF masked both"
status: open
type: bug
area: zephyr
related: [phase-271, phase-248, phase-276, phase-263]
---

## Summary

Rebuilding the Zephyr (native_sim) **Rust** workspace Entry (`examples/workspaces/rust/src/
zephyr_entry`, west lane `build-ws-rs-entry-zenoh`) from current `main` produces a binary that
**cannot run**, in two layers. The lane *looked* healthy because the build root held a **stale
`zephyr.exe`** predating the regressions (the staleness probe correctly flags it — running
`tests/zephyr.rs::test_zephyr_workspace_entry_native_sim_e2e` fails "binary is stale" — but
nothing had rebuilt + rerun it since). Found while adding the phase-276 W1 params-on-Zephyr
fixture (whose fresh build hit both immediately, 2026-07-03).

## Layer 1 — `Executor::open` heap-allocates ~75 KiB; picolibc's default 16 KiB arena panics

Fresh build of the UNMODIFIED base entry, booted against a live zenohd:

```
panic: memory allocation of 75080 bytes failed
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

The zephyr-lang-rust global allocator wraps **libc malloc** — picolibc's arena, sized by
`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` (default 16384) — NOT `CONFIG_HEAP_MEM_POOL_SIZE`.
`Executor::open` on current main heap-allocates a ~75 KiB backing (phase-271 externalised
executor storage — the pre-271 shape evidently didn't push this through malloc). Any Zephyr
Rust entry is affected. **Mitigation applied** (2026-07-03): the base and ws-params entries'
`prj-zenoh.conf` now set `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=1048576` (native_sim runs on
host RAM). Real fix candidates: size the arena from the executor sizing, or give Zephyr a
non-malloc backing (static / kernel heap), per phase-271's per-entry sizing design.

## Layer 2 — with the arena fixed, `Executor::open` fails `Transport(ConnectionFailed)`

```
<err> rust: rustapp: nros: zephyr entry — executor open failed: Transport(ConnectionFailed)
```

Verified NOT environmental:
- zenohd live on the baked locator (`tcp/127.0.0.1:7456`), reachable;
- `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y` (NSOS) in the final `.config`;
- the locator IS baked (build-script output: `cargo:rustc-env=NROS_LOCATOR=tcp/127.0.0.1:7456`);
- fails at t=+2.001 s (immediately after the net-wait), zenohd sees zero connections;
- an `strace -e socket,connect` shows no host socket traffic from the image (inconclusive
  given native_sim's dual-world, but consistent with the session never dialing).

Signature matches the known "no RMW backend registered → `resolve_backend` finds no transport →
`Executor::open` = `Transport(ConnectionFailed)`" failure (the FreeRTOS boards hit this until
Phase 248 C5a added the explicit board-owned `nros_rmw_zenoh::register()`; linkme/.init_array
ctors don't run on `target_os = "none"` images). Suspect: the Rust-Zephyr register path broke in
the phase-248→271 churn (the `Framework::Zephyr` macro arm carries no register call and relies on
the board/C side). Needs: trace `resolve_backend` on the zephyr image; if the vtable is empty,
wire the zephyr equivalent of the FreeRTOS explicit register.

## Impact

- `tests/zephyr.rs::test_zephyr_workspace_entry_native_sim_e2e` — fails (staleness today; both
  layers after any rebuild).
- phase-276 W1 params-on-Zephyr (`ws-params-rust/src/zephyr_entry` + `params_zephyr_entry_e2e`,
  built on the #128 macro fix) — fixture builds + boots + param seed baked, but blocked at the
  same `Executor::open`; its e2e is `#[ignore]`d referencing this issue.
- The C / C++ zephyr workspace entries (different lane — C API + `ZephyrBoard::run_components`)
  are not implicated by this evidence; verify separately.

## Repro

```
NROS_ZEPHYR_FIXTURE_FILTER="workspace-entry" just zephyr build-fixtures   # fresh base entry
build/zenohd/zenohd --listen tcp/127.0.0.1:7456 --no-multicast-scouting &
../nano-ros-workspace/build-ws-rs-entry-zenoh/zephyr/zephyr.exe           # layer 1 panic
# apply the arena bump → rebuild → rerun → layer 2 ConnectionFailed
```

## Update (2026-07-03) — layer 2 root-caused + FIXED; layer 3 isolated

**Layer 2 root cause (registration) — confirmed + fixed.** The June-13 prebuilt images
predate phase-248 C6g: back then the entries carried `rmw-zenoh = ["nros/rmw-zenoh",
"dep:nros-rmw-zenoh"]` and the macro emitted `__register_linked_rmw()`. C6g made the
feature an inert marker and C5c/249-P1 removed the emit, on the premise the backend
"enters the link graph via the board crate / CMake C-port" — which only ever held for
C/C++ entries. On current main the Rust-Zephyr graph had NO backend crate and NO
registration (`cargo tree -i nros-rmw-zenoh` → not in graph; zero zenoh symbols in the
image). **Fix (the RFC-0031 C5b amendment, finally implemented):**
- the Zephyr entries' `rmw-zenoh = ["dep:nros-rmw-zenoh"]` is a real dep again
  (`platform-zephyr` + `ros-humble`; `zpico-sys/zephyr` consumes the west build's
  INCLUDE_DIRS/INCLUDE_DEFINES env);
- `nros::main!`'s Zephyr arm emits `::nros_rmw_<x>::register()` from the deploy
  metadata's `rmw` key (cfg-gated on the entry's matching `rmw-<x>` feature), mirroring
  the bridge-entry emit and the FreeRTOS C5a board-owned register.
Verified by strace: the image now performs the full TCP connect + zenoh open handshake +
`@ros2_lv` node-liveliness declare against zenohd (previously zero network syscalls).

**Layer 3 (NEW, isolated) — fdtable mutex deadlock after the first liveliness declare.**
With registration fixed, the entry connects and declares the first node, then hangs:
gdb shows the app thread blocked forever in `k_mutex_lock(<fdtable+288>)` (a per-fd lock
in Zephyr's `fdtable.c`), while the zenoh-pico read task sits in `k_poll` and the lease
task in `k_sleep` — the main thread and the read task share the single session socket fd
and deadlock on its fdtable entry lock. Keepalives flowed before the hang (both LWPs
interleaved sends/recvs), so the lock-ordering breaks at/after the first declare batch.
Next: reproduce with the fdtable lock instrumented; suspects are the NSOS/zvfs per-fd
locking vs zenoh-pico's MT model (`Z_FEATURE_MULTI_THREAD=1`, one socket shared by main +
read task) — same family as the repo's earlier NSOS patches. The C/C++ Zephyr entries
(same socket model through the C API) may or may not share this; verify.
