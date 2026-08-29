---
id: 876
title: "`CONFIG_HEAP_MEM_POOL_SIZE=0` from phase-391 W3 makes the Zephyr c/talker
  unbuildable on native_sim — latent because no native_sim build dir has reconfigured since"
status: resolved
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

The assert is unconditional in that file, and nothing narrows it away. Note the
leaf's own `boards/native_sim_native_64.conf` is NOT the overlay that applies —
it is never merged (see the merge list below); the NSOS settings that reach the
build come from the shared `cmake/zephyr/native-sim-line-3.7.conf`, which
duplicates its content. 18 such per-leaf board confs exist under
`examples/zephyr/**/boards/` and are dead files. Not this issue's to fix, but
they are the first place someone will try to put a fix.

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

## The 0 was never live on the board it was measured for

Established while fixing this, and it changes the remedy.

`cmake/zephyr/mps2-an385.conf` is appended LAST to every `mps2_an385` fixture
row's `CONF_FILE`, after the leaf's own `prj-*.conf`, and line 104 of it reads:

```
CONFIG_HEAP_MEM_POOL_SIZE=131072
```

Zephyr merges Kconfig fragments last-wins, so on mps2 the leaf's `0` was
overridden before it could take effect. The merge list a build actually
reports — this is `build-c-talker-zenoh`, and the mps2 row's is the same shape
with `mps2-an385.conf` in place of the native_sim fragment:

```
Merged configuration '…/examples/zephyr/c/talker/prj.conf'
Merged configuration '…/examples/zephyr/c/talker/prj-zenoh.conf'
Merged configuration '…/cmake/zephyr/native-sim-line-3.7.conf'
Merged configuration '…/zephyr/misc/generated/extra_kconfig_options.conf'
```

So the `=0` had **exactly one live effect across the whole fixture set**, and
that effect was this bug. On its intended board it changed nothing; on the board
nobody checked it stopped the build.

The W3 commit's measurement (`kheap__system_heap 65,536 -> 1,024`) is therefore
of a configuration the fixture set does not build — a hand-run west build
without `cmake/zephyr/mps2-an385.conf`. The A/B was real and internally
consistent; it just measured a different image than the one CI produces. Worth
separating from the bug: the arena work in W3 is unaffected, only the claim
about the kernel heap shrinking is.

## Fix

Revert the `0` in both confs. Nothing more, because nothing more was ever
happening: `prj-zenoh.conf` and `prj-xrce.conf` go back to `65536`, matching the
sibling `examples/zephyr/c/listener` which runs the same driver on the same
board.

A first attempt moved the `0` into a per-leaf `prj-no-kernel-heap.conf` appended
by the mps2 fixture row, to keep the saving where it was valid. That was wrong
for the same reason the original was: `mps2-an385.conf` still merges after it.
Recorded because the mistake is the natural one — a fragment cannot express
"unless a later fragment disagrees", and every fix here has to be checked
against the *whole* merge list rather than the file being edited.

**Left for phase-391 W3**, not done here:

- If the kernel heap should shrink on mps2, the argument belongs in
  `cmake/zephyr/mps2-an385.conf`, where it applies to *every* mps2 leaf —
  including the rust talker, whose own conf asks for `131072` and which was
  never part of W3's A/B. That is a broader claim than the one W3 made, so it
  needs its own measurement.
- The W3 numbers should be re-stated against a fixture-set build, or marked as
  measured on a hand-configured image.

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
