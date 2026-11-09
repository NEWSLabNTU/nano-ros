---
id: 2301
title: "On a Zephyr board `nros run` cannot see the image's default locator, so `--router auto` starts no router (mps2-an385-zephyr, qemu-cortex-a53, nucleo-f767zi-zephyr, rpi-pico-w-zephyr, fvp-aemv8r-smp)"
status: resolved
type: bug
area: run, zephyr, boards
related: [phase-531, 1793, 2290, phase-548, 2353]
found: 2026-11-06
resolved: 2026-11-09
resolved_in: "the run plan's BOARD rung reads the locator a west build baked, `CONFIG_NROS_ZENOH_LOCATOR` (or the XRCE agent pair) from `<build_dir>/zephyr/.config`, where the descriptor rule answers nothing"
---

**Home:** [phase-548](../../roadmap/phase-548-three-languages-every-board.md)

## What

The run plan's `link` comes from the image's `locator`. When the image states
none, it falls back to the board's derived default,
`nros_board_common::board_net::default_locator` (the PR that filed this
issue). That rule deliberately answers nothing for a `stack = "os"` board with
no `kernel_config`, which is every Zephyr board. On Zephyr the default lives in
Kconfig (`CONFIG_NROS_ZENOH_LOCATOR`). The module defaults it to
`tcp/127.0.0.1:7447`, a board package's `prj.conf` may override it
(`nros-board-mps2-an385-zephyr/prj.conf` says `tcp/10.0.2.2:7447`), and the
image's rendered fragment merges last.

So for a scaffolded image on mps2-an385-zephyr, qemu-cortex-a53,
nucleo-f767zi-zephyr, rpi-pico-w-zephyr or fvp-aemv8r-smp, `nros run` starts
no router, and the book's pages for them still begin with a hand-started
`rmw_zenohd`. `zephyr` (native_sim) is unaffected: it is a host target, and
`run_exec::decorate` gives it `tcp/127.0.0.1:7447`, which is also the Kconfig
default.

## Direction: the planner reads the merged Kconfig value

There has to be one source for the default, and the choice is which way the
fact flows.

**Chosen: the planner reads `CONFIG_NROS_ZENOH_LOCATOR` (and the XRCE agent
pair) from the image's built `.config`** (`<west build dir>/zephyr/.config`,
whose directory the plan already carries as `build_dir`), on the `board` rung
below the image's `locator`.

- It is what the firmware dials. Zephyr merges every fragment last-wins
  (CLAUDE.md, issue 0876): the module default, the board `prj.conf`, the
  image's rendered `nros/zephyr.conf`. Only the merged `.config` holds the
  answer, and any other reader re-derives the merge and can disagree with it.
  That was 1793's shape.
- It keeps RFC-0109 D2: on an `"os"`-stack board the RTOS's own configuration
  owns the network. The descriptor states no addresses for these boards, and
  it should not start.
- It covers an out-of-tree Zephyr board package for free.
- `nros run` builds before it plans, so the file exists whenever a plan
  needs it. A missing `.config` is "no evidence", and the plan states no link,
  exactly as today.

**Rejected: the descriptor rule feeds Kconfig** (a derived
`CONFIG_NROS_ZENOH_LOCATOR` default from `[board.net]`). That would need
network facts in Zephyr descriptors, which RFC-0109 D2 assigns to the OS. It
would also make a board `prj.conf` value silently dead under a later fragment
(the 0876 class), and it still would not see a value the image or a user
fragment sets.

## Next

- Read the merged value in `cmd/run_plan.rs` beside `board_default_locator`.
  Zephyr then has one more input to the same rung, not a second rung. Report
  it in `--why` as `board` with the file named.
- Test: a west build dir fixture whose `.config` states a locator yields that
  link, and a missing `.config` yields none.
- Then the book's Zephyr-on-QEMU pages can drop their hand-started router.

## Resolution (2026-11-09)

Done as chosen above, on the same rung. `run_plan::board_default_locator`
now takes the image's west build directory. When the descriptor rule
(`board_net::default_locator_over`) answers nothing, it reads
`<build_dir>/zephyr/.config` through `run_plan::west_build_locator`:
`CONFIG_NROS_ZENOH_LOCATOR`, or else `udp/<CONFIG_NROS_XRCE_AGENT_ADDR>:<PORT>`.
The plan's link comes from it on the `board` rung, below the image's own
`locator`. A build with no `.config` is no evidence, and the plan states no
link, as before. No second spelling was added: the descriptor still states
no network for a Zephyr board (RFC-0109 D2), and `board_net` is unchanged.

The router decision did not change. `run_exec::host_side` already maps an
emulator's `10.0.2.2` to this host's loopback, so on mps2-an385-zephyr
`--router auto` starts `rmw_zenohd` on `tcp/127.0.0.1:7447`, or reuses a
router already there (issue 2315). Measured: QEMU's user-mode network
delivers the guest's `10.0.2.2:7447` to a router on `127.0.0.1`, so the
`0.0.0.0` the starter page asked for is not needed.

Issue 2353's second half was this bug, measured on mps2-an385-zephyr; it is
resolved with it. Its first half (the west runner's workspace) was a
separate defect, fixed in the same change.

A host target (Zephyr `native_sim`) still takes its link from
`run_exec::decorate`, not from the board rung. Its `.config` default is the
same `tcp/127.0.0.1:7447`.

## Verified

- `cargo test -p nros-cli-core --lib`:
  `run_plan::tests::a_zephyr_image_links_to_the_locator_its_build_baked`
  covers a fixture west build. With no `.config` there is no link. A baked
  `tcp/10.0.2.2:7447` gives the link on the `board` rung, and the router
  listens on `tcp/127.0.0.1:7447`, started when the port is free and reused
  when a router answers (both through the `plan_router_with` probe seam). The
  XRCE pair gives `udp/10.0.2.2:2018`, and an image `locator` still outranks
  the baked one.
- In the `ros2` box (Humble, Zephyr 3.7), 2026-11-09, with `ZEPHYR_BASE`
  unset, in a workspace outside any west workspace:
  `nros new zm --board mps2-an385-zephyr --template talker --lang c --rmw zenoh`,
  `nros build`, then `nros run zm_node --expect 'Publishing:'` printed
  `nros run: starting the zenoh router on tcp/127.0.0.1:7447` and
  `nros run: matched after 4.3 s: Publishing: 'Hello Wo`. With a hand-started
  router on `0.0.0.0:7447`, it printed `nros run: reusing the zenoh router
  already listening on tcp/127.0.0.1:7447`. Beside the run,
  `ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort`
  received `data: 'Hello World: 1'`. Before the fix, the same plan
  (`--dry-run --why`) had no `[link]`.
