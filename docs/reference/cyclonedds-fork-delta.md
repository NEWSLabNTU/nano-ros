# The cyclonedds fork: what it carries, and why

nano-ros builds Cyclone DDS from a fork
([NEWSLabNTU/cyclonedds](https://github.com/NEWSLabNTU/cyclonedds), branch
`nano-ros`) rather than from the ROS package. This page records exactly what
that fork adds, so the delta is a deliberate, reviewable thing rather than an
accumulation nobody can enumerate.

**The delta is carried on purpose. It is not tech debt awaiting an upstream PR** —
see "Why upstreaming does not retire it" below, which is the part that is easy to
get wrong.

## The pin tracks ROS, not upstream

| | version |
| --- | --- |
| fork pin (`third-party/dds/cyclonedds`) | **0.10.5** |
| `ros-humble-cyclonedds` | **0.10.5** |
| upstream `master` | 11.x |

The pin matches the cyclonedds **that ROS ships**, and must. A ROS node on the
host links the distro's Cyclone through `rmw_cyclonedds_cpp`; an embedded image
speaking a different Cyclone is the same drift class as issue 0609, where a
vendored zenoh pin fell behind the one `rmw_zenoh_cpp` actually used. Moving the
fork to upstream `master` would be a wire-and-ABI divergence from every ROS peer
we exist to talk to.

So the fork is rebased when **ROS's** cyclonedds moves (a distro migration), not
when upstream releases.

## What the fork adds

21 commits, 52 files, +2493/−137 against the 0.10.x boundary `5041f356`.
Seven groups.

Re-measured 2026-08-29 and it had drifted BOTH ways: the headline said 15
while the tables below listed 17, and `ae14b312` appeared in neither. A
count that disagrees with its own table is worse than no count — it reads
as the authoritative number. The two verification recipes at the bottom
are the check; run them rather than trusting this line.

### 1. ThreadX / NetX Duo port (6 commits) — a platform upstream does not have

| commit | subject |
| --- | --- |
| `902f7707` | ddsrt: add ThreadX NetX port |
| `56e6170a` | fix(threadx): stabilize Cyclone DDS runtime |
| `e8ce7315` | fix(ddsrt/threadx): multicast byte-order + multi-iovec datagram sendto |
| `5558c6ae` | fix(ddsrt/threadx): network byte order for parsed/interface addresses |
| `12b4af2c` | fix(ddsrt/threadx): join multicast with INADDR_ANY interface |
| `6eb92277` | fix(ddsrt/threadx): drop duplicate ddsrt_setsockreuse (use generic) |

Almost entirely **additive** — new `ddsrt/{sync,threads,sockets,time,heap,
ifaddrs,process}/threadx/` trees plus the `DDSRT_WITH_THREADX` selection arms.
Retired only if upstream accepts a ThreadX port, which nobody has offered.

### 2. Zephyr platform gaps (4 commits)

| commit | subject |
| --- | --- |
| `290152c0` | fix(zephyr): tolerate NSOS socket gaps |
| `1d794c0a` | ddsi_udp: Zephyr multicast join via `struct ip_mreqn` (issue 0231) |
| `4aa337b0` | q_sockwaitset: AF_UNIX socketpair self-pipe on native-IP-stack Zephyr |
| `ae14b312` | ddsrt: initialise the atomics mutexes at runtime on the Zephyr backend |

Upstream *does* have a Zephyr port (`ports/zephyr`, `WITH_ZEPHYR`); these are
places where it does not survive contact with Zephyr's native IP stack / NSOS.

### 3. Lock-pool scaling (2 commits) — the pair this issue was opened about

| commit | subject |
| --- | --- |
| `a09babf3` | ddsrt: Zephyr-native sync backend — `k_mutex`/`k_condvar` instead of pooled pthreads |
| `942dda3c` | ddsi/addrset: stripe the lock instead of one mutex per addrset |

The substantive ones. Zephyr's POSIX `pthread_mutex_t`/`pthread_cond_t` are
handles into **fixed static pools** (`CONFIG_MAX_PTHREAD_MUTEX_COUNT` and
friends). Cyclone puts a mutex in every entity — one per addrset, three per
writer — and the count scales with the **remote** graph (proxy entities, SEDP
announcements), not just what this node creates. That makes "how large a ROS
graph can this board join" a compile-time RAM constant.

Measured (issue 0371): a 33-node Autoware graph exhausted 16384 slots ~19 s in
and died as an anonymous `abort()`; clearing it needed 131072 slots ≈ 4.1 MiB
static. Raising the Kconfig knob is the stock workaround and is a bad trade — the
RAM cost is proportional to a worst case you cannot know at build time, and
getting it wrong fails deep inside discovery with nothing to point at.

The two changes attack the term from different sides and **neither alone was
enough**: with striping only, 2048 slots still exhausted on the remaining
per-proxy-entity locks. The native backend removes ddsrt sync from the pool
entirely; striping stops the addrset term scaling at all.

Both carry a behavioural asymmetry worth remembering: **`k_mutex` is recursive
where a POSIX NORMAL mutex deadlocks**, so a self-relock bug hangs on Linux and
passes on Zephyr. See also `docs/reference/platform-implementation-notes.md`.

### 4. Diagnostics for the failure above (3 commits)

| commit | subject |
| --- | --- |
| `4c8ff8c2` | ddsrt/posix: fail loudly when `pthread_mutex_init` fails |
| `5b87ee52` | fix(ddsrt/posix): say which pool ran out, and fix cond/rwlock too |
| `8601ca66` | ddsrt: the freertos/threadx sync ports say what failed before aborting |

Stock Cyclone ignores the return of `pthread_mutex_init`. That is defensible on a
desktop and useless here: it converts pool exhaustion into an anonymous `abort()`
seconds later, which is exactly how 0371 cost as much time as it did.

### 5. FreeRTOS (2 commits)

| commit | subject |
| --- | --- |
| `22150fbf` | fix(freertos): avoid TLS-only DDS state |
| `99cfac88` | ddsrt: give every FreeRTOS thread its lwIP per-thread netconn semaphore |

With `LWIP_NETCONN_SEM_PER_THREAD=1` every thread touching the socket API needs
its own netconn semaphore. ddsrt creates its own threads and never called
`lwip_socket_thread_init()`, so the first socket call from one asserted
`sem != NULL`. `thread_start_routine` is the point they have in common. Found by
phase-370 W4, the first work to boot an embedded Cyclone image at all.

### 6. The platform allocation funnel (3 commits)

| commit | subject |
| --- | --- |
| `6e2ad36f` | ddsrt/posix: route the heap through nano-ros's platform allocation funnel |
| `8e6ff48a` | ddsi: allocate and free through ddsrt, not libc, in four mismatched places |
| `d97a71e2` | ddsrt: one funnel heap for every port, instead of an arm inside one of them |

Behind `-DNROS_DDSRT_PLATFORM_FUNNEL`, set by `ProvideCycloneDDS.cmake` on
`ddsc` only (never on the tools-side `ddsrt-internal`: idlc and confgen link no
platform layer). Undefined, the file is byte-identical to stock, so cyclone's
own ctest suite is unaffected. Issue 0832 measured the funnel DEFINED and
UNREFERENCED in native cyclone images; it now has 4 inbound edges and the whole
`ddsrt_{malloc,malloc_s,calloc_s,realloc_s,free}` family is off `malloc@plt`.
Compile-time rather than weak-linked on purpose: weak linkage leaves the libc
branch in the binary, and its absence is what a tier gate has to read.

Routing `ddsrt` was not enough, because four ddsi sites called libc DIRECTLY —
and three of them were already a bug before the funnel existed. They allocate
from one heap and release to the other: a listelem `malloc`'d in
`network_interface_find_or_append` but freed by `free_all_elements`'
`ddsrt_free`; `split_at_comma`'s `ddsrt_malloc`'d array released with libc
`free`; a `calloc`'d `->verbatim` released through `dds_stream_free_sample`,
whose allocator is `{ddsrt_malloc, ddsrt_realloc, ddsrt_free}`; and a
`ddsrt_asprintf` string freed with `free`. On POSIX both heaps are glibc so
nothing faults; on ThreadX, FreeRTOS and Zephyr — and under the funnel — they
are genuinely different heaps.

`sysdeps.c`'s `free` stays libc on purpose (`backtrace_symbols` allocated it),
and `q_freelist.c`'s `free` is a function PARAMETER, not libc. After the sweep
`libddsc.a` has exactly one object referencing a raw libc allocator, which is
the `sysdeps.c` one.

**The funnel is one file, not an arm per port.** `6e2ad36f` put the arms inside
`heap/posix/heap.c`, which left FreeRTOS on `pvPortMalloc` and ThreadX on
`tx_byte_allocate` — a second allocation route on exactly the ports whose heap
genuinely is not libc's. `d97a71e2` replaces that with `heap/nros/heap.c`,
which implements the whole `ddsrt_*` family on the platform ABI, and compiles
each port's own heap.c out under the same switch. `heap/posix/heap.c` is stock
again apart from that guard.

The new file sits in the COMMON source list rather than being swapped in per
port, and that is load-bearing: `ddsrt` is INTERFACE and `ddsrt-internal`
compiles the same `INTERFACE_SOURCES`, so a swap would hand the funnel to the
host tools (idlc, confgen) that link no platform layer. Because the switch is a
PRIVATE compile definition on `ddsc`, both targets see one file set and only
`ddsc` gets the funnel — measured: `libddsrt-internal.a` has zero
`nros_platform_*` references and its stock posix heap still defines the family.

Dropping the per-port heaps also drops their size-header prefix and alignment
arithmetic, which existed only because the RTOS allocators have no realloc;
`nros_platform_realloc` is specified with libc realloc semantics. The ThreadX
weak `zpico_threadx_byte_pool` goes with it.

`heap/vxworks/heap.c` is untouched: no CMakeLists references it, it does not
include `dds/ddsrt/heap.h`, and nothing in this tree can compile it.

### 7. Nested-build path resolution (1 commit)

| commit | subject |
| --- | --- |
| `556f79d4` | build: resolve Cyclone's own paths by project, not by CMAKE_SOURCE_DIR |

`CMAKE_SOURCE_DIR` is the TOP-LEVEL project's source dir, so a reference to a
Cyclone file spelled that way is correct only when Cyclone is the top-level
project. We reach it by `add_subdirectory()`, where it names nano-ros's root.

Latent until section 6 made it reachable: `_confgen`'s hash check watches
`ddsi_config.c`, so the first edit to that file arms the regeneration branch and
its four `AppendHashScript.cmake` commands look for the script under nano-ros.
The failure is not clean — `_confgen-exe` has already rewritten `defconfig.c`,
`options.md`, `cyclonedds.rnc` and `cyclonedds.xsd` in the SOURCE tree, and the
step that dies is the one appending the hashes, so a nested build leaves four
generated files modified and hash-less and the next configure re-arms the same
branch. This is what blocked routing `ddsi_config_init` through the funnel.

`project(CycloneDDS ...)` defines `CycloneDDS_{SOURCE,BINARY}_DIR`, which mean
"Cyclone's root" regardless of who included it. Identical to the old spelling in
a standalone build, so upstream behaviour is unchanged and cyclone's own ctest
suite is unaffected. Fixed at all nine sites that mean Cyclone's own tree
(`_confgen`, `idlc/xtests`, `src/idl`'s `MAIN_PROJECT_DIR`, `fuzz`), not only
the one that breaks a build today. `core/xtests/cdrtest` keeps its
`CMAKE_SOURCE_DIR`: its scripts live in that test's own directory, so it is
wrong standalone too — a different bug in a target this change cannot exercise.


Pushed to `origin/nano-ros` as a fast-forward over `8601ca66`, and the
superproject pin bumped to it afterwards — that order, not the reverse: a pin
naming an unpushed commit clones as an unfetchable ref.

## Why upstreaming does not retire it

Upstream accepts patches against `master` (11.x). nano-ros consumes 0.10.5 and
will keep doing so until a ROS distro migration. **A merged upstream PR therefore
changes nothing about this delta for years.** Upstreaming is worth doing for
future-proofing and for the community, but it is not the exit, and treating it as
one produces work whose verification does not even transfer (a green ctest on
11.x says nothing about the 0.10.5 code that actually ships).

Three prep branches exist locally in the submodule, unpushed (fork remotes are
maintainer-gated):

| branch | base | verified |
| --- | --- | --- |
| `upstream-zephyr-sync` | fork's `master` mirror | wiring verified at configure time; code identical to shipped |
| `upstream-addrset-stripe` | fork's `master` mirror | builds clean, ctest 1498/1498 |
| `nano-ros-addrset-hash` | fork pin `8601ca66` | builds clean, ctest 1282/1282 |

The first two need an eclipse-cyclonedds remote added and a rebase off real
upstream before they are PR-able; the fork's `master` mirror lags.

`nano-ros-addrset-hash` is the one with near-term value: it makes the stripe hash
independent of the allocator. The shipped hash divides the address by
`2 * sizeof (void *)`, which distributes well only when the allocator's chunk
spacing for that exact struct size is coprime with the stripe count — measured
with glibc, all 64 stripes at sizeof 40/64/96/128 but **16 of 64 at 48**.
`sizeof (struct addrset)` is 40 today, one added field from the bad case, and the
allocator that matters is picolibc's on Zephyr, which nobody measured.

## Re-deriving the delta

Three separate mechanisms have made this fork *look* like it carries commits
nobody can fetch. All three were wrong, and each cost real time. Start here:

```sh
cd third-party/dds/cyclonedds
git remote prune origin        # stale remote-tracking refs (branches that moved)
git fetch --unshallow origin   # the default checkout is SHALLOW: 8 commits, not 2375
```

Then, two recipes that cross-check each other:

```sh
# by boundary: 5041f356 is the newest upstream 0.10.x commit the stack sits on
git log --oneline 5041f356..origin/nano-ros          # -> 21

# by authorship, as an independent check
git log --oneline --author=jerry73204 origin/master..origin/nano-ros   # -> 21
```

Do not count `origin/master..origin/nano-ros` alone: it sweeps in upstream 0.10.x
release commits (version bumps, iceoryx fixes, deserializer patches) and reports
58.

**`origin` is the NEWSLabNTU fork**, carrying only `master` and `nano-ros`. It is
not eclipse-cyclonedds, and any `releases/*` ref you remember seeing is a stale
remote-tracking entry.

## Testing the fork

`ctest` on a host build of the fork is 1282/1282, **excluding** the
iceoryx/shm tests, which abort with `POSH__RUNTIME_NO_WRITABLE_SHM_SEGMENT`
regardless of any nano-ros change — confirm against a baseline build before
attributing one of those to your work. Note the 0.10.5 tree names them
`iceoryx_*` and `shm_*`, not `psmx_*_iox` as 11.x does, so a `-E iox` filter
silently misses them.
