---
id: 2147
title: "`nros build`'s Zephyr west road pins no SDK, so it takes whichever SDK the CMake user registry yields — on a multi-checkout host, another checkout's, and `check-template-copy-out` goes red"
status: resolved
type: bug
area: [cli, zephyr, build, testing]
severity: medium
found: 2026-10-29
related: [1811, 1254, 0698]
resolved: 2026-10-31
---

## What

`check-template-copy-out` (`scripts/check-template-copy-out.sh`) builds every
Zephyr template copy with `nros build --zephyr-workspace <ws>`. Nothing on that
road sets `ZEPHYR_SDK_INSTALL_DIR`. `git grep` finds no
`ZEPHYR_SDK_INSTALL_DIR`, `ZEPHYR_TOOLCHAIN_VARIANT` or `SDK_VERSION` anywhere
under `packages/cli/*/src`. So Zephyr's `FindZephyr-sdk.cmake` falls back to the
CMake user package registry, `~/.cmake/packages/Zephyr-sdk/*`.

On the host where this was measured, the registry holds six SDKs:

- the store's 0.16.8 and 1.0.1;
- four in-checkout copies (`<checkout>/scripts/zephyr/sdk/zephyr-sdk-<v>`),
  registered by `just zephyr setup` from three other checkouts, one of which
  registered two versions.

CMake chooses among them by the registry's own ordering. On 2026-10-29 it chose
another checkout's SDK, and `nros_checkout_guard.cmake:170`
(`nros_assert_sdk_not_foreign`, phase-478 I2) refused it correctly:

```
nano-ros: the Zephyr SDK belongs to ANOTHER nano-ros checkout (phase-478 I2).
    SDK in use:    <another checkout>/scripts/zephyr/sdk/zephyr-sdk-0.16.8
```

Five Zephyr template copies failed, and with them `check::build` and `just ci
gate` (PR #2829, a docs-only change). The same worktree had passed the same gate
an hour earlier. Nothing in the tree changed. The registry's answer did.
Re-running with
`ZEPHYR_SDK_INSTALL_DIR="$(bash scripts/lib/zephyr-sdk.sh resolve)"` passed.

## Why it is the CLI's defect, not the gate's

The fixture roads were fixed for exactly this in issue 1811:
`nros_zephyr_sdk_env` (`scripts/build/zephyr-toolchain.sh`) resolves the SDK
whose version the Zephyr tree's `SDK_VERSION` pins. It goes store first, then a
registry entry of exactly that version, and refuses otherwise. Its comment says
"the registry's own ordering is the defect this function exists to bypass". Its
callers are `zephyr-fixture-run-one.sh`, `west-fixtures.sh` and
`just/zephyr-dev.just`.

`nros build` is the user's road (the book's Zephyr flow), and it never got that
rule. So the template gate is only the place this surfaced. A user with two
checkouts, or with a checkout plus the store, gets an SDK chosen by registry
order. The guard then refuses it, or, for a non-nano-ros registration, the
guard cannot recognise it and the build may silently use a different SDK
version (1811's `libc-hooks.c` failure on 3.7).

The variant half is unverified: whether the CLI road also leaves
`ZEPHYR_TOOLCHAIN_VARIANT` unset, so that CMake 4 hits issue 0698's parse error,
was not measured. The grep says the CLI never sets it. Whether something
upstream of it does was not checked.

## Direction

Fix the class: one rule, not a second spelling of it.

- `nros build`'s west road resolves the SDK the same way `nros_zephyr_sdk_env`
  does: the pinned `SDK_VERSION`, store first, then a registry entry of exactly
  that version, else a refusal naming `just setup zephyr`. It passes the result
  to west as `ZEPHYR_SDK_INSTALL_DIR`, and the variant as `zephyr` for an SDK
  board.
  - The CLI cannot source a shell helper, so the ladder needs one
    implementation the two languages share, or a CLI verb the shell helper
    calls.
  - A caller's own `ZEPHYR_SDK_INSTALL_DIR` still wins, as it does in 1811.
- Then `check-template-copy-out` needs no SDK handling of its own.
- Extend `check-zephyr-sdk-env.sh`, or add a sibling, so that every road that
  runs `west build` either pins the SDK or is named as exempt. That check exists
  for the shell roads and did not reach the CLI, which is 0196's reach gap.
- Acceptance: on a host whose registry lists a foreign-checkout SDK first,
  `nros build --zephyr-workspace` on a template copy configures against the
  pinned store SDK. A test fixture can stage that with a temporary `HOME`
  holding a crafted `~/.cmake/packages/Zephyr-sdk/`.

## The variant half, measured (2026-10-30)

It is real. On a host with CMake 4.4.4 and no `ZEPHYR_TOOLCHAIN_VARIANT` in
the environment, `just check template-copy-out` failed all five Zephyr
template copies at configure:

```
CMake Error at zephyr-workspace/zephyr/cmake/modules/FindZephyr-sdk.cmake:35 (if):
  if given arguments:
    "(" "zephyr" "STREQUAL" ")" "OR" "(" "NOT" "DEFINED" "ZEPHYR_TOOLCHAIN_VARIANT" ")" ...
  Unknown arguments specified
```

Setting `ZEPHYR_SDK_INSTALL_DIR` alone (the SDK-half workaround above) did not
change it. A `cmake --trace-expand` of one copy shows nothing upstream sets the
variable. `FindDeprecated.cmake:44` runs
`set(ZEPHYR_TOOLCHAIN_VARIANT $ENV{ZEPHYR_TOOLCHAIN_VARIANT})`, which with an
empty environment leaves it undefined. So `nros build`'s west road needs the
variant from `nros_zephyr_toolchain_variant` too, not only the SDK. CI does not
see this because its image runs CMake 3.22, which tolerates the empty operand
(issue 0698's table). The local workaround is
`export ZEPHYR_TOOLCHAIN_VARIANT=zephyr` before the gate.

## Resolution

Fixed 2026-10-31. Both halves, on the one road that had neither.

- `builder::zephyr::resolve_toolchain` is the fixture roads' rule
  (`nros_zephyr_toolchain_variant` + `nros_zephyr_sdk_env`) in Rust, rung for
  rung. The variant is the caller's, else `host` for native_sim, else
  `zephyr`, and is never empty. For an SDK board the SDK is the caller's
  `ZEPHYR_SDK_INSTALL_DIR` (either spelling, any version), else a store root
  of a `[tool.zephyr-sdk*]` entry, else the legacy checkout copy, else a CMake
  registry entry. Each of the last three counts only at exactly the version
  the tree's `SDK_VERSION` pins. Otherwise it refuses, naming
  `just setup zephyr`.
- `nros build`'s west handoff passes both as `ZEPHYR_TOOLCHAIN_VARIANT` and
  `ZEPHYR_SDK_INSTALL_DIR`. Like a missing Zephyr, the refusal is enforced at
  exec, so `--dry-run` still prints the command.
- The two languages do not share one implementation. The CLI has no shell
  helper it could call, and the shell roads run before a CLI exists in early
  provisioning. So the reach is gated instead. `check-zephyr-sdk-env.sh` now
  also refuses a non-test `Handoff::new("west"` in `packages/cli` whose file
  sets neither variable. It has a negative control, and removing the
  `build.rs` change makes it fail on `cmd/build.rs`.
- Tests: `builder::zephyr::toolchain_tests` (5). One is the acceptance case
  above: a registry whose first entry is another checkout's SDK of the same
  version, against a store SDK, resolves to the store's.

Verified: `just ci gate` passed (25m48s, including `check::build` and its
`template-copy-out`) from a shell with neither variable exported, on CMake
4.4.4. Before this, that host failed all five Zephyr template copies at
`FindZephyr-sdk.cmake:35`.
`just check template-copy-out` run alone the same way built all five Zephyr
copies (`ament-workspace-port`, `cpp-port-minimal-publisher`,
`rclcpp-compat-smoke`, `topic-state-monitor-port`, `zephyr-byo`), 15 projects
OK.
