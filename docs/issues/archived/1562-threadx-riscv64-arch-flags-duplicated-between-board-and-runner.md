---
id: 1562
title: "The ThreadX RISC-V64 ISA/ABI flags and the picolibc sysroot probe exist
  twice — in the family runner and in the base board's own build script"
status: resolved
type: tech-debt
area: boards, build
severity: low
related: [0678, 1558, 1560, 1561, phase-471]
found: 2026-09-29
resolved: 2026-09-29
---

## What this is

`-march=rv64gc -mabi=lp64d -mcmodel=medany -fno-builtin` is written out in two
places, and so is the `--specs=picolibc.specs -print-sysroot` probe beside it:

| site | what it holds |
| --- | --- |
| `packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs` | `configure_riscv64` (`:350`), `get_picolibc_sysroot` (`:398`), the newlib probe (`:458`) |
| `packages/boards/nros-board-threadx/build.rs` | the same four flags (`:339`), a second `get_picolibc_sysroot` (`:360`) |

Both reach `nros_build_paths::riscv64::tool_or_legacy("gcc")` for the compiler,
so the TOOL has one spelling; the FLAGS it is handed do not. This is the same
shape phase-471 W2 removed from the FreeRTOS family, where three board scripts
each carried a private `gcc_print_file` with its own hardcoded `-mcpu` list —
and where the two halves had already drifted, because `FREERTOS_CFLAGS` won for
the compile and could not reach the multilib probe.

## Why it is filed rather than fixed

`check-board-build-wiring` gained the rule that catches this class in
phase-471 W2 — a board `build.rs` may not spell an ISA/ABI flag, because the
family builder in `nros-board-common` is what makes that one answer. The
ThreadX site carries a `// nros-board-arch-flags-exempt:` naming this issue
rather than being changed in the same commit, for two reasons:

* phase-471 W6 owns `nros-board-threadx/build.rs`'s include lists right now,
  and the flags sit fourteen lines above them;
* acceptance for this family is a **RISC-V64 ThreadX build**, not a gate —
  moving the flags without one would be exactly the change phase-471 W2 refused
  to make for the FreeRTOS overlays.

## What the fix looks like

The FreeRTOS answer transfers: one `configure_riscv64` in
`nros_board_common::threadx_*`, called by both, and ONE picolibc/newlib probe
that takes its flags from the same function the compile does — not a second
hardcoded copy of the list. The ThreadX family has no `[arch.*]` profile the
way FreeRTOS does, so the flags stay a Rust constant; what changes is that
there is one of it.

Do not fix it with a shared constant alone: the FreeRTOS drift was between the
COMPILE flags and the LIBRARY-PROBE flags, and a constant that both copy is
still two call sites that can diverge in what else they pass.

## Resolution (2026-09-29)

One home: `nros_board_common::arch_flags::riscv64`. Both call sites —
`threadx_qemu_riscv64_build::configure_riscv64` and the riscv64 branch of
`nros-board-threadx/build.rs` — call `configure()` and `add_picolibc_include()`,
and the `// nros-board-arch-flags-exempt:` marker is gone. `-march=rv64gc`
appears **zero** times outside the new module.

### The copies had already drifted, and this issue said they had not

The text above calls them "a byte-identical copy". They were not. The two
`get_picolibc_sysroot` implementations differed in exactly the way issue 0678
fixed in only one of them:

* `nros-board-common`'s returned `None` when the specs probe failed, with a
  comment recording 0678 — *a failed probe is not "picolibc is somewhere else",
  it is "this compiler is not a picolibc toolchain"*.
* `nros-board-threadx`'s fell back to a hardcoded
  `/usr/lib/picolibc/riscv64-unknown-elf`, which is what paired the provisioned
  xPack compiler (emulated TLS) with Debian's picolibc (native TLS), whose
  `libc.a` cannot then define the `__emutls_v.errno` that compiler emits.

So this was not only deduplication: it retired that fallback from the copy that
still had it. The prediction in "Why it is filed rather than fixed" — that the
FreeRTOS drift would repeat here — had already come true unnoticed.

### Two constants, because a probe passes less than a compile does

`MULTILIB` (`-march`, `-mabi`) is what SELECTS the library variant, so every
`-print-sysroot` / `-print-file-name` / `-print-libgcc-file-name` probe passes
exactly it. `CODEGEN` (`-mcmodel`, `-fno-builtin`) affects the objects we emit
and tells a `-print-*` query nothing. The compile passes both.

This is the issue's own requirement — *ONE picolibc/newlib probe that takes its
flags from the same function the compile does, not a second hardcoded copy* —
answered without pretending the two need identical flags. One hardcoded list
would make the probes pass flags they do not need; two hardcoded lists is what
this issue is about; one list built from the other is neither. Every probe goes
through a single `gcc_print()` so that "which multilib am I asking about" has
one answer, and a non-zero exit yields `None` — 0678's rule, now in one place.

Four probes collapsed into it: the picolibc sysroot, the picolibc lib dir's
newlib fallback (`-print-file-name=libc.a`, issue 0657), `libgcc`, and the
threadx copy.

### Acceptance — a build, as this issue required

| what | result |
| --- | --- |
| `nros-board-threadx-qemu-riscv64`, `--target riscv64gc-unknown-none-elf` | builds; emits `libnetxduo.a`, `libvirtio_net_netx.a`, `libthreadx_port_asm.a` |
| `nros-board-threadx`, same target, `THREADX_PORT=risc-v64/gnu` + the board's config dirs | builds; 189 objects |
| emitted objects | `Tag_RISCV_arch: rv64i…m,a,f,d,c` and `Flags: 0x5, RVC, double-float ABI` — i.e. rv64gc/lp64d, unchanged |

A control run matters here: building `nros-board-threadx` standalone with a
hand-set `THREADX_PORT` and no board config dirs fails on `fatal error:
nx_port.h`, and it fails **identically on pristine sources**. That is a
precondition of building the base crate outside its overlay, not a regression —
checked rather than assumed, because a red in an acceptance run is exactly where
a wrong conclusion gets written down.
