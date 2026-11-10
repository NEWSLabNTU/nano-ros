---
id: 2376
title: "The Tutorial's C and C++ tabs scaffold for mps2-an385-baremetal, which refuses a C or C++ entry, so their first `nros build` fails"
status: resolved
type: bug
severity: high
area: [book, cli, education]
purpose: [education]
related: [phase-537, 2269]
found: 2026-11-09
---

**Home: [phase-537](../../roadmap/phase-537-book-user-guide-organization.md)** (W5, the Tutorial). Found while writing issue 2269's QoS lab.

## What fails

Tutorial chapter 1 (`book/src/tutorial/run-a-talker.md`) tells a C or C++
reader to run:

```text
nros new sensor_ws --board mps2-an385-baremetal --template talker --lang c
```

The scaffold succeeds. The first `nros build` then fails at configure,
on main (f450f9a3c3, 2026-11-09), for `--lang c` and `--lang cpp` alike:

```text
CMake Error at …/cmake/NanoRosEntry.cmake:1324 (message):
  nano_ros_entry: `nros dev codegen entry-pack --lang c --board
  mps2-an385-baremetal` failed (rc=1).
  Error: codegen entry-pack: unknown board key `mps2-an385-baremetal` — the
  entry pipeline knows: linux, zephyr, …, mps2-an385-freertos, …
```

The refusal is deliberate. `nros-entry-lower`'s board table leaves the
bare-metal `mps2-an385` out because "a C or C++ entry has no runner for" it.
So the scaffold accepts a pairing the build refuses, and the Tutorial
teaches it in every chapter's C and C++ tabs.

## Why nothing caught it

The `tutorial-c` and `tutorial-cpp` probe tracks exist
(`scripts/probe/probe-tracks.json`, `just probe tutorial-c`), but they are
`weekly: false` and no workflow runs them. Only the Rust tracks
(`first-project`, `tutorial-2` to `tutorial-10`) replay.

## What would close it

One of:

1. A C/C++ entry gets a runner on bare-metal `mps2-an385`, so the pairing
   builds; or
2. the Tutorial's C and C++ tabs scaffold for a board that takes a C entry
   (`mps2-an385-freertos` is the same emulated board), and `nros new`
   refuses `--lang c|cpp` with a board the entry pipeline does not know,
   naming the alternative.

Either way, `tutorial-c` and `tutorial-cpp` run on a schedule.

## Resolution (2026-11-10)

Option 2, the user's ruling (bare metal stays Rust only):

- `nros new --lang c|cpp` refuses a board with no C/C++ entry and names the
  same chip's RTOS boards (`refuse_c_family_without_entry` in `cmd/new.rs`,
  on both the new-workspace and the into-workspace roads).
- Every Tutorial C and C++ tab scaffolds for `mps2-an385-freertos`. Chapter 1
  provisions and scaffolds it; chapters 2, 3 and 4 name its `freertos` image
  and its board-default TCP locator; chapter 9's second target for C and C++
  is the Linux host; chapter 10 runs `nros run freertos`.
- `tutorial-c` and `tutorial-cpp` run on the FreeRTOS row and are weekly in
  `probe-platforms.yml`, with no `awaits`.
- `nros new --to <bringup>` gives the new node a row in the bringup's
  `system.contract.yaml` with its whole surface, for every role (talker,
  listener, server, client, action server, action client), per language: a
  C or C++ client and action node ticks every 500 ms, a Rust client every
  second, and the Rust action nodes have no timer. It also writes the node's
  entry under `topics:`, `services:` or `actions:`, creating the section if
  it is missing. The first version covered the talker and listener only, and
  probe run 42780892067 stopped the C/C++ tabs at step 630 (chapter 6) on the
  same refusal, with four nodes missing. Test:
  `every_role_added_with_to_joins_the_contract_whole`. Probe run 42754577268
  found the gap: on the CMake road, a contract with `params:` on the talker
  and no row for the listener has the parameter store refused ("1 of 2 nodes
  in this image declare no `params:` ... Declare `params:` on every node, or
  on none"), so chapter 3's C and C++ tabs stopped at step 340. The Rust road
  sizes the store without that refusal, which is why the Rust tab passed.
  Test: `a_node_added_to_a_workspace_touches_a_bringup_only_with_to`.
