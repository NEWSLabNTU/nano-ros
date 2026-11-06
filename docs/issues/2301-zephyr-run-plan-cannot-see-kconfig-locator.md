---
id: 2301
title: "On a Zephyr board `nros run` cannot see the image's default locator, so `--router auto` starts no router (mps2-an385-zephyr, qemu-cortex-a53, nucleo-f767zi-zephyr, rpi-pico-w-zephyr, fvp-aemv8r-smp)"
status: open
type: bug
area: run, zephyr, boards
related: [phase-531, 1793, 2290]
found: 2026-11-06
---

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
