---
id: 1931
title: "Remove the deprecated `init_with_launch*` entry points (Rust and C++) at the next breaking release"
status: open
type: tech-debt
area: [api]
severity: low
found: 2026-10-16
related: [1917]
---

## What is deprecated

Issue 1917 retired the runtime launch-overlay placeholder. Its follow-ups
deprecated the four entry points named after it, each of which now forwards to
a function that says what it does:

| Deprecated | Forwards to | Deprecated in |
| --- | --- | --- |
| `nros::init_with_launch_auto()` | `nros::init()` | PR #2281 (`#[deprecated(since = "0.7.0")]`) |
| `nros::init_with_launch(path)` | `nros::init()` behind `path.exists()` | PR #2281 |
| `rclcpp::init_with_launch_auto(argc, argv)` | `rclcpp::init_in(argc, argv)` | PR #2289 (`NROS_CPP_DEPRECATED_MSG`) |
| `rclcpp::init_with_launch(path, argc, argv)` | `rclcpp::init_in(argc, argv)` behind an `fopen` check | PR #2289 |

None reads a launch file. Launch content reaches an image at build time
(`nros sync` → SystemModel → the generated entry); see 1917's resolution.

## In-tree callers: none

Every caller moved in the deprecating PRs: the `nros new` hosted template, the
Rust and C++ examples, the `nros-tests` bins. What still names them:

- `packages/testing/nros-tests/tests/init_api.rs` — two tests under
  `#[allow(deprecated)]` that keep the Rust spellings working until removal;
- `packages/api/nros-cpp/tests/compile/init_honours_ros_args.cpp` — one call
  under a `-Wdeprecated-declarations` pragma, proving the C++ forward;
- the re-export in `packages/api/nros/src/lib.rs` (`#[allow(deprecated)]`);
- the API-parity ledger rows `rust:init_with_launch*` / `cpp:init_with_launch*`.

Check before removing: `git grep -n 'init_with_launch' -- ':!docs/issues/archived' ':!CHANGELOG.md'`.

## Why not now

Removal breaks a downstream caller at compile time, and the deprecation shipped
no earlier than 0.7.0. One release with the warning is the courtesy a
deprecation exists to give; removing in the same release would make the
attribute pointless.

## Removing them

At the first breaking release after the deprecation has shipped in one:

- delete the four functions (and `#include <cstdio>` in `rclcpp/node.hpp` if
  nothing else uses it) and `InitError::LaunchFileNotFound`, which only
  `init_with_launch(path)` returns — itself a public enum variant, so a
  `changelog.d/*.breaking.md` names both;
- delete the two `init_api.rs` tests and the deprecated-forward case in
  `init_honours_ros_args.cpp`, and the `#[allow(deprecated)]` on the re-export;
- mark the four ledger rows removed.
