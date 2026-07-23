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

## Repro

```
just zephyr build-fixtures
cargo nextest run -p nros-tests -E 'test(case_06_zephyr_cpp)'
# TIMEOUT 60 s; manual boot shows banner-then-silence
```
