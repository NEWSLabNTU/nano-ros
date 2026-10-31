---
id: 2122
title: "`rmw_coordinate_truth` runs out of its 25 s `nm` budget on the tier-2 runner and reports 16 rows unread"
status: resolved
type: bug
area: [ci, testing]
severity: low
found: 2026-10-28
related: [2112, 2127, 1984, 1690]
resolved: 2026-10-31
---

## Measured

- Run: run-matrix 40643270547 (schedule, 06:20 UTC, main at `e0f81b4ac`).
- Job: 122666117078, `tier 2 (1-wise matrix)`.

```
rmw_coordinate_truth.rs:485:5:
16 row(s) got no `nm` verdict within budget (located 93 binaries in 2.6s, `nm` over them in 26.4s;
slowest: workspace-cpp-native-service-server 9.0s, workspace-cpp-native-service-client 8.9s, …):
  workspace-features-rust-before-boot: not read — the sweep had spent its 25s
```

The run's other real failure is issue 2112 (FreeRTOS `loan-copy-sub` MISSING).

## Reading

The test reads every linked artifact with `nm` inside a fixed 25 s budget. Single
binaries took 7–9 s here, so the budget measured the runner's load more than
the artifacts. The run-matrix tier-2 run of 2026-10-27 did not fail this test.

## What it is NOT

- **Not a wrong backend.** No row reported a mismatch. The 16 rows were not read
  at all.

## What would close it

Decide between two fixes:

- size the budget from the measured per-binary cost;
- or read the symbols in parallel.

Then confirm in a tier-2 run.

## Resolution

Neither listed fix. The symbols are now read IN PROCESS, and the budget is
gone, because there is nothing left for it to bound.

**Why not "size the budget" or "read in parallel".** The sweep was already
parallel (issue 1690's pool), and issue 2127 had already given it
`threads-required = 8` (confirmed on tier 2: 0.60 to 0.72 s). The cost was
never the artifacts. Measured on this host (4 cores, HDD), 85 located rows:

| read | warm cache | warm, CPU loaded | cold cache (evicted with `POSIX_FADV_DONTNEED`) |
| --- | --- | --- | --- |
| `nm`, pool of 4 (the old shape) | 0.93 s (in the test), 8.0 to 8.2 s (host busy) | 2.2 s | 12.3 to 14.5 s; one in-test run 21.4 s, 1.4 s for one binary |
| in-process reader | 32 to 33 ms | 96 ms | 5.3 to 6.5 s |

One 14 MB C++ binary: `nm` 0.04 s idle, 0.36 s cold, 9 s on the loaded tier-2
runner. A wall-clock budget over that measures the host, not the artifact.
`nm` starts a process, links libbfd, sorts and prints every symbol, and the
test parsed the text back. The reader reads the header, the section headers,
`.symtab` and its string table with positioned reads.

**Fix.**

- New `nros_tests::elf`: an in-process reader for ELF section headers and the
  static symbol table, both classes and both byte orders. It has a synthetic
  ELF builder (`elf::synth`) for negative controls. `zephyr_link_layout.rs`
  had its own ELF32-only copy, which now uses the shared reader.
- `rmw_coordinate_truth`: no `nm`, no `NM_BUDGET` or `NM_SWEEP_BUDGET`, and the
  issue 2127 `threads-required` override is removed. Per-row timings are still
  printed on every run (issue 1984).
- **An unread row now FAILS.** Before, an artifact `nm` could not read, or one
  with no backend symbols, was counted as "unreadable" and its row passed
  unchecked. Now an unreadable file, or one with no `.symtab`, is listed by
  row. A readable image with no backend symbols is "links none of it".
- **Imports count.** `workspace-c-native-cyclonedds` links Cyclone as a shared
  library (`NEEDED libddsc.so.0`, 51 `U dds_`, no `T dds_`). The `nm` counter
  saw zero backend symbols and filed it under unreadable, so the row was never
  checked. A non-weak `U` in a linked executable is resolved by a shared
  library, so it now counts. Weak references and data still do not.

**Evidence.**

- The reader and `nm` agree exactly on all 85 binaries (count of `[tTU]`
  symbols per backend prefix).
- Mutations, each restored and `cmp`-clean:
  - zenoh prefix `_z_` changed to `_zz_`: RED, 76 rows "links none of it".
  - one row's read returns `Err`: RED, all 4 `service_server` rows listed as
    "could not be read".
  - imports not counted: RED, `workspace-c-native-cyclonedds` "links none of
    it".
- `zephyr_link_layout`: the boot-config and serial-only tests pass on the
  ELF32 ARM images through the shared reader. Those fixtures are stale, so this
  was run with `NROS_SKIP_FIXTURE_CHECK=1`; the reader is what was under test.
  The DTCM fixture is not built here.
- Tier: `just ci gate`.

**Coverage, and what tier 2 still has to show.** Measured on the 85 native and
host rows this checkout has built. The tier-2-only rows (NuttX RISC-V,
FreeRTOS-POSIX, ThreadX-Linux) were not read here. The "no budget" part cannot
regress there: no budget exists to run out. The stricter unread and imports
rules are new on those rows. If one of them is stripped or links no backend
symbol, tier 2 now names it instead of passing it. That would be a real
finding, not this issue coming back.

**Sweep:** `git grep -nE 'Command::new\("(nm|objdump|readelf)"\)|budget' -- packages/testing/nros-tests/tests`.
Seven other tests run `nm` (`interface_subset_linkage`, `workspace_metadata`,
`workspace_shadowing`, `zpico_build_matrix`, `staticlib_duplicate_symbols`,
`subnode_freestanding_link`, `zephyr_cortex_m_qemu`). Each makes one
call per test with no wall-clock budget, so none has this shape. The `scripts/` `nm`
callers have no time budget either.

