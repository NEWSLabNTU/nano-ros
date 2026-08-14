---
id: 563
title: "The executor's inline storage is static but its CONSTRUCTION is a ~9.3 KB stack temporary, so every embedded main stack pays for a build step"
status: resolved
type: tech-debt
severity: medium
area: core, platform-zephyr
related: [issue-0552, issue-0196, phase-271]
---

## The measurement

Issue 0552 was a main-stack overflow on `mps2_an385`. Its fix raised
`CONFIG_MAIN_STACK_SIZE` to 131072. This issue is the reason that number has to
be large, separated out because the board conf is the wrong place to fix it.

The executor's inline storage is NOT the problem — the C/C++ entry template
declares it `static ::nros::Node __nros_node;`, so all 88192 bytes of
`NROS_EXECUTOR_SIZE` live in `.bss`. What costs stack is BUILDING it:

| what | measured |
| --- | --- |
| consumed by the time `nros_cpp_init` is entered | ~13.4 KB (`sp = 0x20075e88`, base `0x20075320`) |
| temporary cleared inside `Executor::assemble` | ~9.3 KB (`__aeabi_memclr4`, `r0 = 0x20072e7c`, `r1 = 0x200752dc`) |
| total the path needs | ~23 KB |

Verified from the other side: 65536 passes all three cells 3/3, 32768 does not
(the MPU guard reports `ZEPHYR FATAL ERROR 2: Stack overflow` naming `main`).

## Why it is shaped this way

`Executor::assemble` builds a `Self { … }` literal and returns it by value; the
chain is `assemble` -> `from_session_in` -> `open_in` -> `nros_cpp_init`, which
finally `core::ptr::write`s it into the caller's static context. Rust does not
guarantee RVO, so the object is materialised on the stack before being moved
into storage that was already reserved for it.

The irony is that this path exists to be **heap-free**: `nros_cpp_init` carves
the backing from a caller-owned buffer specifically so nothing is allocated, and
then spends a large stack temporary putting it there.

**Not established:** whether each frame in that chain materialises its own copy
(which would make the peak a multiple of the object) or whether the compiler
collapses them. One ~9.3 KB clear was observed; the multiplier was not measured.
Anyone fixing this should measure it rather than inherit the assumption.

## What a fix looks like

Construct into caller-supplied storage instead of returning by value: `assemble`
writing through a `*mut Executor` / `&mut MaybeUninit<Executor>` the caller
provides, with `open_in` and `from_session_in` threading that pointer. Then the
construction cost is a few frames rather than a copy of the object, and every
embedded board's main stack can come down.

## How to measure it

`cmake/zephyr/mps2-an385.conf` keeps `CONFIG_HW_STACK_PROTECTION=y`, so an
overflow is a named fatal error on the offending thread rather than silent
corruption. Lower `CONFIG_MAIN_STACK_SIZE`, rebuild the three cortex-m leaves,
and run `cargo nextest run -p nros-tests --test zephyr_cortex_m_qemu`. Today
32768 fails and 65536 passes; a correct fix should bring that threshold down
towards the session-open path's own ~13.4 KB.

## The sweep this did NOT do

Every other Zephyr conf still sets `CONFIG_MAIN_STACK_SIZE=16384` for
zenoh/xrce (`examples/zephyr/*/*/prj-{zenoh,xrce}.conf`); the cyclonedds ones
already carry **524288**, so this was paid for there too. Those leaves are
native_sim, which has no MPU and host-backed stacks, so the same overrun would
be silent rather than fatal. Whether it happens there was not established, and
that uncertainty is itself an argument for fixing the cause instead of the
numbers.


## RESOLVED 2026-08-15

`remap_table` was the seventh sized table. phase-271 moved six of them out of
`Executor` and into caller-owned carved storage and left this one inline, where
it grew to **6664 of the struct's 11632 bytes — 57%**. It now carves from the
same backing as `entries`/`sched_contexts`.

```text
size_of::<Executor>()   11632 -> 4992     (host, 64-bit)
```

No `ExecutorSizing` change, so no public API break and no entry re-sizing: the
region is a fixed `MAX_REMAPS` count, and `u64_len()` grows to cover it
automatically. On the 32-bit target the net effect on the C/C++ inline storage
is `88192 -> 88264`, i.e. **+72 bytes total** — the same ~6.6 KB simply moved
off the constructing thread's STACK and into the caller's static buffer, which
was the entire point.

### Measured, on the board that motivated it

At `CONFIG_MAIN_STACK_SIZE=32768`, which overflowed for every language before:

| cell | before | after |
| --- | --- | --- |
| C | overflow | **passes** |
| C++ | overflow | **passes** |
| Rust | overflow | still overflows |

Rust's entry path is deeper and still wants more than 32 KB, so the board conf
stays at 131072 — retuning it is issue 0552's business and would need its own
measurement per language. What is established is that the requirement dropped by
roughly the 6.6 KB the struct lost. The MPU guard 0552 enabled is what makes
this legible: the failing run now says
`ZEPHYR FATAL ERROR 2: Stack overflow ... Current thread: main` instead of
corrupting the idle thread.

Verified: `zephyr_cortex_m_qemu` 3/3 at 131072.

### Guard

`executor_stays_small_enough_to_construct_on_a_stack` asserts
`size_of::<Executor>() <= 6 KiB`. Deliberately loose — it catches a NEW multi-KB
inline table appearing, not small drift, and host `size_of` is not the target's.

### A second defect this exposed, fixed with it

The C/C++ `EXECUTOR_OPAQUE_U64S` is baked from a probe of
`size_of::<ExecutorInlineStorage>()`, but a build script re-runs only when its
OWN package changes — never when a dependency does. So changing `nros-node`'s
layout left that constant stale while `nros-c` recompiled against the new one,
and the const assert fired with a message naming neither the cause nor a working
remedy:

```text
error[E0080]: EXECUTOR_OPAQUE_U64S too small for Executor + backing —
increase NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_MAX_CBS
```

Both suggested knobs are unrelated; only `touch`ing the consumer's `build.rs`
cleared it. That is the issue-0196 shape and the same family as the sizes-header
mirror bugs (0088/0114/0122/0123/0245/0268). `nros-sizes-build` now emits
`cargo:rerun-if-changed` for every source in **rustc's own depfile** for the
probe rlib — exact, no hardcoded paths, and skipping the probe's own build
artifacts so it cannot arm itself. Verified by editing only `nros-node` and
watching the generated header regenerate with no `build.rs` touch.

### Not done

The `nodes` table (1448 bytes) is the next largest inline field and could move
the same way. Left alone: the measured problem is solved, and every carve change
costs a full embedded rebuild to verify.
