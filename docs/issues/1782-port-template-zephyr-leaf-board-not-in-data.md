---
id: 1782
title: "`cpp-port-minimal-publisher/zephyr` names the generic `zephyr` board, which lowers to `native_sim`, so its build cannot be derived from its own data"
status: open
type: bug
area: [examples, tooling]
severity: low
found: 2026-10-10
related: [issue-1764, phase-482, issue-1296, issue-1308]
---

## What was measured

Issue 1764's fix makes `check-template-copy-out` build each template
SUB-PROJECT from a copy, by the road the sub-project's board declares. That
works for `cpp-port-minimal-publisher/mps2-an385-freertos`: its board
descriptor states `[board.cmake] toolchain_file`, and the copied leaf builds
with it (one ARM ELF, `build/minimal_publisher`).

It does not work for `cpp-port-minimal-publisher/zephyr`. Its `system.toml`
says `[image.zephyr] board = "zephyr"`, and the generic `zephyr` descriptor
lowers that to `west_board = "native_sim/native/64"`. `nros build --dry-run`
in the leaf prints `west build -b native_sim/native/64 …`.

Three builds of a copy of the template, against a Zephyr 3.7 workspace, with
`-DZEPHYR_EXTRA_MODULES=<this checkout>`:

| board | conf | result |
| --- | --- | --- |
| `native_sim/native/64` (what the data says) | Zephyr's default (`prj.conf`) | compile error: `conflicting types for 'pthread_t'`, `clockid_t`, `timer_t` (host libc against Zephyr's POSIX types) |
| `mps2_an385` | Zephyr's default (`prj.conf`) | compile error: `unknown type name 'pthread_t'` in `zenoh-pico/system/platform/zephyr.h` |
| `mps2/an385` | `prj.conf;prj-zenoh.conf;<nano-ros>/cmake/zephyr/mps2-an385.conf` (the README) | builds, `zephyr.elf` |

So the board the port targets and the Kconfig fragments it needs are stated
only in the template's README and in its fixture row (`examples/fixtures.toml`,
`west_build_name = "build-cortex-m-cpp-port-minimal-publisher-zenoh"`, whose
board tail `mps2-an385.conf` is added by `scripts/build/zephyr-fixture-leaves.sh`).
Neither is data a copy of the leaf carries. The README itself says
`native_sim` is not a target for this port, which contradicts the board its
`system.toml` names.

## Consequence

`check-template-copy-out` reports this sub-project NOT VERIFIED (through
`nros_check_unverified`) even on a host with Zephyr installed, naming this
issue. A user who copies the template and runs `nros build` in `zephyr/`
gets the `native_sim` build that fails.

## Direction

Make the leaf state its target. Two candidates, to be decided with a Zephyr
build of each:

1. `board = "mps2-an385-zephyr"` (descriptor exists, `west_board =
   "mps2_an385"`), plus the image's `conf = [...]` for `prj-zenoh.conf`. The
   QEMU network tail `cmake/zephyr/mps2-an385.conf` still has to come from
   somewhere; the board descriptor is the natural owner.
2. Keep `board = "zephyr"` and let an image state a concrete west board.

Either way, acceptance is: a copy of the template builds in `zephyr/` by a
road derived from its data, and `check-template-copy-out` builds it instead of
skipping it where Zephyr is installed.
