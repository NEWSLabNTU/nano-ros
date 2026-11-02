---
id: 2194
title: "A Zephyr board descriptor cannot state the zenoh pool or heap knobs its RAM needs, and the leaf road has no board knob rung"
status: resolved
type: limitation
severity: medium
area: [boards, zephyr, knobs]
related: [issue-1782, issue-0460, phase-524]
found: 2026-11-01
resolved_in: "issue 2194 PR (2026-11-02)"
---

## Symptom

Phase-524 W3 added `nucleo-f767zi-zephyr`
(`packages/boards/nros-board-nucleo-f767zi-zephyr/`). Its 384 KiB sram0 does
not fit the crate pool defaults. The W1 spike measured what each image needs
(`docs/roadmap/data/phase-524-w1-renode-nucleo.log`):

| image | knobs | RAM used |
|---|---|---|
| serial | `ZENOH_MAX_LARGE_SUBSCRIBERS = 0` | 89.5 % |
| ethernet | that, plus `ZENOH_MAX_SUBSCRIBERS = 1`, `ZENOH_SERVICE_BUFFER_SIZE = 1024`, `ZEPHYR_HEAP_SIZE = 32768` | 98.2 % |

These are board facts: one RAM, every image. RFC-0103 D6 says a board states
knobs in a top-level `[knobs.<group>]`. The descriptor cannot do that for any
of the four:

1. **Schema.** The descriptor's `[knobs]` is
   `nros_platform_config::platform_config::Knobs`. It has no field for
   `zenoh.max_large_subscribers`, `zenoh.max_subscribers`,
   `zenoh.service_buffer_size` or `zephyr.heap_size`. `ZenohKnobs` and
   `ZenohLimitKnobs` are `deny_unknown_fields`, and an unknown top-level group
   parses as a backend's `BackendKnobs`, which takes `tx` only. So writing
   any of them makes the descriptor fail to load. In the registry
   (`knobs.toml`) all four have `toml: None`: they have an image key and no
   board key.
2. **Delivery.** Even with a schema field, the Zephyr leaf road would not
   carry it. `nros ws leaf-system --kconfig-defaults-out` renders the
   IMAGE's knobs into `Kconfig.from-system-toml` and nothing else
   (`leaf_kconfig::defaults_text`). `nros_resolve_knobs()` in
   `zephyr/cmake/nros_cargo_build.cmake` has no board rung either.
3. **No conf-file way round.** The knobs have no Kconfig prompt, so a
   `CONFIG_NROS_MAX_LARGE_SUBSCRIBERS=0` line in the board's `prj.conf` is a
   configure error. `check-leaf-conf-nros-knobs` refuses one in a leaf conf
   anyway.

## Workaround

Each image on the board states the knob:

```toml
[image.<id>.knobs.zenoh]
max_large_subscribers = 0
```

The W3 build proof does this. The descriptor's header records the numbers,
and the ethernet heap's shortfall: 32 KiB is below the talker's measured
15,152-byte peak plus the 24,576-byte headroom floor, which is 39,728.

## Fix shape

Give the four keys a `toml` row in `knobs.toml` (the board key) and a field
in `Knobs`. Then render a board's `[knobs]` into the same
`Kconfig.from-system-toml` defaults, below the image's values and above the
module default (the RFC-0049 order), on the leaf road and the `nros build`
road alike. Move the nucleo descriptor's numbers into `[knobs.*]` in the same
commit, and drop the per-image rows from phase-524 W4's chapter.

## Resolution (2026-11-02)

**Schema: every registry knob, no second list.** A board's top-level
`[knobs.<group>]` takes every live row of `knobs.toml`, keyed by its image
key, so a board and an image spell a knob the same way
(`[knobs.zephyr] heap_size`). No row gained a `toml` column: that column keeps
meaning "a typed tenant of `nros-platform-config` `Knobs` reads it from
`NROS_BOARD_TOML`", and `a_registry_toml_row_is_a_typed_tenant_field` holds
the column and the tenants equal in both directions. `Knobs::deserialize_board`
(both board readers: the CLI's `BoardFile` and the build scripts'
`BoardKnobsFile`) moves the board-only keys into `Knobs::registry` before the
typed parse, so a typo is still refused. A platform file keeps the plain parse
and still refuses a board-only key.

**Precedence: image > board > builtin** (RFC-0049's ladder, the board rung
under the image's front-end). An image's own `[image.<id>.knobs]` wins key by
key; env and `-DNROS_OVERRIDE_<ID>` stay above both.

**Carrier, per road** (`leaf_settings::board_rung`):

| road | carrier | keys |
|---|---|---|
| Zephyr west (leaf and bringup) | a `default` in `Kconfig.from-system-toml` (`leaf_kconfig::render_over`), only when the board package applies (issues 1782/2179); a board key whose symbol sits under another RMW's `if` is skipped | every key: Zephyr has no `NROS_BOARD_TOML` |
| cargo leaf + cargo workspace | the `[env]` rows, inside `board_fact_layer` | board-only (a typed key rides `NROS_BOARD_TOML`) |
| cmake leaf (`--image-env-out`) + cmake workspace (`cmake_image_rows`) | the issue-1712 image-env file | board-only |

Gate: one test per road (`board_rung_tests`, `leaf_system`'s
`a_board_knob_reaches_the_zephyr_and_cmake_leaf_roads`, `build`'s
`the_workspace_cmake_road_carries_the_board_knobs`), each over the real Pico W
descriptor. Each was mutation-checked: removing the rung from that road turns
its test red.

**The two boards.** The Pico W descriptor states all five knobs. The Nucleo
states `max_large_subscribers = 0` only: the ethernet image's three extra knobs
pay for the IP stack the serial image does not build, so they are that image's
to state. `twin_knobs` left `board-support.toml` and the generator refuses it.

**Build proof.** Copies of phase-524 W3's two leaves (`examples/zephyr/c/talker`
plus one serial image) with their `[image.<id>.knobs.*]` tables deleted, so the
knobs exist only in the descriptors, through the same leaf road (`west build
... -DNROS_IMAGE=<id>`):

| image | RAM | FLASH | `.config` vs W3 |
|---|---|---|---|
| nucleo serial | 351,936 B (89.50 %) | 523,748 B | identical |
| Pico W serial | 254,888 B (94.29 %) | 524,780 B | identical |

RAM is W3's to the byte. FLASH moved since W3 (Nucleo 522,088 -> 523,748, Pico W 524,156 -> 524,780):
that is the tree, not the rung. The W3 leaves, with their image knobs, rebuilt
on this tree give a `zephyr.bin` byte-identical to the board-knob build, on
both boards. `Kconfig.from-system-toml` names the rung on each row
(``# board `rpi-pico-w-zephyr` [knobs] zenoh.max_large_subscribers = 0``).
The Nucleo ELF booted in Renode 1.17.0 (`run-serial.sh`) and `ros2 topic
echo` printed `data: 'Hello World: 1'`, then 2 and 3: PASS.
