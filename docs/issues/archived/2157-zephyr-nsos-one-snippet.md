---
id: 2157
title: "native_sim's NSOS network was 56 copies, every leaf copy tied to Zephyr 3.7, and live on some build roads only"
status: resolved
resolved_in: "2026-10-30"
type: tech-debt
area: [zephyr, boards, examples]
severity: medium
found: 2026-10-29
related: [2140, 1782, 0876]
---

## What was measured

native_sim reaches the network through NSOS (Native Sim Offloaded Sockets: host
BSD sockets) instead of the `zeth` TAP device, which needs root. Without NSOS
the image boots onto the TAP driver and segfaults (`eth_posix: Cannot create
zeth` — issue 2140). Before this, the five lines that select it were stated in
**56 places**, in three different shapes:

| Where | Count | Problem |
|---|---|---|
| `cmake/zephyr/native-sim-line-{3.7,4.4}.conf` | 2 | correct per line; reached only by scripts that knew the path |
| `zephyr/native-sim-nsos.conf` | 1 | the 3.7 fixture lane's copy; no `SLOWDOWN` line, unlike the others |
| `examples/**/boards/native_sim_native_64.conf` | 32 | named `CONFIG_ETH_NATIVE_POSIX=n`, a symbol only 3.7 defines |
| the NSOS tail of `prj-cyclonedds.conf` | 21 | a BOARD fact inside an RMW fragment |

Two defects followed from the shape, not from any one copy:

1. **Road-dependent.** Zephyr merges a leaf's `boards/<board>.conf` only when
   `CONF_FILE` is UNSET (`configuration_files.cmake:39`). The fixture lane sets
   it, so there the 32 leaf copies were dead and the lane appended a shared
   overlay by path; `nros build` and a bare `west build` leave it unset, so
   there the leaf copy was the only NSOS source. CLAUDE.md said the leaf copies
   were "NEVER merged", which was true of one road.
2. **Line-dependent.** 4.4 renamed the driver `ETH_NATIVE_TAP`, and Zephyr turns
   an assignment to an undefined symbol in a handwritten fragment into an error
   (`kconfig.py`, `warn_assign_undef`). So every leaf copy would refuse a 4.4
   `nros build` (read from the source, not built: no 4.4 workspace here).

The customer cases this had to serve, and how the per-line layout first proposed
fared against them: a vendor Zephyr fork with a version like `3.7.99` matches no
per-line directory; a future line needs a new directory before anything builds;
`nros build --zephyr-workspace <4.4 tree>` picks the line from
`NROS_ZEPHYR_VERSION`, whose default is 3.7, so it picks the wrong file; a
customer's own native board (32-bit native_sim, a vendor simulator) had to copy
the file again; and a board package outside this checkout can reach a file only
by a path into nano-ros.

## Resolution

**One snippet, named by the board.**

- `zephyr/snippets/nros-nsos/` — `snippet.yml` + `nsos.conf`. The conf is
  line-agnostic: both 3.7 and 4.4 enable the TAP driver only as
  `default y if NET_L2_ETHERNET && ETH_DRIVER`, and `ETH_DRIVER` exists on both,
  so `CONFIG_ETH_DRIVER=n` removes it without naming it. Snippet-wide, with no
  `boards:` regex: on a non-native board `NET_NATIVE_OFFLOADED_SOCKETS` cannot be
  enabled (`depends on ARCH_POSIX`) and Kconfig refuses the build — measured on
  `mps2/an385` — where a regex would make it a silent no-op and would miss a
  vendor's native board under another name.
- `[board.zephyr] snippets = ["nros-nsos"]` on the generic `zephyr` board — a
  NAME, so any board package (in-tree, `$NROS_EXTRA_BOARD_PATH`, or a workspace
  `src/<board>/`) reaches it with no path.
- Two carriers into `SNIPPET`, both merged by the module hook
  (`_nros_image_kconfig_merge_snippets`, `zephyr/cmake/nros_image_kconfig.cmake`),
  which runs before Zephyr's `snippets.cmake`, so a name added there applies
  exactly as `-S` does, with or without `CONF_FILE`:
  - `nros ws leaf-system` prints `NROS_LEAF_SNIPPET=` for every rendered image
    (a single-package leaf and a generated workspace app) — this reaches the
    fixture lane and a bare `west build` too;
  - `nros build` passes `-DNROS_BOARD_SNIPPETS=` on every west image, for a
    HAND-WRITTEN entry (`nros new entry`), which the hook does not render. Our
    own cache variable, never `-DSNIPPET`, which would replace a user's `-S`.
- `leaf-system` now loads the board catalog WITH the enclosing workspace's
  packages (`load_with_packages`), so a customer's workspace board package
  resolves there as it does for `nros build`.
- `_nros_image_kconfig_check_snippets` matched every `^nros-(.+)$` snippet as an
  RMW choice and refused `nros-nsos` as "selects NSOS"; it now matches the three
  RMW names only.
- Deleted: the 3 shared overlays, the 32 leaf copies, the 21 cyclone tails. The
  fixture lane, `just zephyr build-one`, `check-copy-out` and `nros new entry`
  stop supplying NSOS themselves. Docs (book, guides, READMEs) pass
  `-S nros-nsos` for a bare `west build`.
- Gate `check-nsos-single-source`: no tracked `*.conf` outside the snippet
  enables NSOS or names a TAP-driver symbol (absolute, self-tested).
- Corrected on the way: the "snippets are Zephyr 4.x only" comments in
  `zephyr/module.yml` and all three RMW snippets. 3.7 ships `snippets.py`, the
  module `snippet_root` key and board-scoped snippets; `-S` was measured merging
  on 3.7 with `CONF_FILE` set.

## Measured

- Zephyr 3.7, a copy of `examples/templates/zephyr-byo/app` with NO board conf,
  `nros build`: `Merged configuration '.../zephyr/snippets/nros-nsos/nsos.conf'`,
  `.config` has `# CONFIG_ETH_DRIVER is not set` and no TAP symbol, and the image
  published 11 messages to `rmw_zenohd` on 7447.
- Zephyr 3.7, bare `west build -S <snippet> -DCONF_FILE=prj.conf` (the fixture
  shape): merged on `native_sim/native/64`; refused on `mps2/an385` naming
  `ARCH_POSIX (=n)`.
- Fixture rows rebuilt through the lane — see the PR.

## Not measured / left

- **4.4 was read, not built** (v4.4.0 `Kconfig.defconfig`, `Kconfig.native_tap`,
  `drivers/ethernet/Kconfig`, `drivers/net/Kconfig`, `snippets.cmake`). One
  native_sim build on 4.4 settles it.
- **Timing change.** The 3.7 fixture lane's overlay and the `{c,cpp,mixed}`
  bringup copies lacked `NATIVE_SIM_SLOWDOWN_TO_REAL_TIME`; those images now
  run real-time-paced like the other 29.
- `multi_pkg_workspace_zephyr/zephyr_app` (`west_bringup_zephyr_cyclone_user_config`)
  is a raw `west build` of a guest app, so it no longer gets NSOS; it is a
  build-only config-passthrough check whose zenoh twin never had it.
- bsim boards (`nrf52_bsim`) also run on the native simulator; whether NSOS
  applies there is unmeasured.
