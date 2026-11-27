---
id: 2991
title: "A pristine checkout with no CLI built runs whatever `nros` PATH holds, and a box keeps a stale one forever"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [testing, tooling, dev-env]
related: [1248, 1280, 1850, 1880, 2922]
found: 2026-11-27
---

## What happened

`gate::check::pristine-checkout` went red on PR #4923, in its fresh clone and
only there. The failing gate was `fast::check::zephyr-sdk-env`; the same gate
passed 3 runs in 3 in the worktree:

```
check-zephyr-sdk-env: FAIL -- registered at the pinned version, beside a newer one: got `rc=1 out=`, want `rc=0 out=ZEPHYR_SDK_INSTALL_DIR=.../opt/zephyr-sdk-9.9.1`
check-zephyr-sdk-env: FAIL -- a caller's ZEPHYR_SDK_INSTALL_DIR wins, at any version: got `rc=1 out=`, want `rc=0 out=ZEPHYR_SDK_INSTALL_DIR=.../opt/zephyr-sdk-9.9.2`
```

The clone has no `packages/cli/target/release/nros`, so `nros_cli_bin`
(`scripts/build/cargo.sh`) fell through to rung 3, `nros` on PATH. In the
ros2 distrobox that is `~/.local-box/bin/nros`, an `nros 0.5.0` built on
2026-08-22. It predates `nros dev zephyr-toolchain`, so
`nros_zephyr_sdk_has_board_toolchain` (`scripts/build/zephyr-toolchain.sh`)
got a CLI usage error. Its non-zero exit read as "this SDK lacks the board's
toolchain", and the helper refused a correct SDK.

Moving that binary aside (renamed, reversible) turned the clone green:
`pristine-checkout` 634/634 at the head of #4923. Nothing in the PR was involved.

## Two halves, one class

1. **Nothing refreshes or clears a box's published CLI.**
   `scripts/dev/ros2-distrobox-setup.sh` still ends by telling the user to run
   ~~`nros_box_publish`~~, a function #1248 removed. `ros2-box-env.sh` still puts
   `CARGO_INSTALL_ROOT/bin` (`~/.local-box/bin`) first on PATH. So a box that
   ever published a CLI keeps it, and `$HOME` is shared with the host, so it
   also survives the container being deleted and rebuilt (2026-11-24 here). It
   sits on PATH ahead of every other `nros`, at whatever age it was built.

2. **A checkout with no CLI built silently uses that binary** (more
   important). `nros_cli_bin`'s rung 3 is the "a binary nobody chose" trap that
   router resolution already refuses (`nros_zenohd_bin` deliberately searches
   neither PATH nor a glob). A pristine clone is exactly the checkout whose own
   rung 2 is empty, so every helper that asks the full ladder runs a binary from
   some other tree, of any version. The failure is not a clear "stale CLI" error
   but whatever that version does with today's arguments: here an SDK refusal
   naming the wrong cause.

## Fix direction

- (2) `pristine-checkout` and the helpers its lanes reach resolve the
  checkout's own CLI (`nros_cli_bin --checkout-only`, which exists for gate
  preconditions, issue 1850), or refuse, naming the PATH binary they found and
  its `--version`. Never a silent fallback to rung 3. Where a helper treats "no
  CLI" as "no question to ask" (`nros_zephyr_sdk_has_board_toolchain` returns 0),
  a CLI that answers with a usage error must not count as a "no".
- (1) The setup script stops naming ~~`nros_box_publish`~~. Either it says how to
  install the box CLI on purpose, or the box env stops putting a published `nros`
  ahead of the checkout's, and a published one that does not match the checkout
  is reported, as `cli-fresh` reports an in-tree one.
- A gate that runs in a PATH-poisoned shell: a stub `nros` that fails every
  `dev` subcommand, ahead of everything on PATH, under a pristine lane. Today
  that lane goes red for an unrelated reason, with an SDK message.

## Resolution (2026-11-27)

The issue was filed by #4934; this change fixes both halves.

- **A CLI that cannot answer is not a refusal.**
  `nros_zephyr_sdk_has_board_toolchain` treats only the CLI's refusal
  (`nros dev zephyr-toolchain` exit 1) as "this SDK lacks the toolchain".
  Any other failure, clap's usage exit 2 from a binary older than the
  subcommand among them, leaves the SDK accepted. It warns, naming the
  binary and its `--version`, and says `just setup-cli`.
- **No silent fallback.** Inside a checkout whose own CLI is not built,
  `nros_cli_bin`'s rungs 3 (PATH) and 4 (the installed toolchain) still
  answer, since an installed toolchain is a real user. Each now says on
  stderr which binary it handed back and its version.
- **The box:** `ros2-distrobox-setup.sh` and the book page no longer name the
  retired ~~`nros_box_publish`~~; the setup says `just setup-cli`.
  `ros2-box-env.sh` reports a leftover published `$CARGO_INSTALL_ROOT/bin/nros`
  and its version, and leaves deleting it to the operator.
- **Gate:** `check-zephyr-sdk-env`'s selftest gained three cases, in a
  PATH-poisoned shell with a stub that fails every `dev` subcommand:
  - the pinned SDK is accepted under a CLI too old to answer, and the stub is
    named with its version;
  - a refusing CLI still refuses;
  - a pristine checkout's PATH fallback names the binary and its version.

  The old helper fails the first two and the old `nros_cli_bin` fails the
  third (mutation-checked).

