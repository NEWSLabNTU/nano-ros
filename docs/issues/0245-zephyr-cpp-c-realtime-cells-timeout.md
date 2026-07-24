---
id: 245
title: "realtime_tiers_e2e zephyr_cpp + zephyr_c cells time out on a fresh native_sim image (pre-existing; baseline-verified)"
status: open
type: bug
severity: medium
area: zephyr
related: [issue-0164]
---

## Finding (2026-07-23, during the phase-296 W5.5 C/C++-consumer work)

`realtime_tiers_e2e::case_06_zephyr_cpp` and `case_07_zephyr_c` **time out
(60 s)** — solo and in-sweep — on freshly built `ws-realtime-{cpp,c}` Zephyr
native_sim images.

**Baseline-verified pre-existing:** with the W5.5/W5.7 C/C++-consumer changes
stashed, the in-tree `nros` CLI rebuilt from clean main, and the
`ws-cpp-realtime` fixture rebuilt from scratch, `case_06_zephyr_cpp` still
times out identically. Not introduced by the tier-spec ABI append.

Manual boot of `build-ws-cpp-realtime-entry-zenoh/zephyr/zephyr.exe` prints
the Zephyr boot banner and then **nothing** — no nros output at all (with no
router; the harness's router makes no difference to the silence).

The sibling `case_05_zephyr_rust` (same workspace, Rust entry, same session
shape) passes in ~3 s, so the fixture bake + harness + router path are fine;
the failure is specific to the C/C++ `nros_board_zephyr_run_tiers` image.

## Notes / suspicions

- The C/C++ zephyr tier image boots through `main()` →
  `ZephyrBoard::run_tiers` (main.hpp) → `nros_board_zephyr_run_tiers`
  (`nros-board-zephyr/c/zephyr_run_tiers.c`). Total silence suggests it
  never reaches the first session-open log — possibly hanging in
  `nros_cpp_init` (zenoh open) or earlier in network bring-up.
- May be the museum-binary class (issue 0164): these cells may not have run
  on a FRESH image for a long time — the timeout may predate this check by
  many phases.
- The `zephyr-qos-port`-style serialization is not the cause (solo run red).

## Root cause captured (2026-07-24 debugging session)

**Deterministic repro:** the guest crashes 5/5 when a REMOTE SUBSCRIBER
exists (router + a native `int32-sink` on `/telem` connected before boot),
and runs clean 6/6 without one. Not seed-dependent — subscriber-presence
gated. This is why every harness run fails (the harness always spawns the
observers first) while a bare manual boot looks healthy.

**Backtrace (gdb, thread 11 = the ctrl tier k_thread):** SIGSEGV in the
Zephyr SYSTEM-HEAP free list —
`free_list_remove_bidx (heap.c:51)` ← `sys_heap_free` ← `k_heap_free` ←
`_z_slice_clear` ← `_z_keyexpr_clear` ← `_z_keyexpr_declare_prefix
(keyexpr.c:945)` ← `_z_declare_publisher` ← `z_declare_publisher` ←
`zpico_declare_publisher_ex (zpico.c:1295, keyexpr
"0/ctrl/std_msgs::msg::dds_::Int32_/TypeHashNotSupported")` ←
`nros_rmw_zenoh::zpico::declare_publisher` — i.e. heap ALREADY corrupted
when the ctrl tier's declare frees its prefix slice.

**Race shape:** the boot thread's `nros_cpp_spin_once` drives the zenoh-pico
read path, which processes the incoming REMOTE SUBSCRIBER INTEREST
(write-filter/declare handling — allocs + frees) concurrently with the
spawned tier thread's `z_declare_publisher` on the same session. The
issue-#144 chained-spawn serialized declare-vs-declare, resting on the
assumption "a spin exchanges keepalives/data, not declares" — that
assumption is FALSE once a remote subscriber interest arrives mid-spin:
interest processing mutates the same session/keyexpr/publisher state the
concurrent declare mutates → system-heap corruption.

Both the cpp and rust images bake `Z_FEATURE_MULTI_THREAD=1` (290 defines ≥
253 zenoh TUs in the cpp build.ninja — no obvious 0135-class per-TU flag
mismatch), yet the rust image survives the same topology — the C/C++ path
(`nros-cpp` over `zpico.c`) either misses a session lock the Rust path
takes, or the two link different zenoh-pico builds (the image carries BOTH
the cmake-built zenoh-pico TUs and the Rust `zpico-sys` staticlib — worth
ruling out a mixed-copy link even though the dup-symbol gate should catch
identical names).

**Next (fix wave):** (a) determine which zenoh-pico copy the crashing path
links and whether nros-cpp's declare path holds the session mutex that the
Rust `zpico_spin_once`/declare pairing uses; (b) if the lock exists and is
taken, the race is inside zenoh-pico's interest handling vs declare —
candidate upstream/fork fix; (c) interim mitigation: defer the boot spin
until the whole spawn chain completes (extends #144's serialization to
declare-vs-spin).

## Repro

```
just zephyr build-fixtures
cargo nextest run -p nros-tests -E 'test(case_06_zephyr_cpp)'
# TIMEOUT 60 s; manual boot shows banner-then-silence
```
