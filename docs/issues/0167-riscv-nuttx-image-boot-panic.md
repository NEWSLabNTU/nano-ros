---
id: 167
title: "riscv-nuttx image panics at boot — null fn-ptr call (EPC=0x4) in hpwork, before the app runs"
status: open
type: bug
area: nuttx
related: [issue-0165, phase-285]
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
dump_assert_info: ... task: hpwork process: Kernel 0x80065bd2
up_dump_register: EPC: 00000004 ... RA: 00000004
dump_tasks: ... hpwork (Running), nsh_main (Ready), telnetd (Ready)
```

`EPC = RA = 0x00000004` is a **null function-pointer call** (`(*null)()` — a call
through a null struct/vtable pointer at field offset 4), taken in the **`hpwork`**
high-priority work-queue kernel thread. The panic fires during boot, before
`nsh_main` / the C talker's `app_main` produce any output.

## Reproduce

```
just nuttx build-riscv-c   # builds the rv-virt kernel + the C talker image
qemu-system-riscv32 -M virt -bios none -nographic -icount shift=auto \
    -kernel examples/qemu-riscv-nuttx/c/talker/build-zenoh/nuttx_riscv_c_talker \
    -netdev user,id=net0 -device virtio-net-device,netdev=net0
# → riscv_exception PANIC in hpwork within the first tick
```

(Or via the test harness: `QemuProcess::start_nuttx_riscv(binary, true)`.)

## Candidates

A null callback dispatched on the `hpwork` queue at boot — something that on arm
is wired but on riscv is not:

- **Backend register / ctor-init not wired on riscv.** The arm path wires the RMW
  backend explicitly (no POSIX ctor sections on embedded — CLAUDE.md pitfall; #48
  freertos: "backend must be LINKED + registered"). If a `work_queue()`-scheduled
  init callback is registered null (or a `.init_array`/ctor the riscv link drops),
  the first `hpwork` dispatch jumps to ~null.
- **nros-nuttx-ffi `main` → app_main path.** Trace whether the riscv
  `nros-nuttx-ffi` main registers the backend + reaches `app_main` before the
  crash, or whether the crash precedes it (kernel work-queue bring-up).
- **Linker: a weak/undefined symbol resolved to 0x0** then called (EPC 0x4 = null
  + a small field offset).

## Next steps

Symbolize the crash against the ELF (`riscv-none-elf-addr2line`/`gdb` on
`nuttx_riscv_c_talker`: the frames `0x80065bd2`, `0x8006716e`, `0x800893f4`,
`0x80036f1e`), identify the null callback + who was supposed to set it, and
compare the arm vs riscv board init/register wiring. Tracked under
[phase-285](../roadmap/phase-285-riscv-nuttx-run-tiers-boot-harness.md) W2.b — it
gates the riscv-nuttx runtime (and thus #165's `run_tiers` proof).
