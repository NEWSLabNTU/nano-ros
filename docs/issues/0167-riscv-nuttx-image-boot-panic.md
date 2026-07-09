---
id: 167
title: "riscv-nuttx image panics at boot — garbage fn-ptr (EPC=0x4) inside ZenohRmw::open on the nros_cpp_init backend path"
status: open
type: bug
area: nuttx
related: [issue-0165, issue-0135, phase-285]
---

## Summary

The riscv32 rv-virt NuttX C-talker image
(`examples/qemu-riscv-nuttx/c/talker/build-zenoh/nuttx_riscv_c_talker`) **panics
at boot**, before the talker runs. Exposed by phase-285 W2's new
`QemuProcess::start_nuttx_riscv` harness — this is riscv-nuttx's FIRST boot; the
board had only ever been link-checked (nightly `build-riscv-c`), never run, so
the crash was latent.

```
riscv_exception: EXCEPTION: Instruction access fault. MCAUSE: 00000001, EPC: 00000004, MTVAL: 00000004
riscv_exception: PANIC!!! Exception = 00000001
up_dump_register: EPC: 00000004 ... RA: 00000004
```

`EPC = RA = 0x00000004` is a **jump/return through a garbage function pointer**
(a small non-null value, ~`0x4`) — not a plain null, so it slips past `beqz`
null-guards.

## Reproduce

```
just nuttx build-riscv-c   # builds the rv-virt kernel + the C talker image
qemu-system-riscv32 -M virt -bios none -nographic -icount shift=auto \
    -kernel examples/qemu-riscv-nuttx/c/talker/build-zenoh/nuttx_riscv_c_talker \
    -netdev user,id=net0 -device virtio-net-device,netdev=net0
# → riscv_exception PANIC within the first tick
```

(Or via the test harness: `QemuProcess::start_nuttx_riscv(binary, true)`.)

## Diagnosis (gdb on rv-virt, 2026-07-09) — root cause located

Ran `qemu-system-riscv32 … -gdb tcp::1234 -S` + `riscv-none-elf-gdb`. Caught the
fault at `riscv_exception` (mcause=1, instruction access fault) and a
hardware-breakpoint at `*0x4`, then walked the call chain by breaking at each
layer and confirming which frame is entered but never returns. The crash is **NOT
in the kernel work-queue / netdev / virtio path** (those run fine — the virtio-net
MMIO `metal_io` regions have valid ops, notify works). It is on the **nano-ros
backend-open path**, fully symbolized:

```
nros_app_main
  → nros_cpp_init(config, "node", storage)          # s0 held the "node" name string at the fault
      → nros_app_register_backends → nros_rmw_zenoh_register   [OK — CffiRmw registered into REGISTRY @0x8009da40]
      → getenv / config parse                          [OK]
      → REGISTRY scan → matches CffiRmw
      → <CffiRmw as Rmw>::open                          [0x80012ed6]
          → CffiSession::open_with_vtable              [0x80011846]
              → vtable dispatch  lw a5,0(s8); jalr a5  [0x80011934]  (vtable @0x80089314 — all 16 entries VALID)
                  → vtable[0] = nros_rmw_cffi::rust_adapter::open_trampoline::<ZenohRmw>  [0x80013078]
                      → ZenohRmw::open → zenoh-pico session open
                          → jump/ret through a GARBAGE fn-ptr (~0x4) → instruction-access fault
```

Every layer down to `open_trampoline<ZenohRmw>` is entered and never returns; the
C-FFI vtable itself is intact. The bad pointer materializes **inside
`ZenohRmw::open` / the zenoh-pico session bring-up**. At the fault: `EPC=RA=0x4`,
`a0=a5=0`, `a4=0xe0`, `s0`→rodata `"node"`, a shallow near-leaf stack — i.e. a
function-pointer read from an uninitialized / wrong-offset struct field, then
tail-jumped.

## Leading hypotheses (fix direction)

The garbage-but-non-null pointer read from a struct field points at one of two
known nano-ros failure modes, both riscv-specific because the board was only ever
link-checked:

1. **zpico shim ↔ zenoh-pico config ABI mismatch (issue [0135]).** Flag-gated
   struct fields (`Z_FEATURE_LOCAL_QUERYABLE`, …) shift offsets between mismatched
   TUs, so a function pointer gets read from the wrong offset → garbage. If the
   riscv-nuttx build doesn't inject the shared generated zenoh config into both
   the shim and the library, this is exactly the observed symptom. **Rebuild the
   riscv fixture after any zpico config change.**
2. **Backend/transport init not wired on riscv (no ctor sections — CLAUDE.md
   pitfall; #48).** A zenoh-pico transport/link callback or a ctor-initialized
   global that stays uninitialized (garbage) on nuttx-riscv, then dispatched.

## Next steps

Trace into `open_trampoline<ZenohRmw>` → `ZenohRmw::open` → the zenoh-pico
`_z_open`/transport bring-up on rv-virt (single-step past the network-send waits)
to name the exact struct field / config flag whose pointer reads `~0x4`; compare
the arm-nuttx build's zenoh config injection + generated-config sharing against
riscv. Tracked under
[phase-285](../roadmap/phase-285-riscv-nuttx-run-tiers-boot-harness.md) W2.b — it
gates the riscv-nuttx runtime (and thus #165's `run_tiers` proof).

[0135]: archived/0135-native-zenoh-service-query-path-broken.md
