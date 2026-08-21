---
id: 734
title: "`nros-rmw-zenoh` is compiled TWICE into a C++ image — two `.bss` copies of the subscriber state, ~195 KiB, on different addresses"
status: resolved
type: bug
severity: high
area: rmw-zenoh
related: [issue-0135, rfc-0064]
---

# 0734 — one RMW, two copies of its state

A C++ Zephyr image links **both** `libnros_c.a` and `libnros_cpp.a`, and each
carries its **own build** of `nros-rmw-zenoh`. Not one archive referencing the
other's symbols — two independent compilations, under different `-C metadata`,
whose statics therefore do not even collide. The linker allocates both.

Measured on `mr_canhubk3/s32k344` (S32K344, Cortex-M7, **320 KiB SRAM**),
`zephyr_pre0.map`:

```
0x20474cf1  0x20000  libnros_c.a  (nros_rmw_zenoh-95bdf968fea729fd…cgu.04)
0x2049ccfb  0x20000  libnros_cpp.a(nros_rmw_zenoh-87d8cdd856e25f43…cgu.04)

.bss._RNvNtNtCsewqHElJteY4_14nros_rmw_zenoh4shim10subscriber18SUBSCRIBER_BUFFERS
.bss._RNvNtNtCshvwoP2UscId_14nros_rmw_zenoh4shim10subscriber18SUBSCRIBER_BUFFERS
```

Same crate, same module path, same statics — different crate disambiguators
(`Cs·ewqHElJteY4·` vs `Cs·hvwoP2UscId·`), different addresses.

Totals per copy, from the same map: `0x20000` + `0x8a60` + `0x8000` ≈ **195 KiB**.
So ~195 KiB of a 320 KiB part is a second copy of state that should be a
singleton.

## Why this is worse than footprint

`SUBSCRIBER_BUFFERS`, `NEXT_BUFFER_INDEX`, `NEXT_SMALL_PAYLOAD`,
`NEXT_LARGE_PAYLOAD` and `OVERFLOW_DROPS` are the subscriber's ring state. Two
copies means two rings. Any path that reaches zenoh through the C archive and
any path that reaches it through the C++ archive are then reading and writing
**different buffers**, and `OVERFLOW_DROPS` counts only half the traffic each.

Whether both copies are live at runtime has NOT been established here — the
image does not link yet (it is 851 KiB over RAM even at
`NROS_EXECUTOR_MAX_CBS=4`). The footprint cost is certain; the correctness cost
is a live risk that should be settled either way.

## What it is NOT

* **Not the consumer's Kconfig.** `NROS_C_API` and `NROS_CPP_API` are arms of
  `choice NROS_API` (`zephyr/Kconfig:18`) and are mutually exclusive. Only
  `CONFIG_NROS_CPP_API=y` is set; `CONFIG_NROS_C_API` is *absent* from the
  generated `.config`, verified. Setting it to `n` explicitly changed the
  overflow by exactly zero bytes.
* **Not the `--allow-multiple-definition` case.** That relaxation
  (`nros-rmw-zenoh-staticlib/src/lib.rs` header, `integrations/*/Make.defs`) is
  justified for CODE on the grounds that "codegen output is deterministic, so
  the duplicate definitions are byte-identical and first-wins is safe". Here the
  symbols have different mangled names, so nothing collides, nothing folds, and
  first-wins never applies. The reasoning that makes duplicate *code* harmless
  does not extend to duplicate `.bss`.

`libnros_c.a` is linked regardless of the API choice (5661 references in the
map) because the C++ umbrella bundles it — stated in four cmake sites, e.g.
`cmake/NanoRosEntry.cmake:394` *"the C++ umbrella wins whenever it exists: it
BUNDLES nros-c"*.

## Why it has not been seen

The RMW's `.bss` is ~195 KiB. On a host, on native_sim, or on QEMU boards with
megabytes of RAM, a second copy is invisible. It becomes fatal on the first
part where the whole budget is 320 KiB.

## Reproduce

```sh
west build -b mr_canhubk3/s32k344 -S nros-zenoh <a C++ entry>
grep -E '0x20[0-9a-f]+ +0x20000 .*nros_rmw_zenoh' build/zephyr/zephyr_pre0.map
```
Two lines, two addresses = the bug.

## RESOLVED (2026-08-20)

Not a design decision after all — a **missed sweep site**. The tree already
states the rule (`cmake/NanoRosRuntimeCrate.cmake:6`, "a binary links exactly
ONE Rust staticlib"), `nros-cpp` already deps `nros-c` as an RLIB *precisely* so
one archive carries both (`nros-cpp/Cargo.toml:223`, "Replaces the separate
`libnros_c.a` + backend-staticlib links"), and issue 0425 already swept the
generic cmake path. `zephyr/CMakeLists.txt` was simply never swept.

Fixed by linking `nros_cpp_cargo` only. The Phase 168.X comment that justified
the second link claimed `nros_log_emit` "ships only with libnros_c.a"; `nm`
disproves it — both archives define it, and of the 427 symbols in libnros_c.a
but not libnros_cpp.a, ZERO are non-mangled C ABI. nros-c is still BUILT (the
C++ side compiles against its generated headers), just not LINKED.

Measured, mr_canhubk3/s32k344, `NROS_EXECUTOR_MAX_CBS=4`:

| | before | after |
| --- | --- | --- |
| `nros_rmw_zenoh` `.bss` copies | 2 | **1** |
| RAM overflow | 851432 | **651448** |
| | | **−199984 B (~195 KiB)** |
| new undefined symbols | | **0** |

Gated by `scripts/check-single-rust-staticlib.py` (`just
check-single-rust-staticlib`, on the `check` line): no cmake branch may link
two umbrellas. Sibling if/else arms are the correct shape and are not flagged.

**The gate's first draft passed against this very defect** — it recorded only
the first link site per umbrella, so the legitimate C-API-arm link at line 272
was remembered and the buggy second one never compared. Fixed to compare every
site pairwise, then re-verified BOTH ways: it fails on the reintroduced double
link and passes on the fix. A gate is not done when it goes green.

## Fix directions (considered before the above)

1. **Make the C++ umbrella depend on the C archive's `nros-rmw-zenoh`** rather
   than building its own, so cargo unifies one crate into one unit — the shape
   phase-361 W8.c used to collapse the duplicate `#[global_allocator]`
   ("makes the duplication impossible rather than merely discouraged").
2. **Do not link `libnros_c.a` when the C++ umbrella is selected**, if the
   umbrella genuinely bundles it. That contradicts the four cmake comments
   above, so it needs checking rather than assuming.
3. Whatever the mechanism, the invariant worth gating is: **one
   `nros_rmw_zenoh…SUBSCRIBER_BUFFERS` per image.** A map grep in a link check
   would catch a regression cheaply.

Found bringing up the autoware-safety-island MRM chain on MR-CANHUBK344.
