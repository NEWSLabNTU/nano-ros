---
id: 876
title: "`CONFIG_HEAP_MEM_POOL_SIZE=0` from phase-391 W3 makes the Zephyr c/talker
  unbuildable on native_sim — latent because no native_sim build dir has reconfigured since"
status: open
type: bug
area: platform-zephyr
related: [phase-391, issue-0875, issue-0805]
---

## Problem

`examples/zephyr/c/talker/prj-zenoh.conf` and `prj-xrce.conf` set

```
CONFIG_HEAP_MEM_POOL_SIZE=0
```

and Zephyr's native offloaded-socket driver, which every `native_sim` image with
networking compiles, asserts the opposite:

```
zephyr-workspace/zephyr/drivers/net/nsos_sockets.c:38
    BUILD_ASSERT(CONFIG_HEAP_MEM_POOL_SIZE > 0);
```

So the build fails at compile time:

```
zephyr/include/zephyr/toolchain/gcc.h:87:36: error: static assertion failed: ""
   87 | #define BUILD_ASSERT(EXPR, MSG...) _Static_assert((EXPR), "" MSG)
zephyr/drivers/net/nsos_sockets.c:38:1: note: in expansion of macro 'BUILD_ASSERT'
   38 | BUILD_ASSERT(CONFIG_HEAP_MEM_POOL_SIZE > 0);
```

The assert is unconditional in that file, and the talker's board overlay is
`boards/native_sim_native_64.conf`, so nothing narrows it away.

Only these two confs are affected. Across `examples/zephyr`, the values are 22 ×
`65536`, 18 × `4194304`, 12 × `131072`, and these 2 × `0`.

## Where it comes from

Commit `60b4e0c1e` — *"feat(phase-391 W3): Zephyr's funnel is rlsf-backed —
kheap 65,536 -> 1,024, measured A/B"* (2026-08-28). The change itself is sound
and its reasoning is recorded: once `nros_platform_alloc` reaches the rlsf arena
instead of `k_malloc`, the kernel heap can shrink to whatever Zephyr's own
`HEAP_MEM_POOL_ADD_SIZE_*` mechanism demands, so the conf drops to `0` and lets
that mechanism decide.

That is true on the board it was measured on. The commit states its arm
explicitly — **`build-c-talker-zenoh` (mps2/an385)** — and mps2/an385 has no
`nsos_sockets.c`, because native offloaded sockets are a `native_sim`
construction. The same conf file serves both boards.

So this is the ordinary shape of a config change validated on one coordinate and
shipped on a file that spans several. Worth stating plainly rather than as a
reproach: the A/B was careful, both arms were built from one tree via stash, and
the drift check ran. The measurement was of the right thing on the wrong axis.

## Why nothing caught it for a day

Every `native_sim` build dir in `zephyr-workspace/` predates the commit —
`build-c-talker-zenoh/zephyr/zephyr.exe` is dated 2026-08-27, the conf changed
2026-08-28 — and a west build dir does not reconfigure until something makes it.
Its `autoconf.h` therefore still carried the OLD value while the tracked conf
carried the new one:

```
build-c-talker-zenoh    #define CONFIG_HEAP_MEM_POOL_SIZE 0        <- reconfigured
build-c-listener-zenoh  #define CONFIG_HEAP_MEM_POOL_SIZE 65536    <- stale, still builds
```

This is the museum-binary class CLAUDE.md already names, in its configure-time
form rather than its link-time one: the artifact is not merely out of date, the
*configuration that produced it* is, so the tree reports green on a config that
no longer exists in it. The staleness probe watches build INPUTS; nobody was
watching whether a build dir's `autoconf.h` still matches the conf it was
generated from.

Surfaced by accident, while measuring something unrelated (issue 0875): deleting
`nros-rust/` in that build dir triggered the reconfigure that regenerated
`autoconf.h`.

## Fix

Not attempted here — the right value is a platform question, not a mechanical
one, and phase-391 W3 owns it. The options, and what each concedes:

1. **Board-scope the `0`.** Move it to the mps2/an385 overlay and leave
   `native_sim` at a working value. Smallest change; concedes that the rlsf
   saving does not reach native_sim, which is the tier-1 lane.
2. **Give native_sim the minimum `nsos_sockets.c` needs** rather than `0`. Keeps
   the shrink on both boards but requires knowing what that minimum is, and the
   assert does not say.
3. **Ask why the assert exists** — if `nsos_sockets.c`'s `k_malloc` use is itself
   reachable through the funnel, the answer might be upstream-shaped rather than
   conf-shaped.

Whatever is chosen, a native_sim talker build has to be part of the evidence,
because that is exactly what the original A/B did not cover.

## Reproduce

```
cd zephyr-workspace
rm -rf build-c-talker-zenoh/nros-rust     # forces the reconfigure
ninja -C build-c-talker-zenoh             # fails at nsos_sockets.c:38
```

A clean west build of the same leaf reaches it directly, without the wipe.

## Acceptance

- `examples/zephyr/c/talker` builds on `native_sim/native/64` with both
  `prj-zenoh.conf` and `prj-xrce.conf`.
- The phase-391 W3 heap numbers are re-stated per board, so the record says
  which board each figure came from.
- Something notices a build dir whose `autoconf.h` disagrees with the conf that
  generated it. Without that, the next config-only regression is equally
  invisible for equally long.
