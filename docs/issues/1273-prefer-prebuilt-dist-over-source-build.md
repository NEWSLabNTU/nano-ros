---
id: 1273
title: "Tools that build from source do so because the index has no `dist` row
  for the host, not because they must be built — the fix is index rows, not
  CLI code"
status: open
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
related: [issue-1266, issue-1267, issue-1274, rfc-0014, rfc-0062]
---

## What this is

`plan_install` already prefers a prebuilt:

```rust
if Provenance::read(prefix).is_some()    { return InstallAction::Present }
if let Some(d) = tool.dist_for(host)     { return InstallAction::Prebuilt { url, sha256, install } }
if let Some(s) = &tool.source            { return InstallAction::Source { … } }
```

So a source build never means "this tool must be compiled". It means **the
index has no `dist.<host>` row for it**. 15 tools carry one. The two that built
from source during a contained-runner bootstrap were `espflash` and
`play_launch_parser` — and each has a different right answer.

No CLI change is needed for either. This is index and release work.

## espflash — point at the upstream asset

`[tool.espflash]` carries `upstream = "v4.5.0"` (the esp-rs release tag) and a
`[tool.espflash.source]` recipe, and no dist. esp-rs publishes release binaries.
A `dist.linux-x86_64 = { url, sha256 }` naming the upstream asset directly is
all it needs.

That pattern is already in the tree with its rationale written down, on
`[tool.zephyr-sdk]`:

> UNLIKE every other `[tool.*]` here, these URLs are UPSTREAM rather than
> repackaged into NEWSLabNTU/nano-ros-sdk. Deliberate: the archive is 1.3 GiB
> and we apply nothing to it, so a repack would cost a release asset per host
> to add zero value.

Two consequences that entry also records and a new one must respect: the
archive's compression decides whether `sdk_store.rs`'s `zstd` preflight applies,
and an upstream layout that is not `<prefix>/bin` needs the dist's own `install`
step.

**Survey the other source-only tools the same way** rather than fixing only the
two this bootstrap happened to build. `cargo-nextest`, `sccache`, `mdbook`,
`rustfilt`, `cargo-llvm-cov` and `cargo-show-asm` all publish release binaries
upstream; each needs the same one-line row, or a recorded reason why not.

## play_launch_parser — we build the dist, on our schedule

DECIDED (2026-09-10): building it is fine, and we publish the artifact
ourselves. `play_launch` has its own release cycle, so waiting on an upstream
asset is waiting on somebody else's calendar for a repo we own.

So: build it once, publish the dist into `NEWSLabNTU/nano-ros-sdk` the way the
other repacked tools are published, and add the `dist.linux-x86_64` row. The
index's `upstream = "838ce948…"` pin stays the record of what was built.

## NOT cargo-binstall

DECIDED (2026-09-10): not for now.

It was considered and the reason to decline is worth keeping, because it will
come up again. binstall resolves at RUN time from crates.io metadata and GitHub
releases. The index's contract is the opposite of that: a pinned `url` plus
`sha256`, `verify_sha256` before unpack, a `Provenance` marker written at the
prefix, and a recorded `nros-sdk.lock` — the "locked in nros-sdk.lock" line
every successful `nros setup` prints IS that promise. A binstall call inside
`execute_install` would fetch something nobody pinned and nothing verified.

It would also be a THIRD acquisition path beside `dist` and `source`, which is
what `check-one-producer-per-tool` exists to refuse — it refused an
apt-installed `make`/`ninja` in the runner image for exactly this reason
(PR #842).

If it returns, the shape that keeps both properties is binstall used
MAINTAINER-SIDE and offline, as the thing that MINTS a dist row: resolve the
asset, hash it, write `url + sha256` into the index. Same speed at install time,
pin preserved, no new runtime path.

## Why it is worth doing

A source build is not just slower than a download — it takes the whole machine
while the network sits idle, which is issue 1267's ceiling on any parallel
install. Removing a source build removes a serialisation point, not only
minutes.
