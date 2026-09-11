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
