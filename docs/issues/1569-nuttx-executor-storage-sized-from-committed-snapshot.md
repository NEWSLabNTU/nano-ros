---
id: 1569
title: "On NuttX every executor and component buffer is sized from the committed snapshot, an upper bound 9.8 KB over the build's own executor — exact sizes need the per-build header on the NuttX include path"
status: open
type: tech-debt
area: boards, memory, build
severity: high
found: 2026-09-29
related: [issue-1568, issue-1570, issue-1115, issue-0167, issue-0464, issue-0954, issue-1437]
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
Either needs issue 1570 first: an incremental NuttX build does not recompile
a component when a header it includes changes, so a corrected size never
reaches an image that was already built.

## Acceptance

A NuttX C/C++ image whose `__nros_tier_executor_storage` / `__nros_executor_storage`
/ `GlobalStorageHolder` sizes equal the `nros_cpp_executor_storage_size()` the
image reports at boot, on both arm and riscv32, with the snapshot files either
deleted or reduced to what a TU with no build (e.g. `just check c`) needs.
