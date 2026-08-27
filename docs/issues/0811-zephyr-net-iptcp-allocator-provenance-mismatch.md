---
id: 811
title: "`ep->iptcp` is allocated by two different allocators and always freed by
  one of them"
status: open
type: bug
area: platform
related: [issue-0817, phase-391]
---

> **Fix landed; this file is still `status: open` only because flipping it
> requires the `git mv` to `archived/` plus the `docs/issues/README.md` row move
> (`check-issue-index.sh` fails on a `status: resolved` file left in
> `docs/issues/`), and the fixing session could not stage. See
> [Resolution](#resolution).**

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

## Resolution

### Reachability: established, and it is worse than the issue assumed

The caller is vendored zenoh-pico, and the sequence is mandatory, not
hypothetical. `_z_link_clear` (`src/link/link.c:179-186`) runs `_close_f` then
`_free_f`, which for a multicast UDP link are:

```c
void _z_f_link_close_udp_multicast(_z_link_t *self) {      /* link/multicast/udp.c:155 */
    _z_close_udp_multicast(&self->_socket._udp._sock, &self->_socket._udp._msock,
                           self->_socket._udp._rep, self->_socket._udp._lep);
}
void _z_f_link_free_udp_multicast(_z_link_t *self) {       /* link/multicast/udp.c:160 */
    _z_free_endpoint_udp(&self->_socket._udp._lep);        /* <-- the LOCAL endpoint */
    _z_free_endpoint_udp(&self->_socket._udp._rep);
}
```

`packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c` maps those onto
`nros_platform_udp_mcast_close` (line 574) and `nros_platform_udp_free_endpoint`
(line 512). So the locally built endpoint reached `..._free_endpoint`
**every time a multicast link was torn down**, and because `mcast_close` had
already released the node, that call was a *use-after-free of a
cross-allocator pointer*, not merely a cross-allocator free. The issue's table
was right about the split and wrong about which direction was safe.

`mcast_close` cannot defend itself: the alias passes `lep` **by value**
(`nros_zp_alias_endpoint_t lep`), so nulling `lep->iptcp` there cannot reach the
caller's `_lep`. Ownership had to move, which is what landed.

Upstream zenoh-pico has the same shape and gets away with it on glibc only
because `z_malloc == malloc` and `freeaddrinfo` is a `free` walk there
(`src/system/unix/network.c:427,786`).

### The mismatch is real on every port, not "survivable by accident"

The issue's premise — "before the funnel fix both routes bottomed out in
`k_malloc`" — does not hold on any port:

| port | `getaddrinfo` side frees with | local endpoint allocated by |
| --- | --- | --- |
| zephyr | `zsock_freeaddrinfo` → **picolibc `free`** (`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`), or a vendor hook under `CONFIG_NET_SOCKETS_OFFLOAD` | `nros_platform_alloc` → `k_malloc` (`_system_heap`) |
| threadx | `nx_bsd_freeaddrinfo` → **`tx_block_release`** (NetX addrinfo BLOCK pool) | `nros_platform_alloc` → `tx_byte_allocate` (BYTE pool) |
| freertos | `lwip_freeaddrinfo` → **`memp_free(MEMP_NETDB)`** | was libc `malloc`/`calloc` (heap ≠ `pvPortMalloc` heap either) |
| esp-idf | `lwip_freeaddrinfo` → **`memp_free(MEMP_NETDB)`** | was libc `malloc`/`calloc` |
| posix | `freeaddrinfo` (libc, also frees `ai_addr`/`ai_canonname`/`ai_next`) | libc `malloc`/`calloc` |

Zephyr's `zsock_getaddrinfo` `calloc`s one AI_ARR_MAX array and
`zsock_freeaddrinfo` is a single `free()` — libc arena, never `k_malloc`. On
ThreadX the wrong free links byte-pool memory into a block pool's free list. On
the lwIP ports it pushes heap memory onto a fixed-element pool free list.

### The fix

One rule, one spelling, in all five ports (`packages/platform/nros-platform-{zephyr,threadx,freertos,esp-idf,posix}/src/net.c`):

1. **The locally built endpoint is platform-funnel memory everywhere.**
   `freertos`, `esp-idf` and `posix` moved off libc `malloc`/`calloc`/`free`
   onto `nros_platform_alloc`/`nros_platform_dealloc` (RFC-0034 D6). On
   `esp-idf` and `posix` the funnel *is* `malloc`/`free`, so that half is a
   rename; on `freertos` it moves the allocation to `pvPortMalloc`, which is
   the heap the rest of that port uses. `calloc` became
   `nros_platform_alloc` + an explicit `memset`.
2. **The node carries its provenance.** `mcast_open` sets
   `laddr->ai_canonname = &nros_local_endpoint_tag`, a file-static one-byte
   object. Pointer identity against a static address is unforgeable — no
   resolver can produce it — and `ai_canonname` is a field this tree never
   otherwise reads.
3. **`free_endpoint` dispatches on the tag**: funnel node →
   `nros_platform_dealloc(ai_addr)` + `nros_platform_dealloc(node)`; anything
   else → the socket layer's `freeaddrinfo`. Both TCP and UDP go through this
   one function (`..._udp_free_endpoint` forwards to `..._tcp_free_endpoint`).
4. **`mcast_close` no longer frees the local endpoint** — it takes `lep` by
   value and cannot suppress the caller's follow-up free, so the node is owned
   by `free_endpoint` alone, mirroring `create_endpoint`/`free_endpoint`. This
   is also what removes the double free. `_z_new_link_udp_multicast` memsets
   `_lep` to zero at link creation, so a link that never opened frees nothing.

Nothing outside those five files needed to change: the only callers of
`nros_platform_udp_mcast_{open,close}` are the zenoh-pico aliases, and
`nros-smoltcp`'s endpoints are inline values with a no-op `free_endpoint`.

### Class sweep

- Sibling **fields**: none — `nros_*_endpoint_t` has exactly one field, and the
  socket struct holds an `int fd`.
- Sibling **allocation sites in the same files**: the `lsockaddr` companion
  allocation in every `mcast_open` (and POSIX's `get_ip_from_iface`, shared with
  `mcast_listen`) had the same split; all of them moved to the funnel in the
  same commit, including POSIX's eight `free(lsockaddr)` error-path sites.
- Sibling **ports**: all five, fixed together (the issue named only zephyr).
- `packages/drivers/net/nsos-netx/src/nsos_netx.c` implements
  `nx_bsd_getaddrinfo`/`nx_bsd_freeaddrinfo` for the host-side ThreadX shim with
  `calloc`/`strdup` + `free` — internally consistent, no fix needed, but it is
  why the ThreadX file's comment names two possible `freeaddrinfo` backends.
- Not touched, deliberately: `nros-platform-{posix,esp-idf}/src/timer.c` still
  use `calloc`/`free` for their timer records. That is self-consistent
  (allocated and freed by the same allocator) and is a phase-391 funnel-
  unification item, not this bug.

Sweep command:

```sh
git grep -nE 'iptcp|ai_canonname' -- 'packages/platform/*/src/net.c'
```

Every `iptcp` free site must be inside a `..._free_endpoint`, and every
`lep->iptcp = laddr` must be preceded by the `ai_canonname` tag assignment.

### Not verified by a build

The fixing session ran in a parallel fan-out and was not allowed to compile or
test. Everything above is source-level. The compiler-visible risks are: the
removed `lep` local in each `mcast_close` (replaced by `(void) lep_raw;`), the
`CHAR`-vs-`char` spelling of the ThreadX tag, and the new `memset` in the three
ports that dropped `calloc`.
