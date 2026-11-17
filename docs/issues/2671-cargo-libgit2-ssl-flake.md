---
id: 2671
title: "Cargo's libgit2 transport fails a git dependency fetch with `SSL error: unknown error` where the git CLI fetches the same URL cleanly"
status: open
type: bug
severity: medium
area: [build, ci, tooling]
related: [issue-2475, issue-2592]
found: 2026-11-17
---

## Problem

Every nano-ros crate depends on `ros-launch-manifest` through a git tag
(`https://github.com/NEWSLabNTU/ros-launch-manifest.git`). By default cargo
fetches git dependencies with its built-in libgit2. On host 118 that fetch
failed twice on 2026-11-17, with the same error:

```
failed to get `ros-launch-manifest-model` as a dependency of package `nros-macros v0.6.0 (…/packages/core/nros-macros)` (root cause: SSL error: unknown error; class=Ssl (16))
```

1. **On the host**, in a `just ci gate` run: the `template-copy-out` gate's
   `nros new … --kind runner` copy failed in sync's metadata harness. The
   whole gate passed when it was run again.
2. **In the runner container** `nano-ros-runner-newslab-118-server` (the
   image rebuilt that morning), in `merge-runtime` run 44030478812's zephyr
   shard (job 133860941623): `just build merge` stopped in sync's metadata
   harness, before any cell ran. That made the run void as evidence for
   issues 2592/2113/2511.

The network was not the cause. Right after the second failure, `git
ls-remote https://github.com/NEWSLabNTU/ros-launch-manifest.git HEAD`
passed 3 of 3 on the host and 3 of 3 inside the container. The failure is in
libgit2's TLS, intermittently, and the git CLI does not show it.

## Fix

Cargo has a built-in switch for exactly this, `net.git-fetch-with-cli`, also
spelled `CARGO_NET_GIT_FETCH_WITH_CLI`, which makes it shell out to `git`. It
is a transport choice, not a retry. It is set where the project injects
environment for every caller: `activate.sh` and `activate.fish`, which every
CI job sources (`merge-runtime`, `run-matrix`, `gate` and `queue` all do), so
cmake's and Corrosion's cargo calls inherit it too. A value the caller already
exported is kept.

## Not covered

- An installed `nros` run from a shell that never sourced `activate.sh` still
  uses libgit2. A user project outside a checkout has no activate file.
  Setting the variable on the CLI's own cargo commands would reach that case,
  and is a separate change if this shows up there.
- The runner image itself does not set it. The job sources `activate.sh`,
  which is enough, and changing the image means a runner rebuild.

## Done when

A zephyr `merge-runtime` shard and a `just ci gate` run on a `main` that
carries the change get past the metadata harness without a libgit2 error. If
the error comes back with the CLI transport, this issue's diagnosis is wrong
and it reopens with the new evidence.
