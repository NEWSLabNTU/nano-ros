---
id: 936
title: "Executor::open_in needs more than 32 KiB of the calling thread's stack, and nothing says so"
status: open
area: core, memory
severity: high
found: 2026-08-31
related: [0900, 0271, 0739, phase-403, phase-3]
---

# The init path costs more stack than a small part can give it

## What was measured

mr-canhubk344 (S32K344, 320 KiB SRAM), RFC-0043 C++ components over zenoh
on serial, `CONFIG_MAIN_STACK_SIZE` bisected against a booting image:

| main stack | outcome |
| ---: | --- |
| 8192 | overflow |
| 16384 | overflow |
| 28672 | overflow, NAMED once `CONFIG_HW_STACK_PROTECTION=y` |
| 32768 | no named overflow, but `main` corrupts the idle thread's stack |

Named report at 28672:

```
>>> ZEPHYR FATAL ERROR 2: Stack overflow on CPU 0
Current thread: main
Faulting instruction address (r15/pc): __aeabi_memclr8
r14/lr:                                 Executor::open_in
```

So the requirement is somewhere above 32 KiB, on a part with 320 KiB of SRAM
in total. The consumer is a `memclr` reached from `open_in`.

## Why it is hard to see

**Without the MPU stack guard the overflow does not report as one.** `main`
runs off its stack into whatever is next -- on this image the idle thread's
stack -- and the fault surfaces later, in a different thread, as
`USAGE FAULT: Illegal load of EXC_RETURN into PC` with `pc = 0`. That reads as
memory corruption anywhere in the image, and it names the WRONG THREAD:
`Current thread: idle`. Two separate bring-up sessions were spent on the idle
thread before the guard was enabled.

The guard is not free on a tight image (it selects `MPU_STACK_GUARD`, which adds
a guard region to every thread stack -- measured 23244 B over on this one), so
the tool that names the failure is the tool such an image cannot afford. That is
the same shape as issue 0900's note about the arena being invisible to
`mem-report`.

## What is NOT the cause

* Not the executor arena. Holding the stack at 28672 and taking
  `NROS_EXECUTOR_ARENA_SIZE` from 49152 to 40960 changed nothing -- same
  `MMFAR`, same registers.
* The FPU registers in the fault dump are stale. `s[3]`/`s[4]` read `0x0000c000`
  in both runs, which looks like an arena size and is not one; it did not track
  the knob. Do not infer from them.

## Why it matters beyond one board

`open_in` is on every image's boot path. A hosted target with an 8 MiB main
thread never notices; a 320 KiB part cannot pay it, and gets a fault that names
neither the cost nor the thread that owns it. phase-403 makes receive buffers
type-sized, which took this image's arena from 86108 B to 24516 B -- and none of
that helps, because the stack requirement is independent of it.

## Located (2026-08-31): `Executor` is a ~16 KiB VALUE, moved twice

Frame sizes read from the linked image with `objdump`, largest first:

```
19840  ZenohSession::names_and_types_filtered   (not on the boot path)
16000  Executor::open_in                        (sub.w sp, sp, #16000)
15104  nros_cpp_init
11712  Executor::register_action_server_raw
```

`open_in` and `nros_cpp_init` are the same call chain, so roughly 31 KiB of
prologue is committed before either function does any work. Both frames are one
value: `open_in` builds an `Executor` on its stack and returns it BY VALUE, and
`nros_cpp_init` holds the returned value before `ptr::write`ing it into the
caller's storage. `size_of::<Executor>()` is about 16 KiB, cross-checked against
the generated `NROS_EXECUTOR_SIZE`: it moved by exactly the arena delta (90112 B)
across two builds, leaving Executor plus tables at 22888 B.

**What is inline in a struct whose tables were supposedly moved out.**
phase-271 (issue 0110) moved six sized tables to borrowed storage and left
others behind. The dominant one:

```rust
group_sched_table: heapless::Vec<
    (String<64>, String<64>, String<32>, SchedContextId),
    { crate::config::MAX_CBS },
>
```

about 168 bytes per slot, INLINE, scaled by `MAX_CBS`. Also inline:
`extra_sessions: heapless::Vec<ConcreteSession, MAX_NODES>` (6 x 524 B here),
the `SessionStore` itself, `nodes`, `dispatch_slots`, `component_slots`.

**So `MAX_CBS` costs stack, not just arena.** Raising it from 14 to 36 to fit
this image's 33 handles added roughly 3.7 KiB to the frame of every function
that moves an `Executor`. That coupling is invisible at the knob: nothing in
`NROS_EXECUTOR_MAX_CBS`'s help says it grows the main thread's stack, and the
failure it produces is a stack overflow in a function the knob does not name.
Same shape as `NROS_ZEPHYR_TASK_STACK_SIZE` inheriting `MAIN_STACK_SIZE`.

## The fix, and one that does NOT work

**Rejected: boxing the table.** Tried and reverted. `alloc` is optional in
`nros-node` (`#[cfg(feature = "alloc")] extern crate alloc`), and the `params`
field that looks like a precedent is behind `param-services`. A `Box` compiles
on the std lane and breaks any `no_std` target without alloc -- which is most of
the targets this crate exists for. It also trades stack for heap on parts where
both are scarce.

**The fix is to finish phase-271:** move the remaining `MAX_CBS`- and
`MAX_NODES`-scaled members into the carved `backing` alongside the six tables
already there. That works with no allocator, puts the storage where the CALLER
chose (`.bss` for a static holder, and it is already sized by
`ExecutorSizing`), and removes the coupling rather than relocating it. It
touches `executor/storage.rs`'s `carve` + `ExecutorSizing`, which the C FFI
sizes `_opaque` from, so the generated sizes move with it -- deliberate, and
covered by the existing size-probe gates.

**Independently, STATE the number.** A boot-time check like the heap gate
(`nros: the executor arena cannot fit in the platform heap`) naming the required
main-thread stack would have turned four bring-up sessions into one build error.

## Also found

`ZenohSession::names_and_types_filtered` carries a 19840-byte frame -- larger
than `open_in`. It is not on the boot path, so it did not cause this, but any
image that calls graph introspection on a small part will hit the same wall.
