---
id: 458
title: "`nros_cpp_executor_open_over_session` never stamps the `CppContext` handle tag, so every C/C++ multi-tier spawned tier fails setup with -3 and never runs"
status: resolved  # fixed 2026-08-06
type: bug
area: api
related: [issue-0436, issue-0387, issue-0290, issue-0447, phase-274, rfc-0015]
---

## Symptom

`realtime_tiers_e2e`, three native cells, on freshly built fixtures:

```
native/c:          low-tier /telem never reached 5 deliveries — the low tier was not scheduled
native/cpp:        low-tier /telem never reached 5 deliveries — the low tier was not scheduled
native/cpp-rclcpp: low-tier /telem never reached 5 deliveries — the low tier was not scheduled
```

Running the C entry by hand against a router says it outright:

```
nros: multi-tier run — 2 tier(s) over one session
nros: tier 'low' setup FAILED (rc=-3) — tier will not run
[ctrl] tick=0
[ctrl] tick=1
...
```

Observers confirm the split: `ctrl Received: 490`, `telem Received: 0`. The high
tier (boot thread) runs; the spawned tier dies during setup.

## Root cause

`CppContext` gained a `tag` field in issue 0436 — a handle type tag read
*before* the struct is trusted, so a `void*` mix-up between it and
`nros-bridge`'s `ExecutorBox` becomes a clean error instead of memory
corruption:

```rust
pub(crate) unsafe fn cpp_ctx_checked<'a>(handle: *mut c_void) -> Option<&'a mut CppContext> {
    if handle.is_null() { return None; }
    let ctx = handle as *mut CppContext;
    // Read ONLY the tag until it is proven to be ours.
    if unsafe { core::ptr::read(core::ptr::addr_of!((*ctx).tag)) } != CPP_CONTEXT_TAG {
        return None;
    }
    Some(unsafe { &mut *ctx })
}
```

There are THREE constructors that write a `CppContext` into caller-provided
`MaybeUninit` storage. 0436 stamped the tag in two of them —
`nros_cpp_init` and `nros_cpp_init_multi` — and missed the third,
`nros_cpp_executor_open_over_session`, which is the one the per-tier model uses
for every spawned tier (`nros_board_native_run_tiers`, RFC-0015 Model 1,
phase-274 W2).

The storage is `MaybeUninit`, so `tag` held garbage. Every entry point taking
that handle then failed the tag check: the generated per-tier setup calls
`nros_cpp_node_create`, which does

```rust
let Some(ctx) = (unsafe { cpp_ctx_checked(executor_handle) }) else {
    return NROS_CPP_RET_INVALID_ARGUMENT;   // -3
};
```

so setup returned -3, the tier was abandoned before its node existed, and
`/telem` never published.

The generated code was never at fault — it correctly emits one setup per tier
with only that tier's node:

```c
static int32_t __nros_entry_setup_tier_1(void* executor) {
    nros_cpp_ret_t nrc = nros_cpp_node_create(executor, "telem_node", "/", &__nros_node_1);
    if (nrc != NROS_CPP_RET_OK) return (int32_t)nrc;
    ...
}
```

## This is a recurring class, now twice

The fix site already carried a comment about the SAME defect one field over:

> Issue 0387 — the CppContext is `MaybeUninit`; the reentrancy guard
> `in_dispatch` (added by #0290) MUST be initialized here or `spin_once` reads
> uninitialized garbage as "already dispatching" ... That silently killed every
> borrowed-tier executor (C/C++ multi-tier entries), e.g. a low-tier `/telem`
> publisher that never ran.

Identical shape, identical victim, identical symptom — a new `CppContext` field
is initialized in `nros_cpp_init`/`_init_multi` and the third constructor is
forgotten. `in_dispatch` came from #0290 and was fixed as #0387; `tag` came
from #0436 and is fixed here.

## Fix

Stamp the tag in `nros_cpp_executor_open_over_session` alongside the other
fields, with a note naming all three constructors so the next added field does
not repeat this a third time.

Swept for other sites: `grep` for `addr_of_mut!((*ctx_ptr).executor)` and
`MaybeUninit::<CppContext>` finds exactly three writers (676, 837, 2191) and
three storage sites (938, 2374, 2448); the latter three all route through
`nros_cpp_init` or the now-fixed `open_over_session`. No further gaps.

## Prevention (not yet done)

The real gate is structural: a single `CppContext` initializer that every
constructor must call, so a new field cannot be half-initialized. Filed
separately rather than bolted on with this fix — see the note in
`docs/issues/README.md`.

## Notes

Found while triaging #0447. That issue's own cell (native/rust) turned out to be
a STALE FIXTURE and passes on a rebuild; these three C/C++ cells are a real bug
that the same run surfaced.
