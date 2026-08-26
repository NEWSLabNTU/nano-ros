---
id: 811
title: "`ep->iptcp` is allocated by two different allocators and always freed by
  one of them"
status: open
type: bug
area: platform
related: [issue-0817, phase-391]
---

## Problem

In `packages/platform/nros-platform-zephyr/src/net.c`, the endpoint field
`ep->iptcp` has **two provenances**:

| site | allocator |
| --- | --- |
| net.c:84, net.c:190 | `zsock_getaddrinfo()` — Zephyr's addrinfo pool |
| net.c:354 (`lep->iptcp = laddr`) | `nros_platform_alloc()` — the platform funnel |

And exactly one free path is unconditional:

```c
void nros_platform_tcp_free_endpoint(void *ep_raw) {
    ...
    if (ep->iptcp != NULL) {
        zsock_freeaddrinfo(ep->iptcp);      /* net.c:91 */
```

The local-endpoint path built at net.c:336-354 IS freed correctly, by
`nros_platform_dealloc` at net.c:444-445. The hazard is the other direction: a
locally-built endpoint reaching `nros_platform_tcp_free_endpoint` would hand
funnel memory to `zsock_freeaddrinfo`, which returns it to Zephyr's addrinfo
slab.

## What is NOT established

**Reachability.** Both are `nros_zephyr_endpoint_t`, so the type system does
not separate them, but no call path has been traced that carries a
locally-built endpoint into `nros_platform_tcp_free_endpoint`. This issue
records the mismatch, not a reproduction. Whoever fixes it should establish
reachability first — if it is unreachable, the fix is a type-level split so it
stays unreachable, not a free-path change.

## Why it matters more after issue 0817

Before the funnel fix both routes bottomed out in `k_malloc`, so the mismatch
was *survivable by accident* on Zephyr. It stops being survivable the moment
`nros_platform_alloc` is repointed at a different arena, which is exactly what
[phase 391](../roadmap/phase-391-allocation-unification-and-tier-model.md)
proposes. Two allocators with one free path is a latent bug today and a real
one after the swap.
