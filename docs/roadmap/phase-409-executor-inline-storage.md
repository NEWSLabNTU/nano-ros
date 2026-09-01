# Phase 409 — finish phase-271: every knob-scaled member leaves the `Executor` value

**Status (2026-08-31). Opened from issue 0961.** phase-271 externalised SIX
sized arrays and said so deliberately: "`Executor` keeps every field except the
six sized arrays". Nine knob-scaled members were left inline. On a 320 KiB part
that line turns out to be in the wrong place.

## What issue 0961 measured

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

## Landed (2026-08-31). Acceptance 1 and 2 measured; 3 is the maintainer's.

All nine moved. The mechanism is one new private type in `executor/storage.rs`,
`CarvedVec<'s, T>` -- a `&'s mut [MaybeUninit<T>]` plus a fill cursor, with
`Deref<Target = [T]>`, a `push` that hands the value back when full, and a `Drop`
that drops its initialised prefix. `Deref` is why the ~90 use sites needed no
edit: `heapless::Vec` also derefs to `[T]`, so `.iter()`, `.get()`, `.len()`,
`[i]` and the public `Executor::nodes() -> &[NodeRecord]` accessor are unchanged
text. `MaybeUninit<T>` rather than `Option<T>` for that accessor, and because
`carve` then writes NOTHING for these tables at open -- the `__aeabi_memclr8` in
issue 0961's fault report is the executor's tables being zeroed.

`ExecutorSizing` gained ONE field, `nodes`: seven of the nine were scaled by
`MAX_NODES`, `group_sched_table` by `MAX_CBS` (already `sizing.cbs`), and
`monitor_violations` by the fixed `MAX_VIOLATIONS`, which stays a constant in the
layout for the reason issue 0563 gave for `MAX_REMAPS`. `executor_storage_layout`
and `executor_storage_u64_len` now take an `ExecutorSizing` rather than three
positional `usize`s, which is what stopped a four-`usize` signature.

Two behaviour details that are NOT free and were handled:

* `extra_sessions` holds `ConcreteSession`, which closes an RMW session on drop,
  and a carved element has no owner that would drop it. `CarvedVec` has its own
  `Drop` rather than a pass in `Executor::drop`, so the ORDER is preserved:
  `Executor`'s fields drop in declaration order and `extra_sessions` is declared
  after `session`, so the primary still closes before the extras. A pass in
  `Executor::drop` would have closed the extras first.
* `active_groups` was `Option<heapless::Vec<..>>`, where `None` is the wildcard
  and `Some(empty)` accepts nothing. The table alone cannot carry that, so the
  wildcard became a separate `active_groups_filtering: bool`, and
  `group_filter_accepts` now takes `Option<&[..]>`. Pinned by
  `an_empty_filter_is_not_the_wildcard`.

### Measured

`size_of::<Executor>()`, `cargo test -p nros-node --lib --features std`:

| knobs | before | after |
| --- | ---: | ---: |
| shipped defaults (`MAX_CBS` 4, `MAX_NODES` 4) | 5072 | 1016 |
| the island's (`MAX_CBS` 36, `MAX_NODES` 6) | 12768 | 1016 |

The same number at both, which is acceptance 1. Pinned by
`the_executor_value_does_not_scale_with_the_knobs`, a ceiling with named
allowances for the two large things in the header that the knobs do NOT size
(the primary session, held by value, and `scheduler-os-priority`'s worker pool);
and by `every_knob_scaled_table_is_charged_to_the_backing`, which asserts each
knob's per-slot width shows up in the BACKING's layout.

Stack frames, read with `objdump -d` from a LINKED image. The image is a scratch
probe, not a tracked file -- reproduce it with:

```
cargo build -p nros-cpp --no-default-features \
  --features "std,rmw-cffi,platform-posix,ros-humble"
# a C file calling nros_cpp_init + nros_executor_init, plus an empty
# `void nros_app_register_backends(void) {}` so the staticlib links
cc -O2 -o probe probe.c target/debug/libnros_cpp.a -lpthread -ldl -lm
objdump -d --demangle probe
```

A function's frame is the SUM of the `sub $imm,%rsp` in its prologue, not the
first one: LLVM splits a large x86_64 frame into 4096-byte stack-probe chunks
plus a remainder, so reading only the first `sub` reports 4096 for every large
frame. Bytes:

| function | defaults, before | defaults, after | island knobs, before | island knobs, after |
| --- | ---: | ---: | ---: | ---: |
| `Executor::open_in` | 9544 | 3368 | 18296 | 3368 |
| `nros_cpp_init` | 7992 | 1816 | 16744 | 1816 |
| `nros_executor_init` | 7808 | 1648 | 16560 | 1648 |
| `Executor::open_multi_in` | 10520 | 4376 | 19272 | 4376 |
| `nros_cpp_init_multi` | 9000 | 2840 | 17752 | 2840 |

x86_64, so the absolute numbers are not the Cortex-M ones the issue reports
(16000 / 15104); the RATIO is the claim. The knob column is the point: before,
`NROS_EXECUTOR_MAX_CBS=36 NROS_EXECUTOR_MAX_NODES=6` cost `open_in` another
8752 bytes of prologue; after, it costs nothing. `open_in` + `nros_cpp_init`
together fall from 17536 to 5184 bytes at the defaults, and from 35040 to 5184
at the island's.

Generated FFI sizes, same build: `NROS_EXECUTOR_SIZE` 89680 -> 89816 (+136),
`EXECUTOR_OPAQUE_U64S` 11210 -> 11227 (+17 words). Roughly constant, as
predicted -- the storage relocated from the header into the backing and picked up
alignment padding at the new region boundaries. Both are per-build values emitted
by the size probe, so nothing committed had to change. The committed NuttX
fallback snapshots (`nros_config_generated_nuttx.h`,
`nros_cpp_config_generated_nuttx.h`) are hand-maintained UPPER BOUNDS at 98296
and still cover the new value; they were deliberately left alone.

### Acceptance 3 -- MET on silicon (2026-09-01)

The board arrived after the section above was written. All three acceptance
criteria now hold, and 2 has been re-measured on the target rather than inferred
from an x86_64 ratio.

**Acceptance 2, on Cortex-M.** Frames read with `arm-zephyr-eabi-objdump -d` from
the linked island image (`build-z4rtt/zephyr/zephyr.elf`), against the numbers
issue 0961 reported for the same two functions before the carve:

| function | 0961, before | island image, after |
| --- | ---: | ---: |
| `Executor::open_in` | 16000 | 2244 |
| `nros_cpp_init` | 15104 | 1396 |

31104 bytes of prologue on that call chain becomes 3640. The x86_64 table above
predicted the ratio; this is the part number the phase was actually opened for.

A parser note, because it cost a wrong answer before it cost a right one: on
Thumb-2 the wide form is `subw sp, sp, #N`, which does NOT match a pattern
written for `sub.w`. A scan that misses it reports a frame of ZERO for a function
that has a real 2244-byte one, and zero reads as success. Match all three of
`sub`, `sub.w` and `subw`.

**Acceptance 3.** The island entry boots on `mr_canhubk3/s32k344` with
`CONFIG_MAIN_STACK_SIZE=16384`, half the 32768 that was the smallest workable
value before the carve, and every value in that board `.conf` is now traceable to
a failure the board produced. Recorded in the superproject at commit 8d586b2,
"feat(island): the configuration that boots, measured on silicon": four MRM nodes
and 22 topics visible from a stock ROS 2 Humble graph over zenoh-pico on serial.
Final image RAM 99.16%, DTCM 71.52%, FLASH 9.82%.

phase-271 is finished. Every knob-scaled member is charged to the backing, and
the value no longer scales with a knob at either the defaults or the island's.

### Residual, not this phase

`ZenohSession::names_and_types_filtered` measures 19876 bytes on the island image
(19840 on x86_64) and is still the largest frame there, larger than `open_in`
ever was. Not on the boot path; issue 0961 files it separately.

Two frames the carve did not touch, surfaced by the same scan and worth a look
before anyone enables actions on a small part:

| function | island image |
| --- | ---: |
| `Executor::register_action_server_raw` | 11764 |
| `Executor::register_action_client_raw` | 9036 |

The island sets `NROS_EXECUTOR_ACTION_CLIENTS=0` and registers no action entity,
so neither is on its boot path today. They are per-entity registration frames
rather than knob-scaled `Executor` members, so they are a different shape of
problem from the one this phase solved, and they should be measured before they
are assumed to be one.
