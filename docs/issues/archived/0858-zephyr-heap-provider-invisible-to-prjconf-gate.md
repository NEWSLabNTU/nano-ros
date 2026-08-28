---
id: 858
title: "`zephyr_prjconf_meets_backend_requirements` is red: phase-391 W3 moved the
  heap to an rlsf arena the gate cannot see, because its size is a compile-time
  ENV var and never appears in the `.conf` the gate reads"
status: resolved
type: bug
area: testing
related: [phase-391]
---

## Problem

`just ci` fails in `check-source-gates`:

```
examples/zephyr/c/talker/prj-xrce.conf  (rmw=xrce):  CONFIG_HEAP_MEM_POOL_SIZE must be > 0, got Some("0")
examples/zephyr/c/talker/prj-zenoh.conf (rmw=zenoh): CONFIG_HEAP_MEM_POOL_SIZE must be > 0, got Some("0")
```

`60b4e0c1e` (phase-391 W3) set both to `0` deliberately: the allocation funnel is
now rlsf-backed in `nros-platform`'s `zephyr_heap` module and no longer calls
`k_malloc`, so the kernel system heap can shrink to what Zephyr's own `ADD_SIZE`
mechanism demands. The commit measured it A/B and the `.bss` accounting closes
exactly (arena 67,248 − kheap freed 64,512 = +2,736).

The requirement in `zephyr_prjconf_requirements.rs` still says every backend
needs `HEAP_MEM_POOL_SIZE > 0`, with `why` text naming `k_malloc`. So the gate
asserts a fact the tree no longer has.

## Why this is not a one-line fix

**The gate cannot see the new provider.** `NROS_ZEPHYR_HEAP_SIZE` is a
compile-time ENV var read by `option_env!` in `zephyr_heap.rs` — it is not a
Kconfig symbol, so it never appears in a `.conf` file, and the gate parses
`.conf` files. From the file the gate reads, a deliberate conversion and a
misconfiguration are the same three characters.

**The old rule is still TRUE almost everywhere.** W3 is a pilot: it converted
exactly two confs (`examples/zephyr/c/talker/prj-{zenoh,xrce}.conf`). Twenty-plus
other confs still carry a positive kernel heap, and the Cyclone ones carry
4 MiB because ddsrt genuinely allocates. Deleting the rule would stop catching
the footgun it exists for — the zenoh-pico `-80` case — everywhere it still
applies.

**The commit's evidence is per-image, not global.** It says "nothing else in
THIS image allocates from it, and the link/nm test is what proves that". That is
the right claim to make and it does not generalize to confs W3 has not measured.

So the two obvious moves are both wrong: relaxing the rule globally disables a
live gate, and reverting the two confs undoes a measured improvement.

## Direction

Make the provider VISIBLE in the file the gate reads, then check the invariant
that actually matters — *this image has a heap from some provider* — rather than
one provider's symbol:

* a real Kconfig symbol (say `CONFIG_NROS_PLATFORM_HEAP`) that the converted
  confs set and that genuinely gates the arena, so it is not a marker nothing
  reads; the gate then accepts `HEAP_MEM_POOL_SIZE > 0` OR that symbol;
* and `why` text updated to name both providers, since its whole job is to make
  the fix obvious from the failure.

Whether the arena should eventually become unconditional on Zephyr — which would
retire the kernel-heap requirement for good — is phase-391's call and wants the
per-image link/nm evidence W3 used, extended to the confs it has not converted.

Filed rather than fixed: choosing how the invariant is expressed is a design
decision inside an in-flight phase, and guessing it either re-breaks CI or
silently disables a gate that catches a real runtime failure.

## Resolution (2026-08-28) — fixed upstream, and by the other option

Fixed on main by dropping `Req::Positive("HEAP_MEM_POOL_SIZE")` from the zenoh
and xrce backends and leaving it on cyclonedds, with the reason recorded at the
requirement: *"Requiring pool > 0 here would force every image to carry BOTH
heaps — the exact state W3 removed."*

That is the second of the two moves this issue called wrong, and the issue was
wrong about it. My objection was that relaxing the rule stops catching the
footgun for the twenty-plus unconverted confs — but the footgun the gate exists
for is the zenoh-pico `-80` mutex-count case, and those requirements
(`MAX_PTHREAD_MUTEX_COUNT >= 8`, `MAX_PTHREAD_COND_COUNT >= 6`, `POSIX_API`) are
untouched. The heap requirement was a lower bound on a resource those confs may
simply carry without needing; dropping it loses no live check. Cyclone keeps
its heap requirement because ddsrt genuinely allocates from the kernel pool,
which is exactly the discrimination I claimed the gate could not make — it can,
per BACKEND, which is the axis the table was already organized on.

No Kconfig symbol was needed. This issue's "make the provider visible" direction
solved a problem that only exists if the requirement is per-IMAGE; it is
per-backend, and after W3 the answer for zenoh and xrce is the same for every
image regardless of what its `.conf` says.

Verified: `cargo test -p nros-tests --test zephyr_prjconf_requirements` passes.
