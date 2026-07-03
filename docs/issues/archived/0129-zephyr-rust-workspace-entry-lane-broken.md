---
id: 129
title: "Zephyr rust workspace-entry lane broken on current main: executor heap alloc panics the default malloc arena; with that fixed, Executor::open fails Transport(ConnectionFailed) — stale prebuilt ELF masked both"
status: resolved
resolved_in: "2026-07-03 (three-layer fix: arena bump + C5b register emit + zephyr per-node-liveliness gate; culprit 6601c7e52 bisected)"
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

## Update (2026-07-03, second pass) — C-lane segfault root-caused + FIXED; layer 3 is all-language

**The C zephyr entry (fresh build) SEGFAULTED — a phase-273 FFI ABI drift, now fixed.**
Phase 273 appended `callback_group: *const c_char` (11th arg) to the Rust
`nros_cpp_subscription_register` and updated the C++ header (`subscription.hpp`), but
**missed the pure-C prototype in `nros-c/include/nros/component.h`**. Every C listener
(11 call sites across freertos/nuttx/threadx/zephyr/native workspaces + templates)
compiled the 9/10-arg shape, leaving the 11th slot as stack garbage that the Rust side
dereferenced — gdb: SIGSEGV in `cstr_to_str` ← `nros_cpp_subscription_register` ←
`listener_configure` (component.h macro path) on Zephyr native_sim; silent luck on
platforms where the garbage happened to be NULL. **Fixed:** prototype + doc example +
all 11 C call sites pass `/*callback_group=*/NULL`. Fresh C entry no longer crashes.

**Layer 3 (the silent hang) affects BOTH languages on the current tree.** With the segv
fixed, the fresh C entry exhibits the same symptom as the Rust one: boots, then hangs
before any readiness/publish output (the Rust case gdb'd to the app thread pending
forever in `k_mutex_lock(<fdtable+288>)` vs the zenoh-pico read task in `k_poll`; the C
case pends identically in the scheduler swap). The June-13 images run fine TODAY (the
single-node Rust talker connects + publishes in the same harness), and the zenoh-pico
submodule + zephyr module are unchanged since — so the regression is in the
**nros-node/nros-cpp executor stack churn between June 13 and now (phases 269–274,
prime suspect phase-271's executor externalisation)**, manifesting as an fd-lock
stall on zephyr native_sim in every entry that links current nros-cpp. Next: bisect
f418e3d08/77c745c7e (271) with the C entry as the repro, or instrument the
executor's session/spin fd usage vs the read task.

## RESOLVED (2026-07-03) — layer 3 bisected + fixed; lane green

`git bisect run` (anchor b6836a91c GOOD .. 4fc3d5a22 BAD, 121 commits, automated
west-build+boot probe) converged on **6601c7e52 "fix(268-W2b): thread per-entity node
identity into CFFI session view (#105)"** — NOT phase-271. That commit made every
entity-create carry its node identity, which triggers the Phase-268-W2 lazy
`ensure_node_liveliness` → a zenoh liveliness-token declare from the app thread in the
entity-create window, with the zenoh-pico read task live. On Zephyr native_sim that
declare wedges the app thread in the kernel's per-fd lock (the gdb'd
`k_mutex_lock(<fdtable>)` vs read-task `k_poll` hang); a stub experiment (declare
disabled → entry connects + publishes) confirmed the mechanism exactly.

**Fix:** per-node NN liveliness tokens are gated OFF on the Zephyr platform
(`#[cfg(feature = "platform-zephyr")]` early-return in `ensure_node_liveliness`,
nros-rmw-zenoh shim). The #104 PRIMARY token (session open, pre-contention) is kept, so
`ros2 node list` still shows the session node on Zephyr; only per-component names are
lost there until the create-window race in the zenoh-pico Zephyr port is fixed (a
follow-up if per-component visibility on Zephyr is wanted). All other platforms keep
full per-node liveliness.

**Verified green:** the C workspace entry connects + publishes; the Rust workspace
entries build + run; `params_zephyr_entry_e2e` (phase-276 W1 params-on-Zephyr) PASSES
(un-ignored) — the launch-baked param initial round-trips from the Zephyr entry to a
cross-process subscriber.
