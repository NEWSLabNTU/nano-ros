---
id: 2941
title: "Seven native_sim example images state a QEMU slirp locator (tcp/10.0.2.2), which no native_sim image can reach; the fixtures' own locator define hid it"
status: resolved
type: bug
severity: medium
area: [examples, zephyr]
related: []
found: 2026-11-24
resolved: 2026-11-28
resolved_in: "fa6b48e1f1"
---

## What is wrong

Seven images on the `zephyr` board state `locator = "tcp/10.0.2.2:<port>"`
in their bringup's `system.toml`.

| Bringup | Image | Locator |
| --- | --- | --- |
| `examples/workspaces/rust/src/demo_bringup` | `zephyr` | `tcp/10.0.2.2:7430` |
| `examples/workspaces/rust/src/demo_bringup` | `zephyr_robot1` | `tcp/10.0.2.2:7469` |
| `examples/workspaces/realtime-rust/src/demo_bringup` | `zephyr` | `tcp/10.0.2.2:7471` |
| `examples/workspaces/features/src/demo_bringup` | `zephyr_rust_params` | `tcp/10.0.2.2:7461` |
| `examples/workspaces/features/src/demo_bringup` | `zephyr_rust_lifecycle` | `tcp/10.0.2.2:7463` |
| `examples/workspaces/features/src/demo_bringup` | `zephyr_rust_qos` | `tcp/10.0.2.2:7465` |
| `examples/workspaces/safety/src/demo_bringup` | `zephyr_rust_safety` | `tcp/10.0.2.2:7467` |

The `zephyr` board's west board is `native_sim/native/64`. A native_sim image
is a host process: its sockets go through NSOS to the host's own network
stack, so the router is at `127.0.0.1`. `10.0.2.2` is QEMU user-mode
networking's address for the host. It is correct on the 105 other images
that state it, all on QEMU boards (mps2-an385, qemu-armv7a-nuttx, rv-virt-*),
and unreachable from a native_sim image.

A user who copies one of these images, or builds it with `nros build`, gets a
guest that dials `10.0.2.2` and never opens a session.

## Why nothing caught it

Every one of these images is built for the tests by
`scripts/build/zephyr-fixture-leaves.sh`, which passes the row's
`-DCONFIG_NROS_LOCATOR="tcp/127.0.0.1:<port>"`. A command-line Kconfig value
beats the rendered `system.toml` fragment, so the stated value was dead in
every build anyone ran.

It surfaced only when that define went missing (fixed in PR #4790): the release
run-matrix's Rust native_sim guests then baked the stated `10.0.2.2` address
and failed. The stated ports are the cells' allocated ones, so the address is
the only wrong part.

## Fix

- State `tcp/127.0.0.1:<same port>` on the seven images.
- Close the class: `scripts/check-image-locator-bake.py` already reads image
  locators. It should refuse a `tcp/10.0.2.2` locator on an image whose board
  is a host-process target (native_sim), and a `tcp/127.0.0.1` locator on a
  QEMU slirp board.

Not a release blocker: the fixtures override the value, so no cell depends on
it.

## Acceptance

- `nros build` of each image without a fixture define bakes
  `tcp/127.0.0.1:<port>`, as `zephyr/.config` and the guest's boot line show.
- The gate refuses a slirp address on a native_sim image, with a negative
  control.

## Progress (2026-11-24)

- The seven images now state `tcp/127.0.0.1:<port>`, on the same ports.
- `check-image-locator-bake` rule 6 refuses a `10.0.2.2` locator on any tracked
  image whose board's `[board.zephyr] west_board` is native_sim. Before the
  edit it refused exactly these seven; after it, none.
- Left for acceptance: one of the seven built with `nros build` and no
  fixture define bakes `tcp/127.0.0.1:<port>` (its `zephyr/.config` and the
  guest's boot line). That build needs a host with the Zephyr SDK.
- Not done: the converse, refusing `127.0.0.1` on a QEMU user-mode board.
  The rule would need the set of QEMU boards, which no descriptor states yet.

## Acceptance met (2026-11-28, host 139)

`nros build zephyr_rust_params` in examples/workspaces/features (one of the
seven images; main fa6b48e1f1), with no fixture define:
- `zephyr/.config` reads `CONFIG_NROS_LOCATOR="tcp/127.0.0.1:7461"`;
- the build dir's `CMakeCache.txt` holds no `CONFIG_NROS_LOCATOR`, so the
  value is the image's own;
- the guest dials it at boot. With no router listening the host answers RST
  at once: `zenoh open: no session with tcp/127.0.0.1:7461 in one attempt
  (connect_timeout_ms=0)` at 2.001 s.

The gate half (rule 6, with its negative control) landed with the fix. The
converse rule, refusing `127.0.0.1` on a QEMU user-mode board, is still not
written: no descriptor states which boards are QEMU user-mode. It is a
separate class and not part of this issue's acceptance.

