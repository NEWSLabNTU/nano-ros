---
id: 2194
title: "A Zephyr board descriptor cannot state the zenoh pool or heap knobs its RAM needs, and the leaf road has no board knob rung"
status: open
type: limitation
severity: medium
area: [boards, zephyr, knobs]
related: [issue-1782, issue-0460, phase-524]
found: 2026-11-01
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
