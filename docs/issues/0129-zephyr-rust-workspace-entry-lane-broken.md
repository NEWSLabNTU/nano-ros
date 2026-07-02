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
