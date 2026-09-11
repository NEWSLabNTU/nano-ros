---
id: 1304
title: "An installed `nros` cannot run `nros setup native`: every source the board
  needs is a SUBMODULE, and a release has no checkout to read a gitlink from"
status: open
type: bug
area: cli, provisioning, release
related: [rfc-0099, phase-447, rfc-0097, issue-0204]
---

## Problem

On a pristine host that got `nros` from a release (`install.sh`), the book's
next step fails:

```text
$ nros setup native --rmw cyclonedds
nros setup: native (rmw cyclonedds) needs 3 package(s):
  cyclonedds             prebuilt 0.10.5-nros1 (dist linux-x86_64)
fatal: not a git repository (or any of the parent directories): .git
Error: provision source cyclonedds-src
   0: read gitlink sha for third-party/dds/cyclonedds (source cyclonedds-src)
   1: `git -C /root/.nros/fetch ls-tree HEAD third-party/dds/cyclonedds` failed (exit status: 128)
```

Every `[source.*]` row `native` pulls in — `zenoh-pico`, `mbedtls`,
`cyclonedds-src` — declares only `submodule = "<path>"` and a checkout-relative
`dest`. It carries no URL and no pin of its own: its provisioning is DEFINED as
"the checkout's `.gitmodules` + gitlink". An installed toolchain has neither.
The SDK root a release ships (phase-447 A1, `scripts/stage-sdk-root.sh`) is a
`git archive`, which drops gitlinks by design, so the pins are not in the asset
either. `-C` points wherever the "workspace" resolved to — measured as
`~/.nros/fetch` (the fetched index) on one run and the prefix's own
`share/nros` (the shipped index) on another, with the same asset — and `git`
correctly says neither is a repository.

So the installed journey `install.sh` -> `nros setup` -> `nros new` -> build
still dead-ends — one step EARLIER than RFC-0099 D1 described, and for every
RMW, not only Cyclone.

## How it surfaced

phase-447 A3's installed-track probe (`just probe installed`) — the first run
of the user's path on a machine with no checkout. It reproduces identically
with the phase-447 A1+A2 commit reverted, so it is independent of that work.

## What else is behind it (measured, on the same snapshot)

Skipping `nros setup` to see the rest of the path:

1. **No Rust toolchain.** Configure stops in Corrosion's `FindRust`: `rustc` not
   found. Nothing on the installed path installs one — `bootstrap.sh` does it
   for a contributor, and `installation.md` advertises the release as "no
   checkout, no cargo, no `just`", but the runtime a project links is compiled
   from source by corrosion. `nros setup` only prints a `rustup toolchain
   install` remedy.
2. **Cyclone not found even with its dist installed.** With rustup supplied,
   configure resolves the SDK root out of the release (A1+A2 work) and then
   stops in `ProvideCycloneDDS.cmake`: "CycloneDDS not found and no source to
   build it from". The aborted `nros setup` had already unpacked the
   `cyclonedds` dist into `~/.nros/sdk/cyclonedds/0.10.5-nros1`; nothing tells
   the configure to look there, and the fallback is the submodule above.
3. **The scaffold's own advice names a checkout.** `nros new --workspace`
   prints `cmake -S . -B build -DNANO_ROS_ROOT=<path-to-nano-ros>`, which an
   installed user has no value for — and does not need, since the scaffolded
   CMakeLists asks `nros sdk-root`.

## Fix (not decided here)

The shape is RFC-0099 D2's own argument, one level down: what a build needs
ships with the toolchain. Two candidates, and the choice is a design decision:

- **Carry the pins.** Record `url` + gitlink SHA for every submodule-backed
  source into the release (manifest or a staged table), and give the
  `[source.*]` submodule arm a clone-at-SHA branch when there is no checkout.
  Keeps the asset small; needs network at `nros setup`, like every dist.
- **Stage the sources.** Put the pinned trees into `share/nano-ros` at release
  time. Offline-capable; costs asset size (Cyclone alone is large) and makes
  the release carry third-party source.

Items 1–3 are separate and smaller: a Rust-toolchain step (or a probe) on the
installed path, a configure that finds an installed `cyclonedds` dist, and
scaffold "next steps" that follow the same ladder the CMakeLists does.

## Acceptance

`just probe installed` passes: install -> `nros setup native --rmw cyclonedds`
-> `nros new my_robot --workspace` -> `cmake` -> the entry prints `Published:`
and `Received:`, with no checkout on the host. That probe is the gate; it is red
on this issue today.

## Work in progress (2026-09-11) — pick up here

A fix is **written but not finished** on branch
`work/1304-installed-setup-provisions-without-a-checkout` (no PR). Its agent
stopped at an API session limit mid-change; the tree was committed as-is so it
would not be lost. Treat it as a draft: **unreviewed, and `just ci gate` has not
been run on it.** Pre-push `check-fast` is green on its head (`6cdb02124`).

Commits on the branch, oldest first:

- `b7ec5e55a` — phase-447 A3's probe. Already on `main` via #925; it drops by
  patch-id on rebase. Check the commit count after rebasing.
- `4c5b1b0ca` `wip(#1304)` — the fix. It takes the **"carry the pins"**
  candidate above, plus items 1–3:
  - **Pins:** `scripts/stage-sdk-root.sh` records url + gitlink SHA for every
    submodule-backed source into `nros-submodule-pins.toml` in the SDK root.
    `sdk_store.rs` (`recorded_pin`, `provision_at_recorded_pin`) clones at that
    SHA when there is no checkout, and skips a source that is already at its
    pin. `check-release-manifest.py` is extended to match.
  - **Item 1, Rust:** a new `orchestration/rust_toolchain.rs`, and
    `[rust.rustup]` in `nros-sdk-index.toml`, which pins a sha256-verified
    `rustup-init` for each host. `nros setup` runs it only when neither rustup
    nor a `rustc`+`cargo` pair is found. A host that already has rustup keeps
    its own settings.
  - **Item 2, Cyclone:** `nros-rmw-provision.cmake` asks `nros sdk-path` for
    the prefix `nros setup` already unpacked. It does not glob the store
    (issue 0625).
  - **Item 3, scaffold:** `workspace_scaffold.rs` no longer prints
    `-DNANO_ROS_ROOT` to an installed user.
- `2f2707639` `wip(#1304)` — the agent's uncommitted tree at the stop. It adds
  `cmake/NanoRosRustTool.cmake` (`nros_rust_tool`): the `cargo`/`rustc` that
  nano-ros's own custom commands run. It uses the name when that resolves on
  PATH, which keeps the `--locked` shim. Otherwise it uses the rustup proxies
  in `$CARGO_HOME/bin` / `~/.cargo/bin`, because an installed host has no
  PATH edit, and the build died at 92 % in the first message package's FFI
  glue. It is applied in codegen, generate-interfaces, NuttX, the RTOS
  helpers, the Zephyr cargo build, and the cpp multi-node template and test.
  **This is the part most likely to be incomplete.**
- `6cdb02124` — `cargo-target-spelling`'s "no triple available" arm now hides
  `$HOME`/`$CARGO_HOME` as well as PATH. The fallback above found rustc
  through them, so the must-fail configure succeeded.

To resume: rebase onto `main` and squash the two `wip` commits. The branch also
edits this file and the phase-447 doc, so drop this section in that rebase.
Then run `just ci gate`, and `just probe installed` for acceptance (below).
Then open the PR and arm it.
