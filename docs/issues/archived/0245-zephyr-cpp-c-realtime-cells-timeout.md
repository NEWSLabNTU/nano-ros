---
id: 245
title: "realtime_tiers_e2e zephyr_cpp + zephyr_c cells time out on a fresh native_sim image (pre-existing; baseline-verified)"
status: resolved
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

**Locking survey (2026-07-24b):** the crash stack routes through
`nros_rmw_zenoh::zpico::declare_publisher` → `zpico_declare_publisher_ex`
(`zpico.c:1257`) → `z_declare_publisher` with NO shim-level lock; the shim's
`g_spin_mutex` guards the spin/condvar path only. With
`Z_FEATURE_MULTI_THREAD=1` the background read task starts (`zpico.c:987`)
and processes interests on its own thread — zenoh-pico's internal session
mutex is supposed to make declare-vs-read safe, so either that lock is
bypassed on the interest/write-filter path (fork bug candidate) or the
zephyr `_z_mutex_t` mapping is unsound in this image. NOTE: the crashing
stack is the SAME nros-rmw-zenoh→zpico route the PASSING Rust image uses —
the images differ in spin drive (C boot loop `nros_cpp_spin_once` blocking
read vs the Rust tier loop), not in the declare path.

**Next (fix wave):** (a) determine which zenoh-pico copy the crashing path
links and whether nros-cpp's declare path holds the session mutex that the
Rust `zpico_spin_once`/declare pairing uses; (b) if the lock exists and is
taken, the race is inside zenoh-pico's interest handling vs declare —
candidate upstream/fork fix; (c) interim mitigation: defer the boot spin
until the whole spawn chain completes (extends #144's serialization to
declare-vs-spin).

## RESOLVED (2026-07-24) — executor storage 32 bytes short; heap overflow

**Root cause:** `zephyr_run_tiers.c` allocated tier/boot executor storage
from a HARDCODED `NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES 81920` ("NuttX fallback
79304 rounded up to 80 KiB for headroom") while this build's real
requirement (cmake-generated `NROS_CPP_EXECUTOR_STORAGE_SIZE`) had grown to
**81952** — 32 bytes short. The executor's TAIL state overwrote the next
Zephyr sys_heap chunk header. Subscriber-gated because the tail bytes
(subscriber-delivery state) are only written once a remote subscriber
engages; the next zenoh alloc/free then walked the corrupted free list →
the observed SIGSEGV under `z_declare_publisher`'s clear path. The Rust arm
sizes from the real generated constant (immune); native C++ includes the
real header (immune); FreeRTOS/NuttX real sizes still fit under 81920
(NuttX-arm by only 856 bytes — the same time bomb).

**Fix:** `zephyr_run_tiers.c` now `__has_include`s the generated
`nros_cpp_config_generated.h` (which IS on the Zephyr module include path)
and uses `NROS_CPP_EXECUTOR_STORAGE_SIZE` rounded up to 8; the fallback
(only for header-less builds) is bumped to 96 KiB. The freertos + nuttx×2
mirrors get the same guarded-include (real size when visible; their 80 KiB
fallback retained — real sizes fit today, and blindly bumping risks their
heap budgets).

**Verified:** the deterministic repro (router + remote sink + seeded guest)
went 5/5 crash → 5/5 clean (~795 ticks each); harness cells
`case_06_zephyr_cpp` (0.77 s) + `case_07_zephyr_c` (0.80 s) +
`case_05_zephyr_rust` all PASS; `zephyr_edf_deadline_applied` PASS;
`check-c` green.

**Debugging notes for the class:** gdb + ASAN + SYS_HEAP_VALIDATE all
distorted timing enough to mask the race-looking symptom (banner-then-
silence under heavy checkers is the checker, not the bug). The decisive
steps were (1) subscriber-presence A/B (5/5 vs 0/6), (2) single-executor
talker A/B (fine → multi-tier-specific), (3) reading the storage-size
comment against the generated header. Also: valgrind sees only the
downstream free (custom sys_heap is opaque to memcheck).

## Repro

```
just zephyr build-fixtures
cargo nextest run -p nros-tests -E 'test(case_06_zephyr_cpp)'
# TIMEOUT 60 s; manual boot shows banner-then-silence
```
