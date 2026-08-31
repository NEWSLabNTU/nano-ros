# Phase 405 — finish phase-271: every knob-scaled member leaves the `Executor` value

**Status (2026-08-31). Opened from issue 0936.** phase-271 externalised SIX
sized arrays and said so deliberately: "`Executor` keeps every field except the
six sized arrays". Nine knob-scaled members were left inline. On a 320 KiB part
that line turns out to be in the wrong place.

## What issue 0936 measured

`size_of::<Executor>()` is about 16 KiB, and the value is built on the stack and
moved twice:

```
16000  Executor::open_in     (sub.w sp, sp, #16000)
15104  nros_cpp_init         (holds the returned value, then ptr::writes it)
```

Roughly 31 KiB of prologue on one call chain, against a main thread that a
320 KiB part can give perhaps 32 KiB in total. `MAIN_STACK_SIZE` 8192, 16384 and
28672 all overflow; 32768 does not report an overflow and corrupts the idle
thread instead.

**The coupling that makes it a trap.** `group_sched_table` is scaled by
`MAX_CBS`, so raising the handle limit from 14 to 36 -- the fix for a completely
unrelated failure -- added about 3.7 KiB to the stack frame of every function
that moves an `Executor`. Nothing in `NROS_EXECUTOR_MAX_CBS`'s help says it costs
stack, and the overflow it produces names a function the knob never mentions.

## The nine members

| field | scaled by | note |
| --- | --- | --- |
| `extra_sessions` | `MAX_NODES` | `ConcreteSession` is 524 B here, so 6 of them is ~3.1 KiB |
| `group_sched_table` | **`MAX_CBS`** | ~168 B per slot; the `MAX_CBS` coupling above |
| `nodes` | `MAX_NODES` | `NodeRecord` ~300 B |
| `node_sched_table` | `MAX_NODES` | |
| `extra_session_ids` | `MAX_NODES` | `(String<32>, String<128>)` |
| `dispatch_slots` | `MAX_NODES` | |
| `component_slots` | `MAX_NODES` | |
| `active_groups` | `MAX_NODES` | already `Option`, so empty images pay less |
| `monitor_violations` | `MAX_VIOLATIONS` | |

## Approach

Carve them from the same `backing` the six tables already use. That is
phase-271's own mechanism (`executor/storage.rs`: `ExecutorSizing`, `carve`,
`executor_storage_layout`), extended rather than replaced.

**Not `Box`.** Tried in 0936 and reverted: `extern crate alloc` is feature-gated
in `nros-node` and the `params` field that looks like a precedent sits behind
`param-services`. A `Box` compiles on the std lane and breaks the `no_std`
targets this crate exists for, and it trades stack for heap on parts where both
are scarce.

**`heapless::Vec` becomes a slice plus a length**, the shape `entries` and
`remap_table` already use (`&'s mut [Option<T>]` with a `_len` counter where the
order matters). Push, iterate and overwrite are the only operations these
tables support today.

## What moves with it

`ExecutorSizing` gains the counts, so `executor_storage_layout` and every caller
of `carve` change together. The C FFI sizes `nros_executor_t::_opaque` from
`size_of::<ExecutorInlineStorage>()` via the build probe, so the generated sizes
move -- deliberately, and the existing size-probe gates cover it. Expect
`NROS_EXECUTOR_SIZE` to stay roughly constant (storage relocates, it does not
vanish) while `size_of::<Executor>()` drops from ~16 KiB to the low hundreds.

## Acceptance

1. `size_of::<Executor>()` no longer scales with `MAX_CBS` or `MAX_NODES`. A test
   pins it, the way issue 0900's `the_default_derivation_is_unchanged` pins the
   arena.
2. `Executor::open_in`'s stack frame, read from a linked image with `objdump`,
   drops by roughly the same amount. Compile-tier green is not evidence -- this
   phase inherits phase-403's measurement rule.
3. The island entry on `mr_canhubk3/s32k344` boots with a main stack it can
   afford. That image is the reason this phase exists.
