---
id: 131
title: "ThreadX RISC-V64 zenoh firmware faults at NULL c_app_main after any rebuild — lane green only on stale binaries"
status: resolved
type: bug
area: threadx
related: [phase-277, phase-177]
---

## Summary

During phase-277 W4 (chatter parity) the ThreadX RISC-V64 QEMU lane was
observed to pass **only with stale prebuilt firmware**. Any rebuild of the
Rust examples (even at the pre-W4 baseline commit, unmodified source) produces
firmware that faults early with a jump through a NULL `c_app_main` pointer.

This means the lane's green status does not certify current sources: it
certifies whatever binaries were last left on disk.

## Evidence

- Baseline test: at commit `ea825a341` (pre-W4), `git stash` clean tree,
  rebuild the talker fixture, run the two-QEMU pub/sub e2e → same NULL
  `c_app_main` fault. So this is **not** caused by the W4/W3 example changes
  (they only made `app_main` unconditional; the fault reproduces without them).
- See `tmp/sdd-277/task-9-report.md` (phase-277 working notes) for the exact
  rebuild + QEMU invocations used.

## Suspected area

The boot path resolves `c_app_main` (CMake/cyclonedds `startup.c` symbol vs
the Rust `app_main` in the example staticlib) at link time; a link-order or
weak-symbol regression could leave the pointer NULL in fresh links while old
binaries still carry the resolved address. Compare the working stale ELF's
symbol table against a fresh link.

## Impact

- ThreadX RISC-V64 runtime e2e is unreliable as a gate (false green).
- Blocks trusting phase-277 W4 runtime verification on this platform
  (builds are green; runtime unproven).

## 2026-07-04 update — three defects peeled, two remain

Fresh-rebuild investigation (fixtures fully rebaked) split the "false-green
lane" into five concrete defects. FIXED:
1. phase-277 W6 guard mis-splice corrupted the 6 rust cyclonedds
   CMakeLists (self-recursive add_subdirectory) — fixed, fixtures build.
2. Port drift: bakes never followed the Phase 89.13 per-(variant,lang)
   zenohd table — rust deploy blocks + 12 fixture rows now aligned.
3. Guest IP: rust deploy blocks lacked net keys, so NetX came up on the
   dgram-subnet default 192.0.3.10 under slirp — now 10.0.2.15/24.
Result: **C++ pubsub e2e passes** (first ThreadX-RV64 zenoh runtime green).

REMAINING (the real #131 tail):
4. **C images crash `jalr -> 0`** (`mcause=1`, `mepc=0`) — only AFTER a
   successful router connect; without a router `c_app_main` returns
   cleanly. So the null call sits in the ACTIVE zenoh session path
   (zenoh-pico rx/lease task or a platform vtable slot), not the entry
   registration (`app_main` is present at 0x800000f0 and gets called).
   Prime suspect: a wrong/absent symbol masked by the examples'
   `-Wl,--allow-multiple-definition` (#138) — exactly the wrong-copy
   hazard that flag hides.
5. **Rust zenoh images emit NO wire traffic** (empty `filter-dump` pcap,
   not even ARP) while booting to `Executor::open` — BSD/zpico TX path
   dead on this port for the cargo-built images; cross-ref #132 (these
   combos never ran green anywhere).

## Next steps

1. Reproduce: clean `target*/` in one threadx example + fixture dir, rebuild,
   run lane, capture fault PC + symbol table diff vs a stale-good ELF.
2. Bisect link inputs (board crate, startup objects, `--gc-sections`).
3. Once fixed, re-run the W4 chatter e2e on this platform.


## 2026-07-04 deep-dive — C defect root-caused to a null `drop_fn` in `Executor::drop`

gdb (`hbreak *0`) on the freshly-rebuilt riscv64-threadx **C** talker pins the
`jalr -> 0` exactly: the null call is
`<nros_node::executor::spin::Executor as Drop>::drop+94` — the SECOND drop
loop, `(meta.drop_fn)(arena + meta.offset)`, with `a0 = data_ptr` a valid
arena address (`0x80048ae0`) but `a1 = meta.drop_fn = 0`. The crash fires
**before any `Publishing:`** (executor is torn down almost immediately after
`c_app_main`), so it is an early-exit teardown, and the C++ pubsub sibling is
green (stays on the happy path, executor never drops).

Ruled out:
- **Uninitialised entries** — `storage::carve` writes `None` to every
  `entries` slot (storage.rs:223), so a fresh Executor's Option discriminants
  are valid.
- **App-thread stack overflow** — the riscv64 board strong-overrides
  `nros_board_app_stack_size` to 512 KB (board_threadx_qemu_riscv64.c:49).
- **A defensive null-guard in `Executor::drop`** — tried and reverted: both
  `slot.drop` and `meta.drop_fn` are non-nullable Rust `fn` types, so
  `(x as usize) != 0` is a tautology the optimizer deletes. A `read_volatile`
  guard would only MASK the corruption (violates the fail-loud rule).

So a real `CallbackMeta` entry (valid `offset`) is written/held with a NULL
`drop_fn` on the threadx-C path — genuine memory corruption or a specific
entry-write that leaves `drop_fn = 0`. Next: instrument `add_*` / the C
component (`nros_cpp_timer_create` -> `TimerEntry`) registration to catch the
entry whose `drop_fn` is null at write time, or watchpoint the entry slot.
Deep, dedicated-session work — not a quick fix.

The RUST riscv64-threadx defect (empty pcap — no ARP, network never comes up)
is a SEPARATE transport-down issue, unrelated to this teardown crash.

## 2026-07-04 — C defect CONFIRMED + validated: FFI struct-size mismatch from a stale config-header mirror

Root cause found and confirmed. The null `drop_fn` is a classic FFI
struct-size disagreement, driven by a **stale in-tree config-header mirror**:

- The C example's executor instance `__nros_c_inst_<pkg>` is a C global of
  type `nros_executor_t`, whose `_opaque[…]` is sized on the C side from the
  `NROS_*_STORAGE_SIZE` macro in `nros_config_generated.h` — the **mirror**
  copy under `<build>/nano_ros/packages/core/nros-c/include/nros/`.
- Rust (`nros_executor_init`) writes a fresh-sized `ExecutorInlineStorage`
  (`carve` lays out `entries`/`sched_contexts`/arena for `ExecutorSizing::DEFAULT`)
  straight into that C pointer, using `EXECUTOR_BACKING_U64S` — computed from
  the crate's own fresh build. **No buffer length crosses the FFI.**
- When the mirror is stale-small (observed 79984 vs fresh 81368), the C global
  is smaller than what Rust writes; the carved `entries` table runs off the end
  of `__nros_c_inst` into the adjacent `.bss` (zeroed) → a `CallbackMeta` with
  `drop_fn = 0` → `jalr -> 0` in `Executor::drop`. The C++ sibling only stayed
  green because it kept to the happy path (executor never dropped early).

**Validated:** a clean rebuild (`rm -rf build-zenoh`, rebuild via
`just threadx_riscv64 build-fixture-extras`) makes ALL five copies of the
header agree at 81368 (build-dir == both mirrors), and
`test_rtos_pubsub_e2e ThreadxRiscv64 lang_2_Lang__C` **PASSES** (35s). First
green ThreadX-RV64 zenoh **C** runtime.

**Why the mirror went stale — operational, not a code defect:** within a single
clean build tree the mirror cannot drift. The header is a `build.rs` output, so
any size change forces a `build.rs` rerun → the `cargo-build_nros_c` target
reruns → `libnros_c.a` relinks → the mirror `add_custom_command`'s
`DEPENDS cargo-build_nros_c` fires the `copy_if_different`. The stale mirror only
arises from **interrupted / cross-arch incremental builds** (orphaned cargo
children leaving a half-written tree — see agent-memory
`env_disk_sccache_fixture_build_gotcha`). Fixture builds use fresh dirs, so CI
is unaffected.

**Durable backstop (shipped):** `executor::storage::carve` guarded its backing
bound with `debug_assert!`, which is compiled out of the embedded
release/`nros-fast-release` profiles — so an under-sized backing overflowed
silently instead of panicking. Promoted to a real `assert!`: any
backing/layout size disagreement now panics loudly at `Executor::open` on every
profile, with both sizes named. (Note: this fires for callers that hand-size a
backing via `from_session_in` / the `nros::main!` macro; it does NOT fire for
the C-FFI path here, which passes the fresh `EXECUTOR_BACKING_U64S` length
regardless of the C global's actual size — that mismatch has no length crossing
the FFI to check. It is nonetheless the correct fail-loud hardening for the
whole silent-overflow class.)

**Rejected fix — cmake mirror `DEPENDS` on the generated source header:** tried
adding the cargo-written headers to the mirror `add_custom_command` `DEPENDS`
(+ `set_source_files_properties(GENERATED)`). Ninja rejects it —
`'…/nros_config_generated.h', needed by '…/include/nros/…', missing and no known
rule to make it` — because a file-level dependency needs a producing rule and
cargo emits the header opaquely (corrosion does not declare it as a byproduct).
Reverted. A bulletproof mirror-freshness guarantee (BYPRODUCTS on the corrosion
target, or writing the header directly into the `nros/` include layout) is
deferred as dedicated build-system work; it is not needed for correctness given
the operational cause above.

Status: the **C** crash (defect 4) is root-caused, fixed-by-clean-build, and
backstopped. Remaining open: defect 5 (Rust zenoh TX-dead / empty pcap), a
separate transport issue.

## 2026-07-05 — defect 5 (Rust zenoh TX-dead) FIXED; lane green

The Rust "emits no wire traffic" defect was four stacked problems (commit
`c523c5d68`); pubsub + service e2e now PASS.

1. **No backend registered (the TX-dead root cause).** ThreadX is
   `target_os = "none"`, so `nros_rmw_register_backend!`'s `.init_array` ctor is
   a no-op and the flat image runs no static ctors — nothing registered the
   zenoh vtable, so `resolve_backend` → NoBackend and `Executor::open` failed
   with `Transport(ConnectionFailed)` BEFORE any socket I/O (hence the empty
   pcap; it was never a NIC/transport bug). The example linked `nros-rmw-zenoh`
   but never called `register()`. Fix: `run_app_thread` calls
   `nros_rmw_zenoh::register()` before `Executor::open` (nuttx / freertos / mps2
   pattern), forwarded family → per-board → example via `rmw-zenoh`. Validated:
   pcap 24 B (empty) → full ARP → TCP → zenoh handshake.
2. **`__assert_func` → undefined `stderr`** on link (registering zenoh pulls
   `assert()` code → newlib `__assert_func` → `fprintf(stderr,…)`; no stderr on
   bare-metal). Fix: board provides a strong `__assert_func` → UART + fail-exit.
3. **`log::info!` dropped** (no sink) → the harness saw 0 messages. Fix:
   `run_app_thread` installs a no_std UART `log` sink + emits `nros entry ready`;
   rtos_e2e's readiness gate learns the ThreadX-RV64 rust case.
4. **Duplicate zenoh session IDs → 0 delivery.** Both nodes baked the same
   ip/mac → same `nros_board_compute_rng_seed` → same deterministic-RNG zid →
   the router couldn't distinguish the peers (the exact failure mode zpico.c
   warns about). Fix: second-in-pair examples bake ip `10.0.2.16` for a unique
   seed/zid.

`test_rtos_pubsub_e2e` (35s) + `test_rtos_service_e2e` (40s) ThreadxRiscv64 /
Rust both PASS. No action e2e exists for this combo. This issue's defects 1–5
are all resolved — ready to move to `archived/` once the C `#131` mirror
backstop (b122bc6a1) and this land together.
