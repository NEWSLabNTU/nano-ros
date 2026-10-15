---
id: 1928
title: "An inherited `$NROS_CLI` naming ANOTHER checkout's build outranks this checkout's CLI in `nros_cli_bin`, `nros_resolve_cli` and `nros_cli_bin_path`"
status: resolved
type: bug
area: [build, cmake, testing, cli]
severity: medium
found: 2026-10-16
related: [1280, 1391, 1850, 1880]
---

## Measured

All three resolvers of the `nros` CLI take `$NROS_CLI` as rung 1, before
this checkout's own `packages/cli/target/release/nros`:

- `nros_cli_bin` in `scripts/build/cargo.sh` (shell; every recipe and gate,
  and `scripts/lib/source_locate.py` asks it),
- `nros_resolve_cli` in `cmake/NanoRosCli.cmake` (every configure),
- `nros_cli_bin_path` in `packages/testing/nros-tests/src/lib.rs` (tests).

Rung 1 is honoured whatever the value names — including the CLI of a
DIFFERENT nano-ros checkout. A linked worktree inherits its parent shell's
environment (issue 1280: this very session carries 18 absolute paths into
the main checkout from its direnv), so an `NROS_CLI` exported in the parent
— which is exactly what `nros_cli_bin`'s own "not found" text tells a user to
do (`Set NROS_CLI=/path/to/nros.`) — makes every gate in the worktree run the
parent's binary. `--checkout-only`, whose whole contract is "this checkout's
CLI or nothing" (issue 1850), returns it too.

Measured in a worktree of `origin/main` (9cf11082a0), with the main checkout's
CLI built from an older tree:

```
$ NROS_CLI=<main>/packages/cli/target/release/nros bash -c \
    'source scripts/build/cargo.sh; nros_cli_bin --checkout-only; nros_cli_bin'
<main>/packages/cli/target/release/nros
<main>/packages/cli/target/release/nros
$ NROS_CLI= bash -c 'source scripts/build/cargo.sh; nros_cli_bin --checkout-only'
<worktree>/packages/cli/target/release/nros

$ <main>/packages/cli/target/release/nros --version      -> nros 0.5.0
$ <worktree>/packages/cli/target/release/nros --version  -> nros 0.6.0
```

and the cmake twin, from a `cmake -P` probe that includes the worktree's
`cmake/NanoRosCli.cmake` and calls `nros_resolve_cli`:

```
NROS_CLI=<main>/packages/cli/target/release/nros  -> resolved: <main>/packages/cli/target/release/nros
(unset)                                            -> resolved: <worktree>/packages/cli/target/release/nros
```

So a worktree's gates, configures and codegen would run a binary one minor
version behind the tree they claim to check.

## Who sets it — measured, so the severity is honest

Nothing in the tree EXPORTS `NROS_CLI` into a shell: no version of
`activate.sh`, `activate.fish`, `.envrc`, `scripts/sdk-env.sh` or
`just/sdk-env.just` ever has (`git log -S NROS_CLI` over them is empty), and
this agent session's environment does not carry it. The in-tree exports
(`scripts/build/fixtures-build.sh`, `just/check/{fixtures,rmw,tools}.just`)
set it from `nros_cli_bin` for their OWN children, in the same checkout.
The reachable producer is an operator's (or a parent session's) explicit
export, prescribed by our own error text — the same shape as the 24 SDK
paths of issue 1280, one variable that rule never reached. It is latent, not
live on this host; it is still the 1280 rule missing on the one resolver
every gate goes through.

## Siblings (`git grep -nw NROS_CLI -- scripts cmake just packages examples`)

| Reader | Verdict |
|---|---|
| `scripts/build/cargo.sh` `nros_cli_bin` | rung 1 unguarded — fix |
| `cmake/NanoRosCli.cmake` `nros_resolve_cli` | rung 1 unguarded — fix |
| `packages/testing/nros-tests/src/lib.rs` `nros_cli_bin_path` | rung 1 unguarded — fix |
| `scripts/test/rust-fixture-stale.sh` `${NROS_CLI:-$(nros_cli_bin)}` | bypasses the resolver when set — route through it |
| `scripts/lib/source_locate.py` | asks `nros_cli_bin` — fixed with it |
| `examples/mps2-an385-baremetal/*/build.rs` | copy-out projects (RFC-0026); fed by `fixtures-build.sh`'s export of `nros_cli_bin`'s answer — fixed with it |
| `packages/cli/colcon-cargo-ros2/colcon_nano_ros/manifest.py` | installed colcon plugin, no checkout of its own — out of scope |
| `packages/cli/nros-cli-core/.../metadata_probe_cmake.rs` | SETS it to the running binary — not a reader |

## Resolution

Rung 1 of each resolver now goes through issue 1280's rule, using the
spelling that build system already has — no new one:

- `nros_cli_bin` (shell): `nros_reroot_checkout_path` from
  `scripts/lib/checkout-paths.sh`, applied only when the resolver's root IS
  a checkout (an installed SDK has nothing to re-root onto). A re-rooted
  value whose counterpart here is not built answers NOTHING under
  `--checkout-only` and, without it, fails naming the re-root and
  `just setup-cli` — never the other checkout's binary.
- `nros_resolve_cli` (cmake): `_nros_find_root` from
  `cmake/NanoRosCheckoutRoot.cmake` for both owners; a foreign value is
  re-rooted, then must exist as before.
- `nros_tests::nros_cli_bin_path` (Rust): `nros_build_paths::reroot_foreign`.

The raw `${NROS_CLI:-…}` reads that ran ahead of the resolver now ask it:
`nros_seed_leaf_lock` (`cargo.sh`), `nros_board_facts` (`board-facts.sh`),
`check-entity-census.sh`, `rust-fixture-stale.sh`.

Gate: `check-inherited-checkout-paths` (fast line) gains `probe_nros_cli`,
driving the shell and cmake resolvers against synthetic checkouts in both
shapes (side by side, and nested as an agent worktree is), for all three
rows plus "foreign value, counterpart unbuilt → nothing". The stub is
`tools/fake-nros`, not rung 2's path, so a resolver that merely ignored the
variable reads differently from one that re-rooted it. The Rust twin cannot
be driven without a compile, so its call is pinned.

| Mutation | Gate |
|---|---|
| shell: keep `$NROS_CLI` un-rerooted | FAIL — 4 problems (re-root row, both shapes, plus the unbuilt row) |
| cmake: `_nros_cli_reroot_foreign` returns the value unchanged | FAIL — re-root row, both shapes |
| Rust: drop the `reroot_foreign` call | FAIL — pin |
| shell: `--checkout-only` falls back to the raw foreign value | FAIL — unbuilt row, both shapes |

Symptom, in the same worktree after the fix:

```
NROS_CLI=<main>/packages/cli/target/release/nros  -> <worktree>/packages/cli/target/release/nros  (shell, both modes; cmake)
NROS_CLI=<scratch>/ppaths-out/nros (out of tree)  -> kept                                         (shell, both modes; cmake)
NROS_CLI=<worktree>/packages/cli/target/release/nros -> kept
```

Not changed: the copy-out `examples/mps2-an385-baremetal/*/build.rs` (a
stranger's project; in-tree it is fed `nros_cli_bin`'s answer by
`fixtures-build.sh`), and the installed colcon plugin.
