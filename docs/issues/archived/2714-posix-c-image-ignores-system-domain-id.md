---
id: 2714
title: "A `linux` image drops `[system]`/`[image]` `domain_id` and `locator` on both roads, so a host node joins domain 0 when system.toml says otherwise"
status: resolved
resolved_in: "d87836b231 (#4285)"
type: bug
area: [cmake, boards, c-api, cpp-api, workspace]
severity: high
found: 2026-11-18
related: [1946, 0687, 0794, 1434]
---

## What is wrong

The book's `system.toml` reference says `[image.<id>] domain_id` and
`locator` "override `[system]`'s. `locator` is what the image dials". On the
`linux` board (`NROS_PLATFORM = posix`) neither reaches the running image, in
Rust, C or C++:
- with `$ROS_DOMAIN_ID` unset, a node joins domain 0 whatever `system.toml`
  states;
- with `$NROS_LOCATOR` unset, it dials the backend's default whatever
  `system.toml` states.

Nothing reports it. Nothing that launches a host image sets those variables
from `system.toml` either: no `ROS_DOMAIN_ID` writer exists in the CLI. A
deployment that states domain 7 for its host images gets domain 0: a silent
wrong image.

Every other platform honours both. Found by phase-580 W3's freshness check
(issue 2587). It is not caused by `include`.

## Where each road drops it (read 2026-11-18, main `77b058e609`)

| Road | The value arrives at | Dropped by |
| --- | --- | --- |
| Rust (`nros::main!`) | `DeployOverlay.domain_id` / `.locator`, from the leaf system (`overlay_from_leaf`); `nros-cargo.toml` also carries `NROS_ENTRY_DOMAIN_ID` | `nros-board-linux::hosted_baked_rung`, which builds the resolver's baked rung from `node_name`, `namespace` and `rmw` only: "`locator` / `domain_id` — deliberately absent" |
| C/C++ (cmake) | `nano_ros_add_entry(... DOMAIN_ID 7)` in the generated root (issue 1946's producer) | `cmake/NanoRosEntry.cmake`, whose `NROS_ENTRY_DOMAIN_ID` / `NROS_ENTRY_LOCATOR` bake sits in a block gated `NOT NROS_PLATFORM STREQUAL "posix" AND NOT _nra_is_zephyr`; the entry's boot blob bakes a domain only from a launch-declared deploy (issue 0794) |

Measured on `examples/workspaces/c`, `pubsub_bringup:linux_talker`, with
`domain_id = 7`: no TU has `NROS_ENTRY_DOMAIN_ID` (`compile_commands.json`),
and the boot blob is `.domain_id = 0` with `NROS_BOOT_SET_DOMAIN` unset.

## The sweep (every platform through those two places)

- The C bake is skipped only for posix and Zephyr. Zephyr takes
  `CONFIG_NROS_DOMAIN_ID` from `system.toml` (`leaf_kconfig.rs`, checked
  against the image by `NanoRosDomainAgreement.cmake`).
- The Rust boards other than `linux` apply the overlay through
  `nros-board-common`'s `BaseConfig` (`base_config.rs`: `locator`,
  `domain_id`). That includes the host-process boards `freertos-posix` and
  `threadx-linux`.
- So the drop is exactly the `linux` board, on both roads.

## Why the stated reason does not hold

`hosted_baked_rung` says leaving them `None` "is what keeps `$NROS_LOCATOR` /
`$ROS_DOMAIN_ID` authoritative" (citing issue #48). The resolver they all
reach, `ExecutorConfig::try_resolve_with`, already ranks each field env >
baked > default. A baked value therefore cannot outrank the environment. It
can only replace the bottom rung (domain 0, the backend's default locator)
with what `system.toml` states.

## A stated domain is every domain

`[system] domain_id` is a REQUIRED key: the CLI refuses a `system.toml`
without it. So every image states a domain (its own, else `[system]`'s), and
a stated 0 is a statement like any other. The C road carries it as the C
ABI's explicit zero (255, `NROS_ENTRY_DOMAIN_ID_C_ABI`), so the C/C++ and
Rust roads agree. Only the locator can be unstated. Ruled (relayed by the
manager): the boot warning fires whenever the environment and the statement
both set a value and differ, an inherited `[system]` 0 included. A native
test that sets `$ROS_DOMAIN_ID` against an image stating 0 therefore prints
it, and its behaviour does not change.

## Done when

- A `linux` image in Rust, C and C++ built from a bringup stating
  `domain_id = N` / `locator = L` joins N and dials L with the environment
  unset, and `$ROS_DOMAIN_ID` / `$NROS_LOCATOR` still win when set.
- The precedence is the one rule in `try_resolve_with`, on every road. The
  cross-road test compares the precedence (env set and unset, baked set and
  unset), not only the baked value.
- The locator gets the same treatment as the domain, in the same places.
- The sweep above is re-run in the fix's commit message.

## Resolved (2026-11-19)

Fixed by d87836b231 (#4285, merged 2026-11-18). Each done-when item:

- **Baked rung on both roads.** A `linux` image in Rust, C and C++ takes its
  stated `domain_id`/`locator` with the environment unset, and
  `$ROS_DOMAIN_ID` / `$NROS_LOCATOR` still win when set. That is
  `hosted_baked_rung` on the Rust board, and on C/C++ the host runners'
  leading (locator, domain_id) that `NanoRosEntry.cmake` bakes.
- **One precedence rule.** `nros::env::env_overrides_statement` decides it
  once for the Rust board and for `nros_cpp_warn_env_overrides_stated`. The
  cross-road test `host_connect_facts` compares the precedence (env set and
  unset, stated and unstated) across the three languages. It is not limited
  to the baked value, and the image warns exactly when both are set and
  differ.
- **The locator gets the same treatment as the domain**, in the same places.
- **The sweep is re-run in the commit message.**

Proved on CI: run-matrix 44471252909 (head `eeddd35198`, which carries the
fix), tier 1:
`PASS [ 2.426s] nros-tests::host_connect_facts
a_linux_image_takes_its_stated_connect_facts_below_the_environment_and_warns_when_overridden`.
The fix commit records that the test fails on the old linux board.
