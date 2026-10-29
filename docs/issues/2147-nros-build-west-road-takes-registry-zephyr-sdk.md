---
id: 2147
title: "`nros build`'s Zephyr west road pins no SDK, so it takes whichever SDK the CMake user registry yields — on a multi-checkout host, another checkout's, and `check-template-copy-out` goes red"
status: open
type: bug
area: [cli, zephyr, build, testing]
severity: medium
found: 2026-10-29
related: [1811, 1254, 0698]
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
