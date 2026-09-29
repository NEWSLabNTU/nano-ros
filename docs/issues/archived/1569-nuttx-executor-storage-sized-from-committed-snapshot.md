---
id: 1569
title: "On NuttX every executor and component buffer is sized from the committed snapshot, an upper bound 9.8 KB over the build's own executor — exact sizes need the per-build header on the NuttX include path"
status: resolved
type: tech-debt
area: boards, memory, build
severity: high
found: 2026-09-29
resolved: 2026-09-29
related: [issue-1568, issue-1570, issue-1115, issue-0167, issue-0464, issue-0954, issue-1437, issue-1512]
---

# NuttX sizes its storage from a frozen number

Issue 1568 made every RTOS board take executor storage one way — a `.bss`
static the generated entry sizes from `NROS_CPP_EXECUTOR_STORAGE_SIZE`, checked
at run time by the linked library (`nros_cpp_executor_storage_check`). On every
board but NuttX that macro is the build's own number. On NuttX it is not.

## What NuttX reads

The per-build `nros_cpp_config_generated.h` is on NO NuttX include path
(issue 1115), so `<nros/nros_cpp_config_generated.h>` resolves to the stub,
which under `NROS_PLATFORM_NUTTX` includes the committed
`nros_cpp_config_generated_nuttx.h`. Its values against the per-build headers
the realtime fixtures' own cargo builds wrote beside them:

| Macro | Snapshot (before 1568) | arm, 2026-09-04 | riscv32, 2026-09-03 | arm, 2026-09-29 (1568's branch) |
| --- | ---: | ---: | ---: | ---: |
| `NROS_CPP_EXECUTOR_STORAGE_SIZE` | 98,312 | 88,560 | 88,480 | 89,072 |
| `NROS_PUBLISHER_SIZE` | 560 | 548 | 548 | **596** |
| `NROS_SUBSCRIBER_SIZE` | 560 | — | — | **620** |
| `NROS_SERVICE_SERVER_SIZE` | 528 | — | — | **568** |
| `NROS_SERVICE_CLIENT_SIZE` | 4,632 | 520 | 520 | 568 |
| `NROS_CPP_ACTION_SERVER_STORAGE_SIZE` | 88 | 72 | 84 | 80 |
| `NROS_CPP_ACTION_CLIENT_STORAGE_SIZE` | 48 | 24 | 24 | 24 |

(Per-build headers under `examples/workspaces/realtime-{c,cpp}/build/nuttx-*/
cmake/cargo-target/nros-cpp-generated/`.) The bold values were ABOVE the
snapshot: an "upper bound" that had silently stopped being one, which issue
1568 measured as a live 36-byte publisher overrun on realtime-c and raised
(with the raw/opaque u64 counts that had drifted the same way). That is the
drift this issue exists to end — the numbers were right in early September
and wrong by the end of it, with nothing noticing.

The executor half cannot overrun silently any more: since 1568 a snapshot
that falls below the linked library's size is REFUSED at boot with both
numbers named. The executor storage is instead OVER-reserved: realtime-c's
`__nros_tier_executor_storage` is 196,624 bytes (2 × 98,312) for 178,144
needed.

**The snapshot cannot be made exact even by hand, and one struct shows why.**
`nros_subscription_t` has a field AFTER `_opaque[SUBSCRIPTION_OPAQUE_U64S]`
(the issue-1437 executor pointer), so any snapshot value other than the
build's own puts that field at a different offset in a C TU than in the Rust
that writes it. Raising the bound (1568) turned "Rust writes past the C
object" into "Rust writes inside it" — safe for as long as only Rust reads
the field, and still an ABI split.
But it is not the build's number: a two-tier NuttX image reserves 2 × 9,752
bytes it never uses, and one file stands for two architectures that measure
differently, so no snapshot value can ever be exact for both.

The component buffers (`NROS_C_*_STORAGE_SIZE`, issue 1568) have no runtime
refusal — `nros_cpp_publisher_create` and siblings take no size — so on NuttX
their only protection is the snapshot staying above every per-build value by
hand, the #167/#464/#954 line.

## Why it was not fixed in 1568

Two routes, both larger than 1568's scope:

1. **Put the per-build header on the NuttX include path.** The entry TU is
   compiled inside the cargo `nros-nuttx-ffi` build (after nros-cpp's build
   script has written the header), but the C/C++ interface and component
   libraries are compiled by cmake BEFORE cargo runs, and the 0088/0114
   ordering guards that fix this elsewhere are keyed on targets
   (`nros_c_config_header`, `cargo-build_nros_c`) a NuttX build does not
   have. Giving only the entry TU the per-build header would make
   `Node::GlobalStorageHolder<0>::storage` — a COMDAT array — have two
   different sizes in one image (the #167 ODR shape). It has to be the whole
   build graph or nothing.
2. **A gate comparing the snapshot to a build.** Needs a NuttX build, so it
   cannot be a fast-line gate; it could be a POST_BUILD check in the NuttX
   carrier that reads the per-build header cargo just wrote and fails when
   the snapshot is BELOW it (which the runtime refusal now also catches for
   the executor, but not for the component buffers).

Route 1 is the one that makes NuttX exact; route 2 only keeps the bound safe.
Either needed issue 1570 first: an incremental NuttX build did not recompile
a component when a header it includes changed, so a corrected size never
reached an image that was already built. 1570 is resolved (the NuttX image
link now declares every header its TUs read), so that precondition is met.

## Acceptance

A NuttX C/C++ image whose `__nros_tier_executor_storage` / `__nros_executor_storage`
/ `GlobalStorageHolder` sizes equal the `nros_cpp_executor_storage_size()` the
image reports at boot, on both arm and riscv32, with the snapshot files either
deleted or reduced to what a TU with no build (e.g. `just check c`) needs.

## Resolution

Route 1, the whole graph: NuttX C and C++ images now compile against the
per-build sizes headers like every other platform, and nothing in a NuttX build
can reach the committed snapshot any more.

### Why the per-build header "was on no include path" — measured

It was on the path. `nuttx_ffi_build.rs` already added
`$CARGO_TARGET_DIR/nros-{c,cpp}-generated` — the FLAT shared copies — first.
Two things made that worthless:

1. **No ordering edge.** `nros-c` carries `links = "nros_c"`, so cargo runs the
   FFI build script after it; `nros-cpp` carried no `links` key, so nothing
   ordered the FFI script after the one that writes `nros_cpp_config_generated.h`.
   On a clean build the file was not there yet and every TU fell through to the
   stub, which under `NROS_PLATFORM_NUTTX` included the snapshot.
2. **C TUs never got the C++ header.** `nros-cpp-generated` was added for C++
   builds only, so a C component's `component.h` probe of
   `<nros/nros_cpp_config_generated.h>` always reached the stub — every C
   component's publisher/subscriber buffer was the snapshot's.

And with no `rerun-if-changed` on the header, a sizes change left the entry TU
at its old size.

### The fix (three commits)

| Commit | What |
| --- | --- |
| `a29d87bb6` | `nros-cpp` declares `links = "nros_cpp"` and publishes its OUT_DIR header dir as `DEP_NROS_CPP_CONFIG_INCLUDE` (the twin of nros-c's `DEP_NROS_C_CONFIG_INCLUDE`, issue 1512). `nuttx_ffi_build.rs` reads both for C **and** C++ TUs, panics naming the file when either is missing, and watches both. The OUT_DIR copies are this unit's own, so never a sibling feature set's header (issue 0360). |
| `264e9b824` | The cmake-side compiles of the same sources (each component lib, each `<pkg>__nano_ros_c` STATIC lib) were never linked into any image and compiled against the snapshot. `nros-nuttx.cmake` declares `NROS_SIZES_HEADERS_FROM_IMAGE_CARGO`; the one helper every sizes-reading cmake lib registers with (`_nros_node_register_apply_config_header_deps_to`) marks such a lib `EXCLUDE_FROM_ALL` under it. The C message lib gained a `<lib>_gen` codegen target (the C++ branch had one), and the image build orders on it instead of on the library. |
| `b6a16a54d` | The stubs no longer dispatch on `NROS_PLATFORM_NUTTX`. See "The snapshot" below. |

### The snapshot: kept, renamed, unreachable from any build

`nros_{,cpp_}config_generated_nuttx.h` → `nros_{,cpp_}config_generated_buildless.h`,
reached only under `NROS_CONFIG_BUILDLESS`. Deleting it was the preference, and
it has real readers that compile headers with **no build at all**: API parity
(`scripts/api_parity/extract_cxx.py`), `check-cpp-capability-layout` and its
subject derivation, `check-cpp-hosted-minimal-libcpp`, and
`check-cxx-compat-shim-facilities`. They need every macro to exist and the
values to be self-consistent between arms — not to be anyone's build.

So the gate asserts no bound over a build. A `snapshot >= every per-build value`
check (this issue's route 2, and issue 0954's direction) would protect nothing
now that no image is sized from the file, and it would still need a build to
run. What the gate asserts instead is that no build CAN reach it —
`check-config-fallback-macros` rules 5/6:

* a stub may include a fallback only under `defined(NROS_CONFIG_BUILDLESS)`,
  and may condition on no `NROS_PLATFORM_*` macro;
* `NROS_CONFIG_BUILDLESS` stated outside a comment anywhere but `scripts/`, the
  stubs/snapshots and prose is refused.

Mutation-tested: restoring `#if defined(NROS_PLATFORM_NUTTX)` in the C stub
fails the gate with two findings naming this issue. Rules 1–3 (macro coverage,
exact codegen-version pair, one definition) still apply to the snapshots.

### Acceptance — measured

**Sizes equal the build's.** `qemu-armv7a-nuttx` realtime-c, same tree before
and after:

| Object | Before | After | Per-build header |
| --- | ---: | ---: | ---: |
| `__nros_tier_executor_storage` | `0x30010` = 196,624 (2 × 98,312, the snapshot) | `0x2b7e0` = 178,144 | 2 × 89,072 |
| `__nros_c_inst_ctrl_pkg` / `_telem_pkg` | `0x288` = 648 | `0x260` = 608 | `NROS_PUBLISHER_SIZE` 596 + pointer, 8-aligned |
| `nros_cpp_executor_storage_size()` | — | `movw 0x5bf0; movt 1` = 89,072 | 89,072 |

realtime-cpp arm, realtime-c riscv32 and realtime-cpp riscv32:
`__nros_tier_executor_storage` = `0x2b7e0` in all three (the per-build headers
all read 89,072). One tier block per executor, no over-reservation: the 2 × 9,240
bytes this issue recorded are gone.

**A missing header fails loudly.** With nros-cpp's OUT_DIR header held aside,
an incremental build stops in the FFI build script:

    the per-build sizes header …/nros-cpp-2ace174462bb9ab9/out/nros-cpp-generated/nros/nros_cpp_config_generated.h
    does not exist. Its writer ran (cargo orders this script after it) but wrote nothing — …
    there is deliberately no committed fallback (issue 1569).

and a NuttX TU that reaches the stub stops at its `#error` (measured before the
cmake-side exclusion landed: `builtin_interfaces_msg_time.c` →
`nros_config_generated.h:43: #error … must be supplied per-build`).

**An incremental sizes change relinks.** A temporary `+ 64` on nros-node's
derived arena, then `cmake --build` on the existing realtime-c arm dir: nros-node
→ nros-cpp (header 89,072 → 89,136) → the FFI build script re-ran on the header
watch → `__nros_tier_executor_storage` `0x2b7e0` → `0x2b860` (2 × 89,136).
Reverted, rebuilt, back to `0x2b7e0`. This path did not need issue 1570's fix:
the header is a `rerun-if-changed` input of the FFI script now, so the edge
exists. (Issue 1570 is about component sources and the headers they include,
which this does not cover.)

**Boots and ticks.** `realtime_tiers_e2e` with the four NuttX C/C++ fixtures
built: `realtime_tiers: 17 row(s) ran, 13 skipped` → the four that ran —
nuttx-arm/c, nuttx-arm/cpp, nuttx-riscv/c, nuttx-riscv/cpp — PASS
(`CounterRatio3x`); the 13 skips are other platforms' fixtures this worktree
did not build. Also built clean: the six standalone `examples/qemu-armv7a-nuttx/c/*`,
the six `cpp/*`, `workspace-c-nuttx`, and `just nuttx build-riscv-c`.
